//! Module for invoking [googletest](https://github.com/google/googletest) tests.

use crate::parse::{ParserState, ResultParser};
use crate::runner_results::*;
use crate::timeout::{TimeoutChildStdout, Timer};
use crate::{
    runner_thread_index, CaselistResult, TestCase, TestCommand, TestConfiguration, TestStatus,
};
use anyhow::{Context, Result};
use regex::Regex;
use std::path::PathBuf;
use std::process::{Command, Stdio};

pub struct GTestCommand {
    pub bin: PathBuf,
    pub config: TestConfiguration,
    pub args: Vec<String>,
}

impl TestStatus {
    pub fn from_gtest_str(input: &str) -> Result<TestStatus> {
        match input {
            "OK" => Ok(TestStatus::Pass),
            "FAILED" => Ok(TestStatus::Fail),
            "SKIPPED" => Ok(TestStatus::Skip),
            _ => anyhow::bail!("unknown gtest status '{}'", input),
        }
    }
}

fn parse_test_list(input: &str) -> Result<Vec<TestCase>> {
    let group_regex = Regex::new(r"(\S*\.)").context("Compiling group RE")?;
    let test_regex = Regex::new(r"\s+(\S+)").context("Compiling test RE")?;

    let mut tests = Vec::new();

    let mut group = None;
    for line in input.lines() {
        if let Some(c) = group_regex.captures(line) {
            group = Some(c[1].to_owned());
        } else if let Some(c) = test_regex.captures(line) {
            if let Some(group) = &group {
                tests.push(TestCase::Named(format!("{}{}", group, &c[1])));
            } else {
                anyhow::bail!("Test with no group set at: '{}'", line);
            }
        } else {
            anyhow::bail!("Failed to parse gtest list output: '{}'", line);
        }
    }

    Ok(tests)
}

#[derive(Default)]
struct GTestResultParser {
    current_test: Option<String>,
}

impl GTestResultParser {
    pub fn new() -> GTestResultParser {
        GTestResultParser { current_test: None }
    }
}

impl ResultParser for GTestResultParser {
    fn parse_line(&mut self, line: &str) -> Result<Option<ParserState>> {
        lazy_static! {
            static ref TEST_RE: Regex = Regex::new(r"\[\sRUN\s*\]\s(.*)").unwrap();
            static ref STATUS_RE: Regex = Regex::new(r"\[\s*(\S*)\s*\]\s(\S*)").unwrap();
        }

        let mut state = None;

        if self.current_test.is_some() {
            if let Some(cap) = STATUS_RE.captures(line) {
                let _name = self.current_test.take().unwrap();
                let status = TestStatus::from_gtest_str(&cap[1])?;
                state = Some(ParserState::EndTest(status));
            }
        } else if let Some(cap) = TEST_RE.captures(line) {
            let name = cap[1].to_string();
            self.current_test = Some(name.to_owned());
            state = Some(ParserState::BeginTest(name));
        }

        Ok(state)
    }
}

impl GTestCommand {
    pub fn list_tests(&self) -> Result<Vec<TestCase>> {
        let output = Command::new(&self.bin)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null())
            .args(&self.args)
            .arg("--gtest_list_tests")
            .output()
            .with_context(|| format!("Failed to spawn {}", &self.bin.display()))?;

        if !output.status.success() {
            anyhow::bail!(
                "Failed to invoke gtest command {} for test listing:\nstdout:\n{}\nstderr:\n{}",
                self.bin.display(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }

        parse_test_list(
            std::str::from_utf8(&output.stdout).context("Parsing gtest output as UTF8")?,
        )
    }
}

impl TestCommand for GTestCommand {
    fn name(&self) -> &str {
        "gtest"
    }

    fn prepare(&self, _caselist_state: &CaselistState, tests: &[&TestCase]) -> Result<Command> {
        let mut tests_iter = tests.iter();

        let mut tests_arg = format!(
            "--gtest_filter={}",
            tests_iter.next().context("getting first test")?.name()
        );

        for test in tests_iter {
            tests_arg.push(':');
            tests_arg.push_str(test.name());
        }

        let mut command = Command::new(&self.bin);
        command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null())
            .env("DEQP_RUNNER_THREAD", runner_thread_index()?.to_string())
            .args(&self.args)
            .arg(tests_arg);
        Ok(command)
    }

    fn parse_results(
        &self,
        _caselist_state: &CaselistState,
        _tests: &[&TestCase],
        stdout: TimeoutChildStdout,
        timer: Option<Timer>,
    ) -> Result<CaselistResult> {
        let parser = GTestResultParser::new();
        parser.parse_with_timer(stdout, timer)
    }

    fn see_more(&self, _name: &str, caselist_state: &CaselistState) -> String {
        // This is the same as run() did, so we should be safe to unwrap.
        let qpa_path = self.config.output_dir.join(
            format!(
                "c{}.r{}.log",
                caselist_state.caselist_id, caselist_state.run_id
            )
            .as_str(),
        );
        format!("See {:?}", qpa_path)
    }

    fn config(&self) -> &TestConfiguration {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CaselistResult, TestResult};
    use std::time::Duration;

    fn parse_gtest_results(output: &mut &[u8]) -> Result<CaselistResult> {
        let parser = GTestResultParser::new();
        parser.parse(output)
    }

    fn result(name: &str, status: TestStatus) -> TestResult {
        TestResult {
            name: name.to_string(),
            status,
            duration: Duration::new(0, 0),
            subtests: vec![],
        }
    }

    #[test]
    fn list_tests() -> Result<()> {
        let input = "VAAPICreateContextToFail.
  CreateContextWithNoConfig
VAAPIDisplayAttribs.
  MaxNumDisplayAttribs
  QueryDisplayAttribs
GetCreateConfig/VAAPIGetCreateConfig.
  CreateConfigWithAttributes/0  # GetParam() = (-1:VAProfileNone, 1:VAEntrypointVLD)
  CreateConfigWithAttributes/1  # GetParam() = (-1:VAProfileNone, 2:VAEntrypointIZZ)";

        let tests: Vec<String> = parse_test_list(input)
            .context("parsing")?
            .into_iter()
            .map(|x| x.name().to_string())
            .collect();

        assert_eq!(
            tests,
            vec!(
                "VAAPICreateContextToFail.CreateContextWithNoConfig".to_owned(),
                "VAAPIDisplayAttribs.MaxNumDisplayAttribs".to_owned(),
                "VAAPIDisplayAttribs.QueryDisplayAttribs".to_owned(),
                "GetCreateConfig/VAAPIGetCreateConfig.CreateConfigWithAttributes/0".to_owned(),
                "GetCreateConfig/VAAPIGetCreateConfig.CreateConfigWithAttributes/1".to_owned(),
            )
        );
        Ok(())
    }

    #[test]
    fn parse_results() -> Result<()> {
        use TestStatus::*;
        let output = "[----------] 1 test from VAAPIQueryVendor
[ RUN      ] VAAPIQueryVendor.NotEmpty
[       OK ] VAAPIQueryVendor.NotEmpty (11 ms)
[----------] 1 test from VAAPIQueryVendor (11 ms total)

[----------] 690 tests from GetCreateConfig/VAAPIGetCreateConfig
[ RUN      ] GetCreateConfig/VAAPIGetCreateConfig.CreateConfigNoAttributes/219
../test/test_va_api_fixture.cpp:224: Failure
      Expected: VaapiStatus(expectation)
      Which is: VA_STATUS_ERROR_UNSUPPORTED_PROFILE
To be equal to: VaapiStatus(vaCreateConfig(m_vaDisplay, profile, entrypoint, (attribs.size() != 0 ? const_cast<VAConfigAttrib*>(attribs.data()) : __null), attribs.size(), &m_configID))
      Which is: VA_STATUS_ERROR_UNSUPPORTED_ENTRYPOINT
profile    = 21:VAProfileVP9Profile2
entrypoint = 11:VAEntrypointFEI
numAttribs = 0
[  FAILED  ] GetCreateConfig/VAAPIGetCreateConfig.CreateConfigNoAttributes/219, where GetParam() = (21:VAProfileVP9Profile2, 11:VAEntrypointFEI) (11 ms)
[ RUN      ] GetCreateConfig/VAAPIGetCreateConfig.CreateConfigPackedHeaders/0
[ SKIPPED ] -1:VAProfileNone / 1:VAEntrypointVLD not supported on this hardware
[       OK ] GetCreateConfig/VAAPIGetCreateConfig.CreateConfigPackedHeaders/0
[ RUN      ] CreateSurfaces/VAAPICreateSurfaces.CreateSurfacesWithConfigAttribs/136";

        let results = parse_gtest_results(&mut output.as_bytes())?.results;

        assert_eq!(
            results,
            vec!(
                result("VAAPIQueryVendor.NotEmpty", Pass),
                result(
                    "GetCreateConfig/VAAPIGetCreateConfig.CreateConfigNoAttributes/219",
                    Fail
                ),
                result(
                    "GetCreateConfig/VAAPIGetCreateConfig.CreateConfigPackedHeaders/0",
                    Skip
                ),
                result(
                    "CreateSurfaces/VAAPICreateSurfaces.CreateSurfacesWithConfigAttribs/136",
                    Crash
                ),
            )
        );

        Ok(())
    }
}
