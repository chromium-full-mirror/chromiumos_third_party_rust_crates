use crate::parse::ResultParser;
use crate::parse_skqp::{skqp_parse_testlist_file, SkqpResultParser, SkqpTestFile};
use crate::timeout::{TimeoutChildStdout, Timer};
use crate::{runner_results::*, SubRunConfig, TestConfiguration};
use crate::{CaselistResult, SingleNamedTestCommand, SingleTestCommand, TestCase, TestCommand};
use anyhow::{Context, Result};
use log::*;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::{fs, str};
use structopt::StructOpt;

pub struct SkqpCommand {
    pub skqp: PathBuf,
    pub config: TestConfiguration,
    pub skqp_assets: PathBuf,
}

// Common structure for configuring a skqp on deqp-runner Suite (multiple Runs)
#[derive(Debug, Deserialize, StructOpt)]
pub struct SkqpRunConfig {
    #[structopt(long, help = "path to skqp binary")]
    pub skqp: PathBuf,

    #[structopt(
        long,
        help = "path to skia/platform_tools/android/apps/skqp/src/main/assets"
    )]
    pub skqp_assets: PathBuf,
}

#[derive(Deserialize)]
pub struct SkqpTomlConfig {
    #[serde(flatten)]
    pub sub_config: SubRunConfig,

    #[serde(flatten)]
    pub skqp_config: SkqpRunConfig,
}

fn skqp_get_test_list(skqp_assets: &Path) -> Result<Vec<TestCase>> {
    let rendertests_path = skqp_assets
        .join("skqp")
        .join(SkqpTestFile::RenderTest.to_string());

    let rendertests = std::fs::read_to_string(&rendertests_path)
        .with_context(|| format!("reading {:?}", rendertests_path))?;
    let rendertests = skqp_parse_testlist_file(&rendertests).context("parsing render tests")?;

    let mut test_list = Vec::new();
    for test in rendertests {
        for backend in &["gl", "gles", "vk"] {
            test_list.push(TestCase::Named(format!("{}_{}", backend, test)));
        }
    }

    let unittests_path = skqp_assets
        .join("skqp")
        .join(SkqpTestFile::UnitTest.to_string());

    let unittests = std::fs::read_to_string(&unittests_path)
        .with_context(|| format!("reading {:?}", unittests_path))?;
    let unittests = skqp_parse_testlist_file(&unittests)?;
    for test in unittests {
        test_list.push(TestCase::Named(test.to_owned()));
    }

    Ok(test_list)
}

impl SkqpTomlConfig {
    pub fn test_groups<'d>(
        &self,
        skqp: &'d SkqpCommand,
        filters: &[String],
    ) -> Result<Vec<(&'d dyn TestCommand, Vec<TestCase>)>> {
        let tests: Vec<TestCase> = skqp_get_test_list(&self.skqp_config.skqp_assets)
            .with_context(|| "Collecting skqp tests")?;

        skqp.test_groups(&self.sub_config, filters, tests)
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct SkqpTest {
    pub name: String,
}

impl SkqpCommand {
    fn skqp_output_dir_path(
        &self,
        test_name: &str,
        caselist_state: &CaselistState,
    ) -> Result<PathBuf> {
        Ok(self
            .config
            .output_dir
            .join(format!("{}.r{}", test_name, caselist_state.caselist_id)))
    }
}

impl SingleTestCommand for SkqpCommand {}
impl SingleNamedTestCommand for SkqpCommand {}

impl TestCommand for SkqpCommand {
    fn name(&self) -> &str {
        "Skqp"
    }

    fn prepare(&self, caselist_state: &CaselistState, tests: &[&TestCase]) -> Result<Command> {
        let test = self.current_test(tests);

        let test_output_dir = self.skqp_output_dir_path(test, caselist_state)?;
        fs::create_dir_all(test_output_dir.to_str().unwrap())?;

        let mut command = Command::new(&self.skqp);
        command
            .current_dir(&self.skqp_assets)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null())
            .args(vec![
                self.skqp_assets.to_str().unwrap(),
                test_output_dir.canonicalize()?.to_str().unwrap(),
                test,
            ])
            .envs(self.config.env.iter());

        debug!("Begin test {}", test);
        Ok(command)
    }

    fn clean(
        &self,
        caselist_state: &CaselistState,
        tests: &[&TestCase],
        results: &[RunnerResult],
    ) -> Result<()> {
        let test = self.current_test(tests);
        let test_output_dir = self.skqp_output_dir_path(test, caselist_state)?;

        if !results.is_empty()
            && results
                .iter()
                .all(|x| !x.status.should_save_logs(self.config.save_xfail_logs))
        {
            if let Err(e) = std::fs::remove_dir_all(&test_output_dir)
                .with_context(|| format!("removing caselist at {:?}", &test_output_dir))
            {
                error!("{:?}", e);
            }
        }

        debug!("End test {}", test);
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
        let parser = SkqpResultParser::new(test);
        parser.parse_with_timer(stdout, timer)
    }

    fn log_path(&self, _caselist_state: &CaselistState, tests: &[&TestCase]) -> Result<PathBuf> {
        let test = self.current_test(tests);
        Ok(self
            .config
            .output_dir
            .join(format!("skqp.{}.log", str::replace(test, "/", "_")).as_str()))
    }

    fn see_more(&self, test_name: &str, caselist_state: &CaselistState) -> String {
        let log_path = self
            .config
            .output_dir
            .join(format!("skqp.{}.log", str::replace(test_name, "/", "_")).as_str());

        let test_output_dir = self
            .skqp_output_dir_path(test_name, caselist_state)
            .unwrap();

        format!(
            "See {:?} and {:?}",
            log_path,
            test_output_dir.to_str().unwrap()
        )
    }

    fn config(&self) -> &TestConfiguration {
        &self.config
    }
}
