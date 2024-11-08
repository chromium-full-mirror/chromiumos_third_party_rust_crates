//! Module for invoking [IGT GPU Tools](https://gitlab.freedesktop.org/drm/igt-gpu-tools) tests.
use crate::parse::ResultParser;
use crate::parse_igt::{
    igt_parse_testcases_from_caselist, igt_parse_testcases_from_subtests, igt_parse_testlist_file,
    read_testlist_file, IgtResultParser,
};
use crate::timeout::{TimeoutChildStdout, Timer};
use crate::{
    runner_results::*, SingleBinaryTestCommand, SingleTestCommand, SubRunConfig, TestConfiguration,
};
use crate::{CaselistResult, TestResult, TestStatus};
use crate::{TestCase, TestCommand};
use anyhow::{Context, Result};
use log::*;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::str;
use structopt::StructOpt;

pub struct IgtCommand {
    pub config: TestConfiguration,
    pub igt_folder: PathBuf,
}

// Common structure for configuring a igt on deqp-runner Suite (multiple Runs)
#[derive(Debug, Deserialize, StructOpt)]
pub struct IgtRunConfig {
    #[structopt(long, help = "path to folder containing the IGT test binaries")]
    pub igt_folder: PathBuf,
}

#[derive(Deserialize)]
pub struct IgtTomlConfig {
    pub caselists: Vec<PathBuf>,

    #[serde(flatten)]
    pub sub_config: SubRunConfig,

    #[serde(flatten)]
    pub igt_config: IgtRunConfig,
}

fn igt_get_subtests(test_folder: &Path, binary: &str) -> Result<Vec<String>> {
    let mut command = Command::new(test_folder.join(Path::new(&binary)));
    let output = command
        .current_dir(test_folder)
        .args(&["--list-subtests".to_string()])
        .output()?;
    let s = str::from_utf8(&output.stdout)?
        .lines()
        .map(str::to_string)
        .collect();
    Ok(s)
}

fn igt_get_test_list_from_binaries(
    test_folder: &Path,
    binaries_list: Vec<String>,
) -> Result<Vec<TestCase>> {
    let mut tests: Vec<TestCase> = Vec::new();
    for bin in binaries_list {
        let subtests = igt_get_subtests(test_folder, &bin)?;
        let mut testcases = igt_parse_testcases_from_subtests(&bin, subtests)?;
        tests.append(&mut testcases);
    }
    Ok(tests)
}

fn igt_get_test_list(caselists: &[PathBuf], test_folder: &Path) -> Result<Vec<TestCase>> {
    let test_list = igt_parse_testcases_from_caselist(caselists)?;
    if !test_list.is_empty() {
        return Ok(test_list);
    }
    let text = read_testlist_file(test_folder)?;
    let binaries_list = igt_parse_testlist_file(&text)?
        .iter()
        .map(|i| i.to_string())
        .collect();
    let test_list = igt_get_test_list_from_binaries(test_folder, binaries_list)?;
    Ok(test_list)
}

impl IgtTomlConfig {
    pub fn test_groups<'d>(
        &self,
        igt: &'d IgtCommand,
        filters: &[String],
    ) -> Result<Vec<(&'d dyn TestCommand, Vec<TestCase>)>> {
        assert!(
            rayon::current_num_threads() == 1,
            "igt tests can't be run on more than one thread, use --jobs 1"
        );

        let test_folder = &self.igt_config.igt_folder;
        let tests: Vec<TestCase> = igt_get_test_list(&self.caselists, test_folder)
            .with_context(|| "Collecting IGT tests")?;

        igt.test_groups(&self.sub_config, filters, tests)
    }
}

impl SingleTestCommand for IgtCommand {}
impl SingleBinaryTestCommand for IgtCommand {}

impl TestCommand for IgtCommand {
    fn name(&self) -> &str {
        "Igt"
    }

    fn prepare(&self, _caselist_state: &CaselistState, tests: &[&TestCase]) -> Result<Command> {
        let test = self.current_test(tests);
        let bin_path = self.igt_folder.clone();

        let mut command = Command::new(bin_path.join(Path::new(&test.binary)));
        command
            .current_dir(&self.igt_folder)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null())
            .args(&test.args)
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

    fn handle_exit_status(&self, code: Option<i32>, some_result: Option<&mut TestResult>) {
        const IGT_EXIT_SUCCESS: i32 = 0;
        const IGT_EXIT_INVALID: i32 = 79;
        const IGT_EXIT_FAILURE: i32 = 98;
        const IGT_EXIT_SKIP: i32 = 77;

        if let Some(result) = some_result {
            result.status = match code {
                Some(IGT_EXIT_SUCCESS) => TestStatus::Pass,
                Some(IGT_EXIT_INVALID) => TestStatus::Skip,
                Some(IGT_EXIT_FAILURE) => TestStatus::Fail,
                Some(IGT_EXIT_SKIP) => TestStatus::Skip,
                _ => {
                    if result.status != TestStatus::Timeout {
                        TestStatus::Crash
                    } else {
                        result.status
                    }
                }
            }
        }
    }

    fn parse_results(
        &self,
        _caselist_state: &CaselistState,
        tests: &[&TestCase],
        stdout: TimeoutChildStdout,
        timer: Option<Timer>,
    ) -> Result<CaselistResult> {
        let test = self.current_test(tests);
        let parser = IgtResultParser::new(&test.name);
        parser.parse_with_timer(stdout, timer)
    }

    fn should_save_log(&self, _caselist_state: &CaselistState, tests: &[&TestCase]) -> bool {
        let _ = tests;
        true
    }

    fn log_path(&self, _caselist_state: &CaselistState, tests: &[&TestCase]) -> Result<PathBuf> {
        let test = self.current_test(tests);
        Ok(self
            .config
            .output_dir
            .join(format!("igt.{}.log", str::replace(&test.name, "/", "_")).as_str()))
    }

    fn see_more(&self, test_name: &str, _caselist_state: &CaselistState) -> String {
        let log_path = self
            .config
            .output_dir
            .join(format!("igt.{}.log", str::replace(test_name, "/", "_")).as_str());
        format!("See {:?}", log_path)
    }

    fn config(&self) -> &TestConfiguration {
        &self.config
    }
}
