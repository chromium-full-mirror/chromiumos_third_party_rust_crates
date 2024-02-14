use crate::parse::ResultParser;
use crate::parse_cl_cts::ClCtsResultParser;
use crate::timeout::{TimeoutChildStdout, Timer};
use crate::{runner_results::*, runner_thread_index, BinaryTest, SubRunConfig, TestConfiguration};
use crate::{CaselistResult, SingleBinaryTestCommand, SingleTestCommand, TestCase, TestCommand};
use anyhow::{Context, Result};
use log::*;
use serde::Deserialize;
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use structopt::StructOpt;

// CL-specific Structure for configuring a CL CTS run
#[derive(Debug, Deserialize, StructOpt)]
pub struct ClCtsRunConfig {
    #[structopt(long, help = "path to CL CTS build directory")]
    pub cl_folder: PathBuf,

    #[structopt(
        long,
        help = "CL CTS test set to run (such as `opencl_conformance_tests_full.csv`)"
    )]
    pub profile: String,
}

// Structure for the CL CTS entry in a suite.toml file.
#[derive(Deserialize)]
pub struct ClCtsTomlConfig {
    #[serde(flatten)]
    pub sub_config: SubRunConfig,

    #[serde(flatten)]
    pub cl_cts_config: ClCtsRunConfig,

    #[serde(default)]
    pub prefix: String,
}

fn parse_caselist<R: Read>(caselist: R) -> Result<Vec<crate::TestCase>> {
    let mut tests = Vec::new();

    for line in std::io::BufReader::new(caselist).lines() {
        let line = line.context("Reading caselist")?;
        if line.starts_with('#') {
            continue;
        }
        if line.is_empty() {
            continue;
        }

        fn compat_rsplit_once(s: &str, pat: char) -> Option<(&str, &str)> {
            let mut splitter = s.rsplitn(2, pat);

            let right = splitter.next();
            let left = splitter.next();
            if let (Some(left), Some(right)) = (left, right) {
                Some((left, right))
            } else {
                None
            }
        }

        // rsplit because the csv is badly formatted and there are ','s in
        // test names.  Use rsplit_once when we upgrade to rustc 1.52.
        if let Some((name, command)) = compat_rsplit_once(&line, ',') {
            let mut command = command.split(' ');

            tests.push(
                BinaryTest {
                    name: name.to_string(),
                    binary: command
                        .next()
                        .with_context(|| format!("extracting binary from {}", &line))?
                        .to_string(),
                    args: command.map(|x| (*x).to_string()).collect(),
                }
                .into(),
            );
        } else {
            anyhow::bail!("Failed to parse caselist line {}", line);
        }
    }

    Ok(tests)
}

impl ClCtsTomlConfig {
    pub fn conformance_dir(&self) -> PathBuf {
        self.cl_cts_config.cl_folder.join("test_conformance")
    }

    pub fn test_groups<'d>(
        &self,
        cl_cts: &'d ClCtsCommand,
        filters: &[String],
    ) -> Result<Vec<(&'d dyn TestCommand, Vec<TestCase>)>> {
        let caselist_path = self.conformance_dir().join(&self.cl_cts_config.profile);
        let caselist = std::fs::File::open(&caselist_path)
            .with_context(|| format!("Opening caselist at {}", caselist_path.display()))?;
        let tests: Vec<TestCase> = parse_caselist(caselist)
            .with_context(|| format!("Reading caselist at {}", caselist_path.display()))?;

        cl_cts.test_groups(&self.sub_config, filters, tests)
    }
}

pub struct ClCtsCommand {
    pub config: TestConfiguration,
    pub conformance_dir: PathBuf,
    pub prefix: String,
}

impl SingleTestCommand for ClCtsCommand {}
impl SingleBinaryTestCommand for ClCtsCommand {}

impl TestCommand for ClCtsCommand {
    fn name(&self) -> &str {
        "CL CTS"
    }

    fn prepare(&self, _caselist_state: &CaselistState, tests: &[&TestCase]) -> Result<Command> {
        let test = self.current_test(tests);

        let mut command = Command::new(self.conformance_dir.join(Path::new(&test.binary)));
        command
            .current_dir(&self.conformance_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null())
            .args(&test.args)
            .env("DEQP_RUNNER_THREAD", runner_thread_index()?.to_string())
            .envs(self.config.env.iter());

        debug!("Begin test {}", test.name);
        Ok(command)
    }

    fn clean(
        &self,
        _caselist_state: &CaselistState,
        tests: &[&TestCase],
        _results: &[RunnerResult],
    ) -> Result<()> {
        let test = self.current_test(tests);
        debug!("End test {}", test.name);
        Ok(())
    }

    fn parse_results(
        &self,
        _caselist_state: &CaselistState,
        tests: &[&TestCase],
        stdout: TimeoutChildStdout,
        timer: Option<Timer>,
    ) -> Result<CaselistResult> {
        let test = self.current_test(tests);
        let parser = ClCtsResultParser::new(&test.name);
        parser.parse_with_timer(stdout, timer)
    }

    fn should_save_log(&self, _caselist_state: &CaselistState, tests: &[&TestCase]) -> bool {
        let test = self.current_test(tests);
        test.name.contains("glinfo")
    }

    fn log_path(&self, _caselist_state: &CaselistState, tests: &[&TestCase]) -> Result<PathBuf> {
        let test = self.current_test(tests);
        Ok(self
            .config
            .output_dir
            .join(format!("cl_cts.{}.log", test.name).as_str()))
    }

    fn see_more(&self, test_name: &str, _caselist_state: &CaselistState) -> String {
        let log_path = self
            .config
            .output_dir
            .join(format!("cl_cts.{}.log", test_name).as_str());
        format!("See {:?}", log_path)
    }

    fn config(&self) -> &TestConfiguration {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case(name: &str, binary: &str, args: Vec<&str>) -> TestCase {
        BinaryTest {
            name: name.to_string(),
            binary: binary.to_string(),
            args: args.iter().map(|x| (*x).to_string()).collect(),
        }
        .into()
    }

    #[test]
    fn parse_caselist_test() {
        // This test must be run without -fbo, or it fails.
        let caselist = r#"
# #########################################
# General operation
# #########################################
Atomics,atomics/test_atomics
Allocations (single maximum),allocations/test_allocations single 5 all

# #########################################
# CPU is required to pass linear and normalized image filtering
# #########################################
CL_DEVICE_TYPE_CPU, Images (Kernel CL_FILTER_LINEAR),images/kernel_read_write/test_image_streams CL_FILTER_LINEAR
"#;

        assert_eq!(
            parse_caselist(caselist.as_bytes()).unwrap(),
            [
                case("Atomics", "atomics/test_atomics", vec![]),
                case(
                    "Allocations (single maximum)",
                    "allocations/test_allocations",
                    vec!["single", "5", "all"]
                ),
                case(
                    "CL_DEVICE_TYPE_CPU, Images (Kernel CL_FILTER_LINEAR)",
                    "images/kernel_read_write/test_image_streams",
                    vec!["CL_FILTER_LINEAR"]
                ),
            ]
        );
    }
}
