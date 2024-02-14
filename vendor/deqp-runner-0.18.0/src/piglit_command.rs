//! Module for invoking [piglit](https://piglit.freedesktop.org/) tests.

use crate::parse::ResultParser;
use crate::parse_piglit::{parse_piglit_xml_testlist, read_profile_file, PiglitResultParser};
use crate::timeout::{TimeoutChildStdout, Timer};
use crate::{runner_results::*, runner_thread_index, SubRunConfig, TestConfiguration};
use crate::{CaselistResult, SingleBinaryTestCommand, SingleTestCommand, TestCase, TestCommand};
use anyhow::{Context, Result};
use log::*;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use structopt::StructOpt;

pub struct PiglitCommand {
    pub config: TestConfiguration,
    pub piglit_folder: PathBuf,
    pub prefix: String,
}

// Common structure for configuring a piglit run between Run (single run) and deqp-runner Suite (muliple Runs)
#[derive(Debug, Deserialize, StructOpt)]
pub struct PiglitRunConfig {
    #[structopt(long, help = "path to piglit folder")]
    pub piglit_folder: PathBuf,

    #[structopt(long, help = "piglit profile to run (such as quick_gl)")]
    pub profile: String,

    #[structopt(long = "process-isolation")]
    #[serde(default)]
    pub process_isolation: bool,
}

#[derive(Deserialize)]
pub struct PiglitTomlConfig {
    #[serde(flatten)]
    pub sub_config: SubRunConfig,

    #[serde(flatten)]
    pub piglit_config: PiglitRunConfig,

    #[serde(default)]
    pub prefix: String,
}

impl PiglitTomlConfig {
    pub fn test_groups<'d>(
        &self,
        piglit: &'d PiglitCommand,
        filters: &[String],
    ) -> Result<Vec<(&'d dyn TestCommand, Vec<TestCase>)>> {
        let test_folder = self.piglit_config.piglit_folder.join("tests");
        let text = read_profile_file(
            &test_folder,
            &self.piglit_config.profile,
            self.piglit_config.process_isolation,
        )?;
        let tests: Vec<TestCase> =
            parse_piglit_xml_testlist(&test_folder, &text, self.piglit_config.process_isolation)
                .with_context(|| {
                    format!("reading piglit profile '{}'", &self.piglit_config.profile)
                })?;

        piglit.test_groups(&self.sub_config, filters, tests)
    }
}

impl SingleTestCommand for PiglitCommand {}
impl SingleBinaryTestCommand for PiglitCommand {}

impl TestCommand for PiglitCommand {
    fn name(&self) -> &str {
        "Piglit"
    }

    fn prepare(&self, _caselist_state: &CaselistState, tests: &[&TestCase]) -> Result<Command> {
        let test = self.current_test(tests);
        let mut bin_path = self.piglit_folder.clone();
        bin_path.push("bin");

        let mut command = Command::new(bin_path.join(Path::new(&test.binary)));
        command
            .current_dir(&self.piglit_folder)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null())
            .args(&test.args)
            .env("MESA_DEBUG", "silent")
            .env("DEQP_RUNNER_THREAD", runner_thread_index()?.to_string())
            .env("PIGLIT_SOURCE_DIR", &self.piglit_folder)
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
        let parser = PiglitResultParser::new(&test.name);
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
            .join(format!("piglit.{}.log", test.name).as_str()))
    }

    fn see_more(&self, test_name: &str, _caselist_state: &CaselistState) -> String {
        let log_path = self
            .config
            .output_dir
            .join(format!("piglit.{}.log", test_name).as_str());
        format!("See {:?}", log_path)
    }

    fn config(&self) -> &TestConfiguration {
        &self.config
    }
}
