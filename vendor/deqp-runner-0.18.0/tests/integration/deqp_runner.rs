use super::*;
use ::deqp_runner::{RunnerResults, RunnerStatus, TestStatus};
use anyhow::{Context, Result};
use std::borrow::Borrow;
use std::fs::File;
use std::io::{BufReader, Cursor};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

enum MaybeTempFile {
    Temp(tempfile::TempPath),
    File(PathBuf),
}

impl AsRef<Path> for MaybeTempFile {
    fn as_ref(&self) -> &Path {
        match self {
            MaybeTempFile::Temp(t) => t,
            MaybeTempFile::File(path) => path,
        }
    }
}

pub fn add_includes(cmd: &mut Command, includes: &[String]) {
    if !includes.is_empty() {
        cmd.arg("--include-tests");
        for i in includes {
            cmd.arg(i);
        }
    }
}

/// Builder for a mocked deqp-runner invocation
#[derive(Default)]
struct DeqpMock {
    pub caselists: Vec<MaybeTempFile>,
    pub baselines: Vec<tempfile::TempPath>,
    pub skips: Vec<tempfile::TempPath>,
    pub flakes: Vec<tempfile::TempPath>,
    pub runner_args: Vec<String>,
    pub prefix: String,
    pub renderer_check: String,
    pub version_check: String,
    pub extensions_check: Option<MaybeTempFile>,
    pub includes: Vec<String>,
    pub fraction: Option<u32>,
    pub fraction_start: Option<u32>,
    pub j: u32,
}

impl DeqpMock {
    pub fn new() -> DeqpMock {
        DeqpMock {
            j: 3,
            ..Default::default()
        }
    }

    pub fn run(&self) -> Result<RunnerCommandResult> {
        let output_dir = tempfile::tempdir().context("Creating output dir")?;

        assert!(self.prefix.is_empty());

        // Get the location of our deqp-runner binary from rustc
        let deqp_runner = env!("CARGO_BIN_EXE_deqp-runner");

        let mut cmd = Command::new(deqp_runner);
        let child = cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        let child = child.arg("run");

        let child = child.arg("--deqp");
        let child = child.arg(deqp_runner);

        let child = child.arg("--output");
        let child = child.arg(output_dir.path());

        let child = child.arg("--timeout");
        let child = child.arg("1");

        if let Some(fraction) = self.fraction {
            child.arg("--fraction");
            child.arg(format!("{}", fraction));
        }

        if let Some(fraction_start) = self.fraction_start {
            child.arg("--fraction-start");
            child.arg(format!("{}", fraction_start));
        }

        for caselist_file in &self.caselists {
            child.arg("--caselist");
            child.arg(caselist_file.as_ref());
        }

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

        if !self.renderer_check.is_empty() {
            child.arg("--renderer-check");
            child.arg(&self.renderer_check);
        }

        if !self.version_check.is_empty() {
            child.arg("--version-check");
            child.arg(&self.version_check);
        }

        if let Some(file) = &self.extensions_check {
            child.arg("--extensions-check");
            child.arg(file.as_ref().as_os_str());
        }

        child.arg("-j");
        child.arg(format!("{}", self.j));

        add_includes(child, &self.includes);

        for arg in &self.runner_args {
            child.arg(arg);
        }

        child.arg("--");
        child.arg("mock-deqp"); // Passed as the first arg of the "deqp" binary (the deqp-runner we passed as --deqp!) to trigger its mock-deqp mode

        let output = child
            .spawn()
            .with_context(|| format!("Spawning {:?}", deqp_runner))?
            .wait_with_output()
            .context("waiting for deqp-runner")?;

        let results_path = output_dir.path().to_owned().join("results.csv");
        let results = std::fs::File::open(&results_path)
            .with_context(|| format!("opening {:?}", &results_path))
            .and_then(|mut f| RunnerResults::from_csv(&mut f).context("reading results.csv"));

        let output_dir_contents = dir_contents(output_dir.path());

        let _ = output_dir.close();

        Ok(RunnerCommandResult {
            status: output.status,
            stdout: String::from_utf8(output.stdout).context("UTF-8 of stdout")?,
            stderr: String::from_utf8(output.stderr).context("UTF-8 of stderr")?,
            output_dir_contents,
            results,
        })
    }

    pub fn with_cases<S, I>(&mut self, lines: I) -> &mut DeqpMock
    where
        S: AsRef<str>,
        I: IntoIterator<Item = S>,
    {
        self.caselists.push(MaybeTempFile::Temp(
            lines_tempfile(lines).context("writing caselist").unwrap(),
        ));
        self
    }

    pub fn with_caselist_file(&mut self, file: &Path) -> &mut DeqpMock {
        self.caselists.push(MaybeTempFile::File(file.to_path_buf()));
        self
    }

    pub fn with_baseline(&mut self, data: impl AsRef<str>) -> &mut DeqpMock {
        self.baselines
            .push(tempfile(data).context("writing baseline").unwrap());
        self
    }

    pub fn with_skips(&mut self, data: impl AsRef<str>) -> &mut DeqpMock {
        self.skips
            .push(tempfile(data).context("writing skips").unwrap());
        self
    }

    pub fn with_j(&mut self, j: u32) -> &mut DeqpMock {
        self.j = j;
        self
    }

    pub fn with_flakes(&mut self, data: impl AsRef<str>) -> &mut DeqpMock {
        self.flakes
            .push(tempfile(data).context("writing flakes").unwrap());
        self
    }

    pub fn with_runner_arg(&mut self, arg: &str) -> &mut DeqpMock {
        self.runner_args.push(arg.into());
        self
    }

    pub fn with_prefix(&mut self, arg: &str) -> &mut DeqpMock {
        self.prefix = arg.to_owned();
        self
    }

    pub fn with_fraction(&mut self, arg: u32) -> &mut DeqpMock {
        self.fraction = Some(arg);
        self
    }

    pub fn with_fraction_start(&mut self, arg: u32) -> &mut DeqpMock {
        self.fraction_start = Some(arg);
        self
    }

    pub fn with_includes(&mut self, arg: &str) -> &mut DeqpMock {
        self.includes.push(arg.to_owned());
        self
    }

    pub fn with_renderer_check(&mut self, arg: &str) -> &mut DeqpMock {
        self.renderer_check = arg.to_owned();
        self
    }

    pub fn with_version_check(&mut self, arg: &str) -> &mut DeqpMock {
        self.version_check = arg.to_owned();
        self
    }

    pub fn with_extensions_check(&mut self, path: MaybeTempFile) -> &mut DeqpMock {
        self.extensions_check = Some(path);
        self
    }
}

fn mocked_deqp_runner<S: AsRef<str>>(tests: Vec<S>) -> RunnerResults {
    DeqpMock::new()
        .with_cases(tests)
        .run()
        .unwrap()
        .results
        .unwrap()
}

fn result_status<S: AsRef<str>>(results: &RunnerResults, test: S) -> RunnerStatus {
    results.get(test.as_ref()).unwrap().status
}

#[derive(Default)]
struct DeqpSuite {
    deqps: Vec<DeqpMock>,
    includes: Vec<String>,
}

fn write_file_list_toml<W: Write, P: AsRef<Path>>(
    file: &mut W,
    key: &str,
    files: &[P],
) -> Result<()> {
    if !files.is_empty() {
        write!(file, "{} = [", key).context("writing file list")?;
        for f in files {
            let path = f.as_ref().to_str().context("formatting path")?;
            write!(file, "\"{}\"", path.escape_default()).context("writing file list")?;
        }
        writeln!(file, "]").context("writing file list")?;
    }

    Ok(())
}

impl DeqpSuite {
    pub fn new() -> DeqpSuite {
        Default::default()
    }

    pub fn run(&self) -> Result<RunnerCommandResult> {
        let output_dir = tempfile::tempdir().context("Creating output dir")?;

        // Get the location of our deqp-runner binary from rustc
        let deqp_runner = env!("CARGO_BIN_EXE_deqp-runner");

        let mut toml = tempfile::NamedTempFile::new().context("creating toml tempfile")?;

        // Use a closure to wrap all the try operator paths with one .context().
        || -> Result<()> {
            for deqp in &self.deqps {
                writeln!(toml, "[[deqp]]")?;
                writeln!(toml, r#"deqp = "{}""#, deqp_runner.escape_default())?;

                write_file_list_toml(&mut toml, "caselists", &deqp.caselists)?;
                write_file_list_toml(&mut toml, "skips", &deqp.skips)?;
                write_file_list_toml(&mut toml, "flakes", &deqp.flakes)?;
                if !deqp.baselines.is_empty() {
                    assert!(deqp.baselines.len() == 1);
                    writeln!(
                        toml,
                        r#"baseline = "{}""#,
                        deqp.baselines[0].to_string_lossy().escape_default()
                    )?;
                }

                if !deqp.renderer_check.is_empty() {
                    writeln!(toml, r#"renderer_check = "{}""#, &deqp.renderer_check)?;
                }
                if !deqp.version_check.is_empty() {
                    writeln!(toml, r#"version_check = "{}""#, &deqp.version_check)?;
                }
                if let Some(path) = &deqp.extensions_check {
                    writeln!(toml, r#"extensions_check = "{:?}""#, path.as_ref())?;
                }

                if !deqp.includes.is_empty() {
                    write!(toml, r#"include = ["#,)?;
                    for i in &deqp.includes {
                        write!(toml, r#""{}", "#, i)?;
                    }
                    writeln!(toml, "]")?;
                }
                // Passed as the first arg of the "deqp" binary (the deqp-runner we passed as "deqp ="!) to trigger its mock-deqp mode
                writeln!(toml, r#"deqp_args = ["mock-deqp"]"#)?;

                //for arg in &deqp.runner_args {
                //    child.arg(arg);
                //}

                writeln!(toml, "timeout = 1.0")?;

                if !deqp.prefix.is_empty() {
                    writeln!(toml, r#"prefix = "{}""#, deqp.prefix)?;
                }

                writeln!(toml)?;
            }
            Ok(())
        }()
        .context("writing toml file")?;

        //println!("toml: {}", std::fs::read_to_string(toml.path()).context("readback")?);

        let mut cmd = Command::new(deqp_runner);
        let child = cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        let child = child.arg("suite");

        let child = child.arg("--output");
        let child = child.arg(output_dir.path());

        let child = child.arg("--suite");
        let child = child.arg(toml.path());

        add_includes(child, &self.includes);

        let output = child
            .spawn()
            .with_context(|| format!("Spawning {:?}", deqp_runner))?
            .wait_with_output()
            .context("waiting for deqp-runner")?;

        let results_path = output_dir.path().to_owned().join("results.csv");
        let results = std::fs::File::open(&results_path)
            .with_context(|| format!("opening {:?}", &results_path))
            .and_then(|mut f| RunnerResults::from_csv(&mut f).context("reading results.csv"));

        let output_dir_contents = dir_contents(output_dir.path());

        let _ = output_dir.close();

        Ok(RunnerCommandResult {
            status: output.status,
            stdout: String::from_utf8(output.stdout).context("UTF-8 of stdout")?,
            stderr: String::from_utf8(output.stderr).context("UTF-8 of stderr")?,
            output_dir_contents,
            results,
        })
    }

    pub fn with_deqp(&mut self, deqp: DeqpMock) -> &mut DeqpSuite {
        self.deqps.push(deqp);
        self
    }

    pub fn with_includes(&mut self, arg: &str) -> &mut DeqpSuite {
        self.includes.push(arg.to_owned());
        self
    }
}

#[test]
fn many_passes() {
    let mut tests = Vec::new();
    for i in 0..1000 {
        tests.push(format!("dEQP-GLES2.test.p.{}", i));
    }

    let result = DeqpMock::new().with_cases(tests).run().unwrap();
    assert_eq!(result.stderr, "");
    assert_eq!(result.status.code(), Some(0));
    assert!(result.stdout.contains("Pass: 1000"));
    assert_eq!(result.results.unwrap().result_counts.pass, 1000);

    /* No logs should have been saved, since everything passed. */
    assert!(!result
        .output_dir_contents
        .iter()
        .any(|x| x.ends_with(".log")));
}

#[test]
fn many_passes_and_a_fail() {
    let mut tests = Vec::new();
    for i in 0..1000 {
        tests.push(format!("dEQP-GLES2.test.p.{}", i));
    }
    tests.push("dEQP-GLES2.test.f.foo".to_string());

    let results = mocked_deqp_runner(tests);
    assert_eq!(results.result_counts.pass, 1000);
    assert_eq!(results.result_counts.fail, 1);
    assert_eq!(
        result_status(&results, "dEQP-GLES2.test.f.foo"),
        RunnerStatus::Fail
    );
}

#[test]
fn crash() {
    let mut tests = Vec::new();
    for i in 0..50 {
        tests.push(format!("dEQP-GLES2.test.{}.p.foo", i));
    }

    tests.push("dEQP-GLES2.test.50.c.foo".to_string());

    for i in 51..100 {
        tests.push(format!("dEQP-GLES2.test.{}.p.foo", i));
    }

    let results = mocked_deqp_runner(tests);
    assert_eq!(results.result_counts.pass, 99);
    assert_eq!(results.result_counts.crash, 1);
    assert_eq!(
        result_status(&results, "dEQP-GLES2.test.50.c.foo"),
        RunnerStatus::Crash
    );
}

#[test]
fn timeout() {
    let mut tests = Vec::new();
    for i in 0..20 {
        if i % 7 == 1 {
            tests.push(format!("dEQP-GLES2.test.{}.timeout.foo", i));
        } else {
            tests.push(format!("dEQP-GLES2.test.{}.p.foo", i));
        }
    }

    let results = mocked_deqp_runner(tests);
    assert_eq!(results.result_counts.pass, 17);
    assert_eq!(results.result_counts.timeout, 3);
    assert_eq!(
        result_status(&results, "dEQP-GLES2.test.1.timeout.foo"),
        RunnerStatus::Timeout
    );
}

// Tests a run with a flakes list like we might actually write in Mesa CI
#[test]
fn flake_handling() {
    let mut tests = Vec::new();
    for i in 0..100 {
        tests.push(format!("dEQP-GLES2.test.p.{}", i));
    }
    for i in 0..2 {
        tests.push(format!("dEQP-GLES2.test.flaky.{}", i));
    }

    {
        // Verify that our mocked flaky test actually flakes, and that
        // the default flake handling detects it!
        let mut found_pass = false;
        let mut found_fail = false;
        let mut found_flake = false;
        while !(found_fail && found_pass && found_flake) {
            let results = mocked_deqp_runner(tests.clone());
            match result_status(&results, "dEQP-GLES2.test.flaky.0") {
                RunnerStatus::Pass => found_pass = true,
                RunnerStatus::Fail => found_fail = true,
                RunnerStatus::Flake => found_flake = true,
                _ => unreachable!("bad test result"),
            }
        }
    }

    {
        // Verify that we can handle known flakes
        let mut found_flake = false;
        let mut found_pass = false;
        let mut found_xfail = false;
        while !(found_flake && found_pass && found_xfail) {
            let result = DeqpMock::new()
                .with_flakes("dEQP-GLES2.test.flaky.*\n")
                .with_baseline("dEQP-GLES2.test.flaky.1,Fail")
                .with_cases(tests.clone())
                .run()
                .unwrap();

            let results = result.results.unwrap();
            match result_status(&results, "dEQP-GLES2.test.flaky.0") {
                RunnerStatus::Pass => found_pass = true,
                RunnerStatus::KnownFlake => {
                    found_flake = true;
                    assert!(results.result_counts.flake >= 1);
                }
                _ => unreachable!("bad test result"),
            }

            match result_status(&results, "dEQP-GLES2.test.flaky.1") {
                RunnerStatus::ExpectedFail => found_xfail = true,
                RunnerStatus::KnownFlake => {
                    found_flake = true;
                    assert!(results.result_counts.flake >= 1);
                    assert!(
                        result.stdout.contains("known flakes found"),
                        "stdout missing known flakes report:\n{}",
                        result.stdout
                    );
                }
                _ => unreachable!("bad test result"),
            }
        }
    }
}

// Tests a run with a xfail line accidentally pasted as a skips line.
#[test]
fn skip_has_status_pasted() {
    let mut tests = Vec::new();
    for i in 0..10 {
        tests.push(format!("dEQP-GLES2.test.p.{}", i));
    }

    let result = DeqpMock::new()
        .with_skips(
            "
# Pasted a result line as a skip, so when parsed as a regex it won't actually skip!
# deqp-runner should catch this and print something nice.
dEQP-GLES2.test.c.0,Fail
",
        )
        .with_cases(tests)
        .run()
        .unwrap();

    assert!(
        result.status.code() != Some(0) && result.stderr.contains("regex line appears to incorrectly contain a result status: dEQP-GLES2.test.c.0,Fail"),
        "didn't catch xfail line pasted as skip, exited with {:?} and stdout/stderr shows:\n{}\n{}",
        result.status.code(),
        result.stdout,
        result.stderr
    );
}

#[test]
fn baseline() {
    let mut tests = Vec::new();
    for i in 0..10 {
        tests.push(format!("dEQP-GLES2.test.p.{}", i));
    }
    for i in 0..4 {
        tests.push(format!("dEQP-GLES2.test.f.{}", i));
    }
    for i in 0..2 {
        tests.push(format!("dEQP-GLES2.test.c.{}", i));
    }

    let results = DeqpMock::new()
        .with_baseline(
            "
dEQP-GLES2.test.p.1,Fail
dEQP-GLES2.test.f.2,Fail
dEQP-GLES2.test.f.3,Fail
dEQP-GLES2.test.c.1,Crash",
        )
        .with_cases(tests)
        .run()
        .unwrap()
        .results
        .unwrap();

    assert_eq!(results.result_counts.pass, 9);
    assert_eq!(results.result_counts.unexpected_improvement, 1);
    assert_eq!(results.result_counts.crash, 1);
    assert_eq!(results.result_counts.fail, 2);
    assert_eq!(results.result_counts.expected_fail, 3);
}

#[test]
fn missing() {
    let mut tests = Vec::new();
    for i in 0..100 {
        tests.push(format!("dEQP-GLES2.test.p.{}", i));
    }
    tests.push("dEQP-GLES2.test.m.foo".to_string());

    let command_results = DeqpMock::new().with_cases(tests).run().unwrap();

    let results = command_results.results.unwrap();
    assert_eq!(results.result_counts.pass, 100);
    assert_eq!(results.result_counts.missing, 1);
    assert_eq!(
        result_status(&results, "dEQP-GLES2.test.m.foo"),
        RunnerStatus::Missing
    );

    assert!(command_results
        .output_dir_contents
        .iter()
        .any(|x| x.ends_with(".log")));
}

// Tests round-tripping some results through csv formatting.
#[test]
fn results_serialization() {
    let mut tests = Vec::new();
    for i in 0..50 {
        tests.push(format!("dEQP-GLES2.test.p.{}", i));
    }
    for i in 0..30 {
        tests.push(format!("dEQP-GLES2.test.f.{}", i));
    }
    for i in 0..20 {
        tests.push(format!("dEQP-GLES2.test.s.{}", i));
    }
    for i in 0..10 {
        tests.push(format!("dEQP-GLES2.test.m.{}", i));
    }
    tests.push("dEQP-GLES2.test.c.foo".to_string());
    let results = mocked_deqp_runner(tests);

    let mut results_file = Cursor::new(Vec::new());
    results.write_results(&mut results_file).unwrap();
    results_file.set_position(0);
    let read_results = RunnerResults::from_csv(&mut results_file).unwrap();
    assert_eq!(results.result_counts, read_results.result_counts);

    let mut results_file = Cursor::new(Vec::new());
    results.write_failures(&mut results_file).unwrap();
    results_file.set_position(0);
    let read_results = RunnerResults::from_csv(&mut results_file).unwrap();
    assert_eq!(0, read_results.result_counts.pass);
    assert_eq!(0, read_results.result_counts.skip);
    assert_eq!(results.result_counts.fail, read_results.result_counts.fail);
    assert_eq!(
        results.result_counts.crash,
        read_results.result_counts.crash
    );
}

#[test]
fn missing_skips() {
    let results = DeqpMock::new()
        .with_runner_arg("--skips")
        .with_runner_arg("/does-not-exist.txt")
        .with_cases(vec!["dEQP-GLES2.test.p.1"])
        .run()
        .unwrap();
    assert_eq!(Some(1), results.status.code());
    println!("{}", results.stderr);
}

#[test]
fn missing_flakes() {
    let results = DeqpMock::new()
        .with_runner_arg("--flakes")
        .with_runner_arg("/does-not-exist.txt")
        .with_cases(vec!["dEQP-GLES2.test.p.1"])
        .run()
        .unwrap();
    assert_eq!(Some(1), results.status.code());
    println!("{}", results.stderr);
}

#[test]
fn includes() {
    let results = DeqpMock::new()
        .with_includes("dEQP-GLES2.test.p.*")
        .with_cases(vec!["dEQP-GLES2.test.p.1", "dEQP-GLES2.test.f.2"])
        .run()
        .unwrap()
        .results
        .unwrap();
    assert_eq!(results.result_counts.pass, 1);
    assert_eq!(results.result_counts.fail, 0);
    assert_eq!(results.result_counts.total, 1);
}

#[test]
fn caselist_subdir() {
    let caselist = test_resource_path("caselist-subdir.txt");

    let results = DeqpMock::new().with_caselist_file(&caselist).run().unwrap();

    println!("stdout: {}", results.stdout);
    println!("stderr: {}", results.stderr);
    let results = results.results.unwrap();

    assert_eq!(results.result_counts.pass, 2);
    assert_eq!(results.result_counts.fail, 0);
    assert_eq!(results.result_counts.total, 2);
    assert_eq!(
        result_status(&results, "dEQP-GLES2.p.1"),
        RunnerStatus::Pass
    );
    assert_eq!(
        result_status(&results, "dEQP-GLES2.p.2"),
        RunnerStatus::Pass
    );
}

#[test]
fn bad_includes() {
    let results = DeqpMock::new()
        .with_includes("*")
        .with_cases(vec!["dEQP-GLES2.test.p"])
        .run()
        .unwrap();
    assert_eq!(Some(1), results.status.code());
}

/// Make sure that we nicely report fail -> skip transitions
#[test]
fn fail_to_skip() {
    let result = DeqpMock::new()
        .with_cases(vec!["dEQP-GLES2.test.s.1"])
        .with_baseline("dEQP-GLES2.test.s.1,Fail")
        .run()
        .unwrap();
    assert_eq!(Some(1), result.status.code());
    assert!(
        result
            .stdout
            .contains("dEQP-GLES2.test.s.1,UnexpectedImprovement(Skip)"),
        "skip improvement not reported in stdout:\n{}",
        result.stdout,
    );
    assert_eq!(
        result.results.unwrap().result_counts.unexpected_improvement,
        1
    );
}

#[test]
fn suite_pass() {
    let mut deqp1 = DeqpMock::new();
    deqp1.with_cases(vec!["dEQP-GLES2.test.p.1"]);
    let mut deqp2 = DeqpMock::new();
    deqp2.with_cases(vec!["dEQP-GLES2.test.p.2"]);
    let results = DeqpSuite::new()
        .with_deqp(deqp1)
        .with_deqp(deqp2)
        .run()
        .unwrap();
    assert_eq!(Some(0), results.status.code());
    assert_eq!(results.results.unwrap().result_counts.pass, 2);
}

#[test]
fn suite_fail() {
    let mut deqp1 = DeqpMock::new();
    deqp1.with_cases(vec!["dEQP-GLES2.test.p.1"]);
    let mut deqp2 = DeqpMock::new();
    deqp2.with_cases(vec!["dEQP-GLES2.test.f.2"]);
    let results = DeqpSuite::new()
        .with_deqp(deqp1)
        .with_deqp(deqp2)
        .run()
        .unwrap();
    assert_eq!(Some(1), results.status.code());
    let counts = results.results.unwrap().result_counts;
    assert_eq!(counts.pass, 1);
    assert_eq!(counts.fail, 1);
    assert!(
        results.stdout.contains("dEQP-GLES2.test.f.2,Fail"),
        "stdout missed failure printout:\n{}",
        results.stdout
    );
}

// Same-named test between deqps should be a fail since you can't distinguish them.
#[test]
fn suite_dupe_test() {
    let mut deqp1 = DeqpMock::new();
    deqp1.with_cases(vec!["dEQP-GLES2.test.p.1"]);
    let mut deqp2 = DeqpMock::new();
    deqp2.with_cases(vec!["dEQP-GLES2.test.p.1"]);
    let results = DeqpSuite::new()
        .with_deqp(deqp1)
        .with_deqp(deqp2)
        .run()
        .unwrap();
    println!("{}", results.stdout);
    println!("{}", results.stderr);
    assert_eq!(Some(1), results.status.code());
    let counts = results.results.unwrap().result_counts;
    assert!(results.stdout.contains("Pass: 1, Fail: 1"));
    assert_eq!(counts.fail, 1);
    // Since our counts come from reloading the csv, we won't see the first pass of the test.
    assert_eq!(counts.pass, 0);
}

// Using a prefix to distinguish the same test
#[test]
fn suite_prefix_test() {
    let mut deqp1 = DeqpMock::new();
    deqp1.with_cases(vec!["dEQP-GLES2.test.p.1"]);
    let mut deqp2 = DeqpMock::new();
    deqp2.with_cases(vec!["dEQP-GLES2.test.p.1", "dEQP-GLES2.test.p.2"]);
    deqp2.with_skips("variant1-dEQP-GLES2.*.p.2");
    deqp2.with_prefix("variant1-");
    let mut deqp3 = DeqpMock::new();
    deqp3.with_cases(vec!["dEQP-GLES2.test.p.1"]);
    deqp3.with_prefix("variant2-");
    deqp3.with_baseline("variant2-dEQP-GLES2.test.p.1,Fail");

    let results = DeqpSuite::new()
        .with_deqp(deqp1)
        .with_deqp(deqp2)
        .with_deqp(deqp3)
        .run()
        .unwrap();
    println!("{}", results.stdout);
    println!("{}", results.stderr);
    assert_eq!(Some(1), results.status.code());
    assert!(results
        .stdout
        .contains("Pass: 2, UnexpectedImprovement: 1, Skip: 1"));

    let results = results.results.unwrap();
    assert_eq!(results.result_counts.pass, 2);
    assert_eq!(results.result_counts.unexpected_improvement, 1);

    assert_eq!(
        results.get("dEQP-GLES2.test.p.1").unwrap().status,
        RunnerStatus::Pass
    );
    assert_eq!(
        results.get("variant1-dEQP-GLES2.test.p.1").unwrap().status,
        RunnerStatus::Pass
    );
    assert_eq!(
        results.get("variant1-dEQP-GLES2.test.p.2").unwrap().status,
        RunnerStatus::Skip
    );
    assert_eq!(
        results.get("variant2-dEQP-GLES2.test.p.1").unwrap().status,
        RunnerStatus::UnexpectedImprovement(TestStatus::Pass)
    );
}

#[test]
fn suite_prefix_skips() {
    let mut deqp1 = DeqpMock::new();
    deqp1.with_cases(vec!["dEQP-GLES2.test.p.1"]);
    deqp1.with_skips("dEQP-GLES2.*.p.1");
    let mut deqp2 = DeqpMock::new();
    deqp2.with_cases(vec!["dEQP-GLES2.test.p.1"]);
    deqp2.with_skips("dEQP-GLES2.*.p.1");
    deqp2.with_prefix("variant-");

    let results = DeqpSuite::new()
        .with_deqp(deqp1)
        .with_deqp(deqp2)
        .run()
        .unwrap();
    assert_eq!(Some(0), results.status.code());
    assert!(results.stdout.contains("Skip: 2"));

    let results = results.results.unwrap();
    assert_eq!(results.result_counts.skip, 2);
    assert_eq!(results.result_counts.fail, 0);

    assert_eq!(
        results.get("dEQP-GLES2.test.p.1").unwrap().status,
        RunnerStatus::Skip
    );
    assert_eq!(
        results.get("variant-dEQP-GLES2.test.p.1").unwrap().status,
        RunnerStatus::Skip
    );
}

#[test]
fn suite_missing() {
    let mut deqp = DeqpMock::new();
    deqp.with_cases(vec!["dEQP-GLES2.test.m.1"]);
    deqp.with_prefix("variant-");

    let results = DeqpSuite::new().with_deqp(deqp).run().unwrap();
    assert_eq!(Some(1), results.status.code());
    assert!(results.stdout.contains("Missing: 1"));

    let results = results.results.unwrap();
    assert_eq!(results.result_counts.missing, 1);
    assert_eq!(results.result_counts.fail, 0);

    assert_eq!(
        results.get("variant-dEQP-GLES2.test.m.1").unwrap().status,
        RunnerStatus::Missing
    );
}

#[test]
fn suite_includes() {
    let mut deqp = DeqpMock::new();
    deqp.with_cases(vec![
        "dEQP-GLES2.test.p.1.a.b",
        "dEQP-GLES2.test.p.1.a.c",
        "dEQP-GLES2.test.p.1.a.d",
        "dEQP-GLES2.test.p.1.a.e",
    ]);
    deqp.with_includes(".a");

    let results = DeqpSuite::new()
        .with_deqp(deqp)
        .with_includes(".c")
        .run()
        .unwrap();
    assert_eq!(Some(0), results.status.code());
    let results = results.results.unwrap();
    assert_eq!(results.result_counts.pass, 1);
    assert_eq!(
        results.get("dEQP-GLES2.test.p.1.a.c").unwrap().status,
        RunnerStatus::Pass
    );
}

#[test]
fn renderer_version_check() {
    let mut deqp = DeqpMock::new();
    deqp.with_cases(vec!["dEQP-GLES2.test.p.1"]);
    deqp.with_renderer_check("Intel.*CFL");
    deqp.with_version_check("OpenGL ES 3.2.*git-6a274846ba");

    let results = deqp.run().unwrap();

    assert!(results
        .stdout
        .contains("renderer: Mesa Intel(R) UHD Graphics 630 (CFL GT2)"));
    assert!(results
        .stdout
        .contains("version: OpenGL ES 3.2 Mesa 21.3.0-devel (git-6a274846ba)"));

    assert_eq!(Some(0), results.status.code());

    let results = results.results.unwrap();
    assert_eq!(results.result_counts.pass, 1);
    assert_eq!(results.result_counts.fail, 0);
}

#[test]
fn renderer_check_fail() {
    let mut deqp = DeqpMock::new();
    deqp.with_cases(vec!["dEQP-GLES2.test.p.1"]);
    deqp.with_renderer_check("AMD");
    deqp.with_version_check("OpenGL ES 3.2.*git-6a274846ba");

    let results = deqp.run().unwrap();

    assert!(results
        .stdout
        .contains("renderer: Mesa Intel(R) UHD Graphics 630 (CFL GT2)"));
    assert!(results
        .stdout
        .contains("version: OpenGL ES 3.2 Mesa 21.3.0-devel (git-6a274846ba)"));
    assert_eq!(Some(1), results.status.code());
}

#[test]
fn gl_extensions_check() {
    let exts_path = test_resource_path("deqp-gles2-extensions.txt");

    let mut deqp = DeqpMock::new();
    deqp.with_cases(vec!["dEQP-GLES2.test.p.1"]);
    deqp.with_extensions_check(MaybeTempFile::File(exts_path));

    let results = deqp.run().unwrap();

    assert_eq!(Some(0), results.status.code());

    let results = results.results.unwrap();
    assert_eq!(results.result_counts.pass, 1);
    assert_eq!(results.result_counts.fail, 0);
}

#[test]
fn gl_extensions_check_unexpected() -> Result<()> {
    // List of exts to check for, with a bunch of exts missing that the actual
    // impl has.
    let test_ext_file = lines_tempfile(vec!["GL_EXT_base_instance", "GL_EXT_draw_instanced"])
        .context("writing list of extensions to test for")?;

    let mut deqp = DeqpMock::new();
    deqp.with_cases(vec!["dEQP-GLES2.test.p.1"]);
    deqp.with_extensions_check(MaybeTempFile::Temp(test_ext_file));

    let results = deqp.run().unwrap();

    assert_eq!(Some(1), results.status.code());

    // Check that we mentioned one of the unexpected exts.
    assert!(results.stderr.contains("Unexpected: GL_EXT_frag_depth"));

    Ok(())
}

#[test]
fn gl_extensions_check_missing() -> Result<()> {
    let mut exts = Vec::new();
    for ext in BufReader::new(
        File::open(test_resource_path("deqp-gles2-extensions.txt")).context("opening exts list")?,
    )
    .lines()
    {
        let ext = ext.context("reading from exts list")?;
        exts.push(ext.clone());
    }

    // List of exts to check for, with an extra one
    let missing_ext = "GL_ARB_ham_sandwich";
    exts.push(missing_ext.to_owned());
    let test_ext_file = lines_tempfile(exts).context("writing list of extensions to test for")?;

    let mut deqp = DeqpMock::new();
    deqp.with_cases(vec!["dEQP-GLES2.test.p.1"]);
    deqp.with_extensions_check(MaybeTempFile::Temp(test_ext_file));

    let results = deqp.run().unwrap();

    assert_eq!(Some(1), results.status.code());

    // Check that we mentioned the missing ext.
    assert!(results
        .stderr
        .contains(&format!("Missing: {}", missing_ext)));

    Ok(())
}

#[test]
fn version_check_fail() {
    let mut deqp = DeqpMock::new();
    deqp.with_cases(vec!["dEQP-GLES2.test.p.1"]);
    deqp.with_renderer_check("Intel.*CFL");
    deqp.with_version_check("OpenGL ES 3.1.*git-6a274846ba");

    let results = deqp.run().unwrap();

    assert!(results
        .stdout
        .contains("renderer: Mesa Intel(R) UHD Graphics 630 (CFL GT2)"));
    assert!(results
        .stdout
        .contains("version: OpenGL ES 3.2 Mesa 21.3.0-devel (git-6a274846ba)"));
    assert_eq!(Some(1), results.status.code());
}

// Make sure that a set of runs with --fraction-start get different tests.
#[test]
fn fraction_start() {
    let mut cases = Vec::new();
    for i in 0..100 {
        cases.push(format!("dEQP-GLES2.test.p.{}", i));
    }

    let mut results = Vec::new();
    for i in 0..3 {
        let mut deqp = DeqpMock::new();
        results.push(
            deqp.with_cases(&cases)
                .with_fraction(3)
                .with_fraction_start(i + 1)
                .run()
                .unwrap()
                .results
                .unwrap(),
        );
    }

    // Check that no result list containers another list's tests.
    for i in 0..results.len() {
        for j in 0..results.len() {
            if i != j {
                for test in &results[i].tests {
                    let test: &str = test.borrow();
                    println!("looking for {} in {}", test, j);
                    assert!(!results[j].tests.contains(test));
                }
            }
        }
    }

    assert_eq!(
        cases.len(),
        results[0].tests.len() + results[1].tests.len() + results[2].tests.len()
    );
}

// Make sure that a run with --fraction 2 doesn't just skip every other test.
// Wouldn't want to lose all of the VS testing!
#[test]
fn fraction_uniformity() {
    let mut deqp = DeqpMock::new();

    let mut cases = Vec::new();
    for i in 0..1000 {
        cases.push(format!("dEQP-GLES2.test.p.{}.fragment", i));
        cases.push(format!("dEQP-GLES2.test.p.{}.vertex", i));
    }

    deqp.with_fraction(2);

    let results = deqp.with_cases(cases).run().unwrap().results.unwrap();

    // We had enough tests that we should have got one of each, at least.
    assert!(results
        .tests
        .iter()
        .map(|x| x.borrow())
        .any(|x: &str| x.contains("fragment")));
    assert!(results
        .tests
        .iter()
        .map(|x| x.borrow())
        .any(|x: &str| x.contains("vertex")));
}

#[test]
fn vk_renderer_check() {
    let mut deqp = DeqpMock::new();
    deqp.with_cases(vec!["dEQP-VK.test.p.1"]);
    deqp.with_renderer_check("AMD RADV");

    let results = deqp.run().unwrap();

    assert!(results.stdout.contains("deviceName: AMD RADV VEGA10"));

    assert_eq!(Some(0), results.status.code());

    let results = results.results.unwrap();
    assert_eq!(results.result_counts.pass, 1);
    assert_eq!(results.result_counts.fail, 0);
}

#[test]
fn vk_renderer_check_fail() {
    let mut deqp = DeqpMock::new();
    deqp.with_cases(vec!["dEQP-VK.test.p.1"]);
    deqp.with_renderer_check("Intel");

    let results = deqp.run().unwrap();

    assert!(results.stdout.contains("deviceName: AMD RADV VEGA10"));
    assert_eq!(Some(1), results.status.code());
}

#[test]
fn suite_renderer_version_check() {
    let mut deqp = DeqpMock::new();
    deqp.with_cases(vec!["dEQP-GLES2.test.p.1"]);
    deqp.with_renderer_check("Intel.*CFL");
    deqp.with_version_check("OpenGL ES 3.2.*git-6a274846ba");

    let results = DeqpSuite::new().with_deqp(deqp).run().unwrap();

    assert!(results
        .stdout
        .contains("renderer: Mesa Intel(R) UHD Graphics 630 (CFL GT2)"));
    assert_eq!(Some(0), results.status.code());

    let results = results.results.unwrap();
    assert_eq!(results.result_counts.pass, 1);
    assert_eq!(results.result_counts.fail, 0);
}

#[test]
fn suite_renderer_check_fail() {
    let mut deqp = DeqpMock::new();
    deqp.with_cases(vec!["dEQP-GLES2.test.p.1"]);
    deqp.with_renderer_check("AMD");
    deqp.with_version_check("OpenGL ES 3.2.*git-6a274846ba");

    let results = DeqpSuite::new().with_deqp(deqp).run().unwrap();

    assert!(results
        .stdout
        .contains("renderer: Mesa Intel(R) UHD Graphics 630 (CFL GT2)"));
    assert_eq!(Some(1), results.status.code());
}

#[test]
fn fraction_scales_group_size() {
    let mut deqp = DeqpMock::new();
    let mut tests = Vec::new();
    for i in 0..50 {
        tests.push(format!("dEQP-GLES2.test.p.{}", i));
    }
    deqp.with_cases(tests);
    deqp.with_fraction(10);

    deqp.with_j(2);

    let results = deqp.run().unwrap();

    println!("{}", results.stdout);

    assert!(results.stdout.contains("3-test groups")); // 2.5 tests per thread, rounded up
    assert_eq!(Some(0), results.status.code());
}
