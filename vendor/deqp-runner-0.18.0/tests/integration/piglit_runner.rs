use super::*;
use ::deqp_runner::RunnerResults;
use anyhow::{Context, Result};
use std::process::{Command, Stdio};

/// Builder for a mocked piglit-runner invocation
#[derive(Default)]
struct PiglitMock {
    baselines: Vec<tempfile::TempPath>,
    skips: Vec<tempfile::TempPath>,
    flakes: Vec<tempfile::TempPath>,
    runner_args: Vec<String>,
}

pub fn temp_piglit<S: AsRef<str>>(runner: &str, tests: Vec<S>) -> Result<tempfile::TempDir> {
    let temp_dir = tempfile::tempdir().context("Creating output dir")?;

    let tests_dir = temp_dir.path().join("tests");
    std::fs::create_dir(&tests_dir).context("creating tests")?;

    let mut profile = std::fs::File::create(tests_dir.join("test_profile.xml"))
        .context("creating temp profile")?;

    || -> Result<()> {
        writeln!(profile, "<?xml version='1.0' encoding='utf-8'?>")?;
        writeln!(profile, "<PiglitTestList>")?;

        for test in tests {
            writeln!(profile, r#"  <Test type="gl" name="{name}">"#, name=test.as_ref())?;
            writeln!(profile, r#"    <option name="command" value="['{runner}', 'mock-piglit', '{name}', '--']" type="gl"/>"#, runner=runner, name=test.as_ref())?;
            writeln!(profile, r#"    <option name="run_concurrent" value="True"/>"#)?;
            writeln!(profile, "  </Test>")?;
        }
        writeln!(profile, "</PiglitTestList>")?;
        Ok(())
    }().context("writing profile")?;

    Ok(temp_dir)
}

impl PiglitMock {
    pub fn new() -> PiglitMock {
        Default::default()
    }

    pub fn run<S: AsRef<str>>(&self, tests: Vec<S>) -> Result<RunnerCommandResult> {
        let output_dir = tempfile::tempdir().context("Creating output dir")?;

        // Get the location of our piglit-runner binary from rustc
        let piglit_runner = env!("CARGO_BIN_EXE_piglit-runner");

        let piglit_dir =
            temp_piglit(piglit_runner, tests).context("creating temporary piglit profile")?;

        let mut cmd = Command::new(piglit_runner);
        let child = cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        let child = child.arg("run");

        let child = child.arg("--profile");
        let child = child.arg("test_profile");

        let child = child.arg("--piglit-folder");
        let child = child.arg(piglit_dir.path());

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

        for arg in &self.runner_args {
            child.arg(arg);
        }

        child.arg("-j");
        child.arg("3");

        let output = child
            .spawn()
            .with_context(|| format!("Spawning {:?}", piglit_runner))?
            .wait_with_output()
            .context("waiting for deqp-runner")?;

        let results_path = output_dir.path().to_owned().join("results.csv");
        let results = std::fs::File::open(&results_path)
            .with_context(|| format!("opening {:?}", &results_path))
            .and_then(|mut f| RunnerResults::from_csv(&mut f).context("reading results.csv"));

        let output_dir_contents = dir_contents(output_dir.path());

        // Debug knob, flip it to save the output dirs so you can look into why things failed.
        if false {
            let _ = piglit_dir.into_path();
            let _ = output_dir.into_path();
        } else {
            let _ = piglit_dir.close();
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

    pub fn with_baseline<S: AsRef<str>>(&mut self, data: S) -> &mut PiglitMock {
        self.baselines
            .push(tempfile(data).context("writing baselines").unwrap());
        self
    }

    pub fn with_runner_arg(&mut self, arg: &str) -> &mut PiglitMock {
        self.runner_args.push(arg.into());
        self
    }
}

#[test]
fn many_passes() {
    let mut tests = Vec::new();
    for i in 0..1000 {
        tests.push(format!("piglit@{}@pass", i));
    }

    let result = PiglitMock::new().run(tests).unwrap();
    assert_eq!(result.stderr, "");
    assert_eq!(result.status.code(), Some(0));
    assert!(result.stdout.contains("Pass: 1000"));
    assert_eq!(result.results.unwrap().result_counts.pass, 1000);
}

#[test]
fn timeout() {
    let result = PiglitMock::new().run(vec!["piglit@timeout"]).unwrap();
    assert!(result.stderr.contains("piglit@timeout"));
    assert_eq!(result.status.code(), Some(1));
    assert!(result.stdout.contains("Timeout: 1"));
    assert_eq!(result.results.unwrap().result_counts.timeout, 1);
}

/// Test detection of a crash after piglit reported a top-level result.
#[test]
fn late_crash() {
    let result = PiglitMock::new().run(vec!["piglit@late_crash"]).unwrap();
    assert!(result.stderr.contains("piglit@late_crash"));
    assert_eq!(result.status.code(), Some(1));
    assert!(result.stdout.contains("Crash: 1"));
    assert_eq!(result.results.unwrap().result_counts.crash, 1);
}

#[test]
fn missing_skips() {
    let results = PiglitMock::new()
        .with_runner_arg("--skips")
        .with_runner_arg("/does-not-exist.txt")
        .run(vec!["piglit@pass"])
        .unwrap();
    assert_eq!(Some(1), results.status.code());
    println!("{}", results.stderr);
}

#[test]
fn missing_flakes() {
    let results = PiglitMock::new()
        .with_runner_arg("--flakes")
        .with_runner_arg("/does-not-exist.txt")
        .run(vec!["piglit@pass"])
        .unwrap();
    assert_eq!(Some(1), results.status.code());
    println!("{}", results.stderr);
}

#[test]
fn includes() {
    let results = PiglitMock::new()
        .with_runner_arg("-t")
        .with_runner_arg("piglit@p.*")
        .run(vec!["piglit@pass@1", "piglit@fail@2"])
        .unwrap()
        .results
        .unwrap();
    assert_eq!(results.result_counts.pass, 1);
    assert_eq!(results.result_counts.fail, 0);
    assert_eq!(results.result_counts.total, 1);
}

#[test]
fn bad_includes() {
    let results = PiglitMock::new()
        .with_runner_arg("-t")
        .with_runner_arg("*")
        .run(vec!["piglit@pass"])
        .unwrap();
    assert_eq!(Some(1), results.status.code());
}

/// Test detection of a crash after piglit reported a top-level result.
#[test]
fn subtest_statuses() {
    let result = PiglitMock::new()
        .run(vec!["piglit@subtest_statuses"])
        .unwrap();
    assert!(!result.stderr.contains("piglit@subtest_statuses@p"));
    assert!(!result.stderr.contains("piglit@subtest_statuses@s"));
    assert!(result.stderr.contains("piglit@subtest_statuses@f"));
    assert_eq!(result.status.code(), Some(1));

    // One each for the subtests, plus the top-level fail result.
    assert!(result.stdout.contains("Pass: 1"));
    assert!(result.stdout.contains("Fail: 2"));
    assert!(result.stdout.contains("Skip: 1"));

    let result_counts = result.results.unwrap().result_counts;
    assert_eq!(result_counts.pass, 1);
    assert_eq!(result_counts.fail, 2);
    assert_eq!(result_counts.skip, 1);
}

/// Test detection of a crash after piglit reported a top-level result.
#[test]
fn subtest_baseline() {
    let result = PiglitMock::new()
        .with_baseline(
            "
piglit@subtest_statuses@f,Fail
piglit@subtest_statuses,Fail",
        )
        .run(vec!["piglit@subtest_statuses"])
        .unwrap();
    assert!(!result.stderr.contains("piglit@subtest_statuses@p"));
    assert!(!result.stderr.contains("piglit@subtest_statuses@s"));
    assert!(!result.stderr.contains("piglit@subtest_statuses@f"));
    assert_eq!(result.status.code(), Some(0));

    // One each for the subtests, plus the top-level fail result.
    assert!(result.stdout.contains("Pass: 1"));
    assert!(result.stdout.contains("ExpectedFail: 2"));
    assert!(result.stdout.contains("Skip: 1"));

    let result_counts = result.results.unwrap().result_counts;
    assert_eq!(result_counts.pass, 1);
    assert_eq!(result_counts.expected_fail, 2);
    assert_eq!(result_counts.fail, 0);
    assert_eq!(result_counts.skip, 1);
}

/// Test clean handling of a duplicated subtest (a somewhat common piglit test bug)
#[test]
fn subtest_dupe() {
    let result = PiglitMock::new().run(vec!["piglit@subtest_dupe"]).unwrap();
    assert!(result.stderr.contains("piglit@subtest_dupe@subtest"));
    assert_eq!(result.status.code(), Some(1));
    assert!(result.stdout.contains("Fail: 1"));
    assert_eq!(result.results.unwrap().result_counts.fail, 1);
}

/// Test handling of piglit subtests with commas in them (can be trouble for the .csvs)
#[test]
fn subtest_commas() {
    let result = PiglitMock::new()
        .with_baseline(
            "
piglit@subtest_commas@GL_INTENSITY16- swizzled- border color only,Fail
piglit@subtest_commas,Fail",
        )
        .run(vec!["piglit@subtest_commas"])
        .unwrap();
    assert!(result
        .stderr
        .contains("piglit@subtest_commas@GL_INTENSITY12")); // not in baseline
    assert!(!result
        .stderr
        .contains("piglit@subtest_commas@GL_INTENSITY16")); // in baseline
    assert_eq!(result.status.code(), Some(1));
    assert!(result.stdout.contains("Fail: 1"));
    let results = result.results.unwrap();
    assert_eq!(results.result_counts.expected_fail, 2);
    assert_eq!(results.result_counts.fail, 1);
}
