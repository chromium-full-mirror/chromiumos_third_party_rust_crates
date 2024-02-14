use super::*;
use ::deqp_runner::RunnerResults;
use anyhow::{Context, Result};
use std::process::{Command, Stdio};

/// Builder for a mocked gtest-runner invocation
#[derive(Default)]
struct GTestMock {
    baselines: Vec<tempfile::TempPath>,
    skips: Vec<tempfile::TempPath>,
    flakes: Vec<tempfile::TempPath>,
}

impl GTestMock {
    pub fn new() -> GTestMock {
        Default::default()
    }

    pub fn run<S: AsRef<str>>(&self, includes: Vec<S>) -> Result<RunnerCommandResult> {
        let output_dir = tempfile::tempdir().context("Creating output dir")?;

        // Get the location of our gtest-runner binary from rustc
        let gtest_runner = env!("CARGO_BIN_EXE_gtest-runner");

        let mut cmd = Command::new(gtest_runner);
        let child = cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        let child = child.arg("run");

        let child = child.arg("--gtest");
        let child = child.arg(gtest_runner);

        let child = child.arg("--output");
        let child = child.arg(output_dir.path());

        let child = child.arg("--timeout");
        let child = child.arg("1");

        for baseline_file in &self.baselines {
            child.arg("--baseline");
            child.arg(baseline_file);
        }

        for skips_file in &self.skips {
            child.arg("--skips");
            child.arg(skips_file);
        }

        for flakes_file in &self.flakes {
            child.arg("--flakes");
            child.arg(flakes_file);
        }

        for include in &includes {
            child.arg("--include-tests");
            child.arg(include.as_ref());
        }

        child.arg("-j");
        child.arg("3");

        child.arg("--");
        child.arg("mock-gtest"); // Passed as the first arg of the "gtest" binary (the gtest-runner we passed as --bin!) to trigger its mock-gtest mode

        let output = child
            .spawn()
            .with_context(|| format!("Spawning {:?}", gtest_runner))?
            .wait_with_output()
            .context("waiting for gtest-runner")?;

        let results_path = output_dir.path().to_owned().join("results.csv");
        let results = std::fs::File::open(&results_path)
            .with_context(|| format!("opening {:?}", &results_path))
            .and_then(|mut f| RunnerResults::from_csv(&mut f).context("reading results.csv"));

        let output_dir_contents = dir_contents(output_dir.path());

        // Debug knob, flip it to save the output dirs so you can look into why things failed.
        if false {
            let _ = output_dir.into_path();
        } else {
            let _ = output_dir.close();
        }

        Ok(RunnerCommandResult {
            status: output.status,
            stdout: String::from_utf8(output.stdout).context("UTF-8 of stdout")?,
            stderr: String::from_utf8(output.stderr).context("UTF-8 of stderr")?,
            output_dir_contents,
            results,
        })
    }

    pub fn with_baseline<S: AsRef<str>>(&mut self, data: S) -> &mut GTestMock {
        self.baselines
            .push(tempfile(data).context("writing baselines").unwrap());
        self
    }
}

#[test]
fn basic_cases() {
    let mut gtest = GTestMock::new();

    let mut baseline = String::new();
    for i in 0..12 {
        baseline.push_str(&format!("Group1.fail/{},Fail\n", i));
    }

    let result = gtest
        .with_baseline(baseline)
        .run(vec!["pass", "fail", "skip"])
        .unwrap();
    assert_eq!(result.stderr, "");
    assert_eq!(result.status.code(), Some(0));

    assert!(result.stdout.contains("Pass: 10"));
    assert!(result.stdout.contains("Skip: 11"));
    assert!(result.stdout.contains("ExpectedFail: 12"));
    let results = result.results.unwrap();
    assert_eq!(results.result_counts.pass, 10);
    assert_eq!(results.result_counts.skip, 11);
    assert_eq!(results.result_counts.expected_fail, 12);
}

#[test]
fn timeout() {
    let result = GTestMock::new().run(vec!["timeout", "pass"]).unwrap();
    assert!(result.stderr.contains("Group2.timeout"));
    assert_eq!(result.status.code(), Some(1));
    assert!(result.stdout.contains("Timeout: 1"));
    let results = result.results.unwrap();
    assert_eq!(results.result_counts.pass, 10);
    assert_eq!(results.result_counts.timeout, 1);
}

/// Test detection of a crash after gtest reported a result.  Attributed to the
/// last test, but maybe that's not the best.
#[test]
fn crash() {
    let result = GTestMock::new().run(vec!["crash", "pass"]).unwrap();
    assert!(result.stderr.contains("Group2.crash"));
    assert_eq!(result.status.code(), Some(1));
    assert!(result.stdout.contains("Pass: 10"));
    assert!(result.stdout.contains("Crash: 1"));
    let results = result.results.unwrap();
    assert_eq!(results.result_counts.pass, 10);
    assert_eq!(results.result_counts.crash, 1);
}

#[test]
fn logs_stderr() {
    let result = GTestMock::new().run(vec!["stderr"]).unwrap();
    assert!(result.stderr.contains("Output to stderr"));
}
