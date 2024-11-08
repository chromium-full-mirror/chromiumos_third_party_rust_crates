//! deqp_runner consists of this library crate for supporting parallel testing
//! of various test suites (dEQP, piglit, gtest, IGT GPU Tools) using baseline
//! expectations plus known flakes, plus several binary commands for invoking
//! this library.
//!
//! Adding a new test suite should be a matter of:
//! - Add a `parse_testsuite` module for parsing the output of your test suite
//!   and turning it into a [TestStatus].
//! - Add a `testsuite_command` module implementing [TestCommand] for invoking
//!   your test suite.
//! - Add at least one of:
//!   - `bin/testsuite.rs` binary to call [parallel_test] on your [TestCommand].
//!   - `bin/deqp.rs` support to read a description of your to run your
//!     testsuite from a `suite.toml` file and generate test groups for it along
//!     with the other testsuites in `SuiteConfig`.  This is useful if a user
//!     would want to run your testsuite along with others all at once with one
//!     summary at the end.
//! - Probably:
//!   - Add a `mock` subcommand in your bin/testsuite.rs that pretends to be
//!     your testsuite binary (for integration testing without having the
//!     testsuite installed) with whatever interesting behaviors your testsuite
//!     has you might want to test (timeouts, crashes, flakiness).
//!   - `tests/integration/testsuite_runner.rs` module to test calling your
//!     `bin/testsuite.rs` command, passing in testsuite.rs's built binary path
//!     as the testsuite to call with the Mock subcommand.

#[macro_use]
extern crate lazy_static;
pub mod cl_cts_command;
pub mod deqp_command;
pub mod gtest_command;
pub mod igt_command;
pub mod mock_deqp;
pub mod mock_gtest;
pub mod mock_igt;
pub mod mock_piglit;
pub mod mock_skqp;
pub mod parse;
pub mod parse_cl_cts;
mod parse_deqp;
pub mod parse_igt;
pub mod parse_piglit;
pub mod parse_skqp;
pub mod piglit_command;
mod runner_results;
pub mod skqp_command;
mod test_status;
mod timeout;

use anyhow::bail;
pub use runner_results::*;

pub use crate::test_status::{CaselistResult, TestResult, TestStatus};
use anyhow::{Context, Result};
use log::*;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use rayon::prelude::*;
use regex::RegexSet;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::prelude::*;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;
use std::time::Instant;
use structopt::StructOpt;
use timeout::{TimeoutChildExt, TimeoutChildStdout, Timer};

fn parse_key_val<T, U>(s: &str) -> Result<(T, U), Box<dyn std::error::Error>>
where
    T: std::str::FromStr,
    T::Err: std::error::Error + 'static,
    U: std::str::FromStr,
    U::Err: std::error::Error + 'static,
{
    let pos = s
        .find('=')
        .ok_or_else(|| format!("invalid KEY=value: no `=` found in `{}`", s))?;
    Ok((s[..pos].parse()?, s[pos + 1..].parse()?))
}

// Cross test-type CLI/toml options
#[derive(Clone, Debug, Deserialize, StructOpt)]
pub struct SubRunConfig {
    #[structopt(
        long,
        help = "path to baseline results (such as output/failures.csv from another run)"
    )]
    #[serde(default)]
    pub baseline: Option<PathBuf>,

    #[structopt(
        long,
        help = "path to file of regexes of tests to skip running (for runtime or stability reasons)"
    )]
    #[serde(default)]
    pub skips: Vec<PathBuf>,

    #[structopt(
        long,
        help = "path to file of regexes of tests to assume any failures in those tests are flaky results (but still run them, for long-term status tracking)"
    )]
    #[serde(default)]
    pub flakes: Vec<PathBuf>,

    #[structopt(
        short = "t",
        long = "include-tests",
        help = "regexes of tests to include (non-matching tests are skipped)"
    )]
    #[serde(default)]
    pub include: Vec<String>,

    #[structopt(
        long,
        default_value = "60.0",
        help = "per-test timeout in floating point seconds"
    )]
    #[serde(default)]
    pub timeout: f32,

    #[structopt(long, default_value = "1", help = "Runs 1 out of every N tests.")]
    #[serde(default)]
    pub fraction: usize,

    #[structopt(
        long,
        default_value = "1",
        help = "Skips the first N-1 tests in the test list before applying --fraction (useful for running N/M fraciton of the test list across multiple devices)."
    )]
    #[serde(default)]
    pub fraction_start: usize,

    #[structopt(
        parse(try_from_str = parse_key_val),
        long = "env",
        help = "Environment variables to set when invoking the test process"
    )]
    #[serde(with = "tuple_vec_map", default)]
    pub env: Vec<(String, String)>,
}

impl SubRunConfig {
    pub fn apply_suite_top_config(&mut self, top: &SubRunConfig) {
        if self.fraction == 0 {
            self.fraction = 1;
        }
        if self.fraction_start == 0 {
            self.fraction_start = 1;
        }

        for f in &top.skips {
            self.skips.push(f.clone());
        }

        for f in &top.flakes {
            self.flakes.push(f.clone());
        }

        if let Some(run_baseline) = top.baseline.as_ref() {
            if self.baseline.is_some() {
                eprintln!("baseline may only be set on either the command line or per-deqp.");
                std::process::exit(1);
            }
            self.baseline = Some(run_baseline.clone());
        }

        // Apply global fraction to the suite's internal fraction.
        self.fraction *= top.fraction;
        self.fraction_start += top.fraction_start - 1;

        if self.timeout == 0.0 {
            self.timeout = 60.0;
        }

        for (var, data) in &top.env {
            self.env.push((var.to_owned(), data.to_owned()));
        }
    }
}

// Top-level commandline run/suite options.  Except for env, which ought to be
// in SubRunConfig but I found hard to get matched up between toml and structopt
// parsing.
#[derive(Debug, StructOpt)]
pub struct CommandLineRunOptions {
    #[structopt(long = "output", help = "path to output directory")]
    pub output_dir: PathBuf,

    #[structopt(flatten)]
    pub sub_config: SubRunConfig,

    #[structopt(
        short = "j",
        long,
        default_value = "0",
        help = "Number of processes to invoke in parallel (default 0 = number of CPUs in system)"
    )]
    pub jobs: usize,

    #[structopt(
        long,
        default_value = "25",
        help = "Number of fails or flakes to print in the summary line (0 = no limit)"
    )]
    pub summary_limit: usize,

    #[structopt(
        parse(from_occurrences),
        short = "v",
        long,
        help = "Enable verbose mode (-v, -vv, -vvv, etc)"
    )]
    pub verbose: usize,

    #[structopt(long, help = "Enable log timestamps (sec, ms, ns)")]
    pub timestamp: Option<stderrlog::Timestamp>,
    #[structopt(
        long,
        help = "Saves log files for expected failures along with new ones"
    )]
    pub save_xfail_logs: bool,
}

impl CommandLineRunOptions {
    pub fn setup(&self) -> Result<()> {
        stderrlog::new()
            .module(module_path!())
            .verbosity(self.verbose)
            .timestamp(self.timestamp.unwrap_or(stderrlog::Timestamp::Off))
            .init()
            .unwrap();

        if self.jobs > 0 {
            rayon::ThreadPoolBuilder::new()
                .num_threads(self.jobs)
                .build_global()
                .unwrap();
        }

        if self.sub_config.fraction < 1 {
            eprintln!("--fraction must be >= 1.");
            std::process::exit(1);
        }
        if self.sub_config.fraction_start < 1 {
            eprintln!("--fraction_start must be >= 1.");
            std::process::exit(1);
        }

        std::fs::create_dir_all(&self.output_dir).context("creating output directory")?;

        Ok(())
    }

    pub fn baseline(&self) -> Result<RunnerResults> {
        read_baseline(self.sub_config.baseline.as_ref())
    }

    pub fn skips_regex(&self) -> Result<RegexSet> {
        parse_regex_set(read_lines(&self.sub_config.skips)?).context("compiling skips regexes")
    }

    pub fn flakes_regex(&self) -> Result<RegexSet> {
        parse_regex_set(read_lines(&self.sub_config.flakes)?).context("compiling flakes regexes")
    }

    pub fn includes_regex(&self) -> Result<RegexSet> {
        if self.sub_config.include.is_empty() {
            RegexSet::new(vec![""]).context("compiling all-tests include RE")
        } else {
            parse_regex_set(&self.sub_config.include).context("compiling include filters")
        }
    }
}

// Common runtime configuration of a test suite sub-run.
pub struct TestConfiguration {
    pub output_dir: PathBuf,
    pub skips: RegexSet,
    pub flakes: RegexSet,
    pub baseline: RunnerResults,
    pub timeout: Duration,
    pub env: HashMap<String, String>,
    pub save_xfail_logs: bool,
}

impl TestConfiguration {
    pub fn from_cli(run: &CommandLineRunOptions) -> Result<TestConfiguration> {
        TestConfiguration::from_suite_config(run, &run.sub_config)
    }

    pub fn from_suite_config(
        run: &CommandLineRunOptions,
        sub_config: &SubRunConfig,
    ) -> Result<TestConfiguration> {
        Ok(TestConfiguration {
            output_dir: run.output_dir.to_path_buf(),
            skips: parse_regex_set(read_lines(&sub_config.skips)?)
                .context("compiling skips regexes")?,
            flakes: parse_regex_set(read_lines(&sub_config.flakes)?)
                .context("compiling flakes regexes")?,
            baseline: read_baseline(sub_config.baseline.as_ref())?,
            timeout: Duration::from_secs_f32(sub_config.timeout),
            env: sub_config.env.iter().cloned().collect(),
            save_xfail_logs: run.save_xfail_logs,
        })
    }
}

/// This is implemented by each supported test suite.
pub trait TestCommand: Send + Sync {
    /// Must be implemented, returns the name of this TestCommand for logging errors.
    fn name(&self) -> &str;

    /// Must be implemented, returns the shared test configuration for this TestCommand.
    fn config(&self) -> &TestConfiguration;

    /// Optional (recommended) hook for logging on failure to point the user to
    /// more information about a specific failure (caselist, log file, etc).
    fn see_more(&self, _name: &str, _caselist_state: &CaselistState) -> String {
        "".to_string()
    }

    fn skips(&self) -> &RegexSet {
        &self.config().skips
    }

    fn flakes(&self) -> &RegexSet {
        &self.config().flakes
    }

    fn baseline(&self) -> &RunnerResults {
        &self.config().baseline
    }

    fn baseline_status(&self, test: &str) -> Option<RunnerStatus> {
        self.baseline().get(test).map(|x| x.status)
    }

    fn translate_result(
        &self,
        result: &TestResult,
        caselist_state: &CaselistState,
    ) -> RunnerStatus {
        let mut status = RunnerStatus::from_deqp(result.status)
            .with_baseline(self.baseline_status(&result.name));

        if !status.is_success() && self.flakes().is_match(&result.name) {
            status = RunnerStatus::KnownFlake;
        }

        if !status.is_success()
            || status == RunnerStatus::Flake
            || status == RunnerStatus::KnownFlake
        {
            error!(
                "Test {}: {}: {}",
                &result.name,
                status,
                self.see_more(&result.name, caselist_state)
            );
        }

        status
    }

    fn skip_test(&self, test: &str) -> bool {
        self.skips().is_match(test)
    }

    fn prepare(&self, caselist_state: &CaselistState, tests: &[&TestCase]) -> Result<Command>;

    /// Invokes the test suite's result parser on stdout.
    fn parse_results(
        &self,
        caselist_state: &CaselistState,
        tests: &[&TestCase],
        stdout: TimeoutChildStdout,
        timer: Option<Timer>,
    ) -> Result<CaselistResult>;

    /// Used for doing any extra logging that might need to be done for failed tests.
    fn handle_result(
        &self,
        _caselist_state: &CaselistState,
        _result: &TestResult,
        _status: &RunnerStatus,
    ) -> Result<()> {
        Ok(())
    }

    fn log_path(&self, caselist_state: &CaselistState, _tests: &[&TestCase]) -> Result<PathBuf> {
        self.caselist_file_path(caselist_state, "log")
            .context("log path")
    }

    /// Overrideable hook for whether a log should be always be saved. Sometimes
    /// there's a test that collects general implementation details (API
    /// version, extensions) that might be useful for the user to always have,
    /// for sanity-checking that the run tested what they intended.
    fn should_save_log(&self, _caselist_state: &CaselistState, _tests: &[&TestCase]) -> bool {
        false
    }

    /// Hook for cleaning up anything necessary after running a test binary
    /// (like removing logs that shouldn't be saved for all the passing tests).
    fn clean(
        &self,
        _caselist_state: &CaselistState,
        _tests: &[&TestCase],
        _results: &[RunnerResult],
    ) -> Result<()> {
        Ok(())
    }

    /// By default, sets the last test result with Crash if the binary crashed
    /// after emitting test results.  Can also be overridden if the exit status
    /// is what determines test success/failure.
    fn handle_exit_status(&self, code: Option<i32>, some_result: Option<&mut TestResult>) {
        // If the process crashed, then report the case crashed
        // regardless of whether it produced a plausible result string.
        match code {
            Some(0) | Some(1) => {}
            _ => {
                if let Some(result) = some_result {
                    if result.status != TestStatus::Timeout {
                        result.status = TestStatus::Crash;
                    }
                }
            }
        }
    }

    /// Invokes the test command on a list of testcases. This is common code
    /// because of the trickiness in handling timeouts, whether logs should be
    /// saved, etc.
    fn run(
        &self,
        caselist_state: &CaselistState,
        tests: &[&TestCase],
    ) -> Result<Vec<RunnerResult>> {
        let mut command = self.prepare(caselist_state, tests)?;
        let command_line = format!("{:?}", command);

        let timer = Timer::new(self.config().timeout);
        let mut child = command
            .spawn()
            .with_context(|| format!("Failed to spawn {}", &command_line))?
            .with_timeout(timer.clone());
        let stdout = child.stdout().context("opening stdout")?;
        let results = self
            .parse_results(caselist_state, tests, stdout, Some(timer))
            .context("parsing results");

        // The child should have run to completion based on the parser consuming its output,
        // but if we had a timeout or parse failure then we want to kill this run.
        let _ = child.kill();

        // Make sure we reap the child process.
        let status = child.wait().context("waiting for child")?;

        let CaselistResult {
            mut results,
            stdout,
        } = results.context("parsing results")?;

        self.handle_exit_status(status.code(), results.last_mut());

        let stderr: Vec<String> = BufReader::new(child.stderr().context("opening stderr")?)
            .lines()
            .flatten()
            .collect();

        for line in &stderr {
            // If the driver has ASan enabled and it detected leaks, then mark
            // all the tests in the caselist as failed (since we don't know who
            // to assign the failure to).
            if line.contains("ERROR: LeakSanitizer: detected memory leaks") {
                error!(
                    "{}: Leak detected, marking caselist as failed ({})",
                    self.name(),
                    self.see_more("", caselist_state)
                );
                for result in results.iter_mut() {
                    result.status = TestStatus::Fail;
                }
            }
            error!("{} error: {}", self.name(), line);
        }

        let mut runner_results: Vec<RunnerResult> = Vec::new();
        let mut save_log = self.should_save_log(caselist_state, tests) || results.is_empty();
        for result in &mut results {
            result.name = format!("{}{}", self.prefix(), &result.name);
            for subtest in &mut result.subtests {
                subtest.name = format!("{}@{}", &result.name, &subtest.name);

                if self.skip_test(&subtest.name) {
                    error!(
                        "Skip list matches subtest {}, but you can't skip execution of subtests.",
                        &subtest.name
                    );
                }

                runner_results.push(RunnerResult {
                    test: subtest.name.to_owned(),
                    status: self.translate_result(subtest, caselist_state),
                    duration: subtest.duration.as_secs_f32(),
                    subtest: true,
                });
            }

            let status = self.translate_result(result, caselist_state);
            if status.should_save_logs(self.config().save_xfail_logs) {
                save_log = true;
            }

            self.handle_result(caselist_state, result, &status)?;

            runner_results.push(RunnerResult {
                test: result.name.to_owned(),
                status,
                duration: result.duration.as_secs_f32(),
                subtest: false,
            });
        }

        if save_log {
            let log_path = self.log_path(caselist_state, tests)?;
            let mut file = File::create(log_path).context("opening log file")?;

            fn write_output(file: &mut File, name: &str, out: &[String]) -> Result<()> {
                if out.is_empty() {
                    writeln!(file, "{}: (empty)", name)?;
                } else {
                    writeln!(file, "{}:", name)?;
                    writeln!(file, "-------")?;
                    for line in out {
                        writeln!(file, "{}", line)?;
                    }
                }
                Ok(())
            }

            // Use a closure to wrap all the try operator paths with one .context().
            || -> Result<()> {
                writeln!(file, "command: {}", command_line)?;
                writeln!(file, "exit status: {}", status)?;
                write_output(&mut file, "stdout", &stdout)?;
                write_output(&mut file, "stderr", &stderr)?;
                Ok(())
            }()
            .context("writing log file")?;
        }

        self.clean(caselist_state, tests, &runner_results)?;

        Ok(runner_results)
    }

    /// Loop invoking the list of cases, where we run again to check if it was
    /// just a flake if an unexpected result occurs.
    fn run_caselist_and_flake_detect(
        &self,
        caselist: &[TestCase],
        caselist_state: &mut CaselistState,
    ) -> Result<Vec<RunnerResult>> {
        // Sort the caselists within test groups.  dEQP runs tests in sorted order, and when one
        // is debugging a failure in one case in a caselist, it can be nice to be able to easily trim
        // all of the caselist appearing after the failure, to reduce runtime.
        let mut caselist: Vec<_> = caselist.iter().collect();
        caselist.sort_by(|x, y| x.name().cmp(y.name()));

        caselist_state.run_id += 1;
        let mut results = self.run(caselist_state, caselist.as_slice())?;
        // If we made no more progress on the whole caselist,
        // then dEQP doesn't know about some of our tests and they'll report Missing.
        if results.is_empty() {
            anyhow::bail!(
                "No results parsed.  Is your caselist out of sync with your deqp binary?"
            );
        }

        // If any results came back with an unexpected failure, run the caselist again
        // to see if we get the same results, and mark any changing results as flaky tests.
        if results.iter().any(|x| !x.status.is_success()) {
            caselist_state.run_id += 1;
            let retest_results = self.run(caselist_state, caselist.as_slice())?;
            for pair in results.iter_mut().zip(retest_results.iter()) {
                if pair.0.status != pair.1.status {
                    pair.0.status = RunnerStatus::Flake;
                }
            }
        }

        Ok(results)
    }

    fn process_caselist(
        &self,
        tests: Vec<TestCase>,
        caselist_id: u32,
    ) -> Result<Vec<RunnerResult>> {
        let mut caselist_results: Vec<RunnerResult> = Vec::new();
        let mut remaining_tests = Vec::new();
        for test in tests {
            // Get the test name with the group prefix.
            let name = if !self.prefix().is_empty() {
                self.prefix().to_owned() + test.name()
            } else {
                test.name().to_owned()
            };

            if self.skip_test(&name) {
                caselist_results.push(RunnerResult {
                    test: name,
                    status: RunnerStatus::Skip,
                    duration: Default::default(),
                    subtest: false,
                });
            } else {
                remaining_tests.push(test);
            }
        }

        let mut caselist_state = CaselistState {
            caselist_id,
            run_id: 0,
        };

        while !remaining_tests.is_empty() {
            let results = self.run_caselist_and_flake_detect(&remaining_tests, &mut caselist_state);

            match results {
                Ok(results) => {
                    for result in results {
                        /* Remove the reported test from our list of tests to run.  If it's not in our list, then it's
                         * a subtest.
                         */
                        if let Some(position) = remaining_tests
                            .iter()
                            .position(|x| x.name() == result.test.trim_start_matches(self.prefix()))
                        {
                            remaining_tests.swap_remove(position);
                        } else if !result.subtest {
                            error!(
                                "Top-level test result for {} not found in list of tests to run.",
                                &result.test
                            );
                        }

                        caselist_results.push(result);
                    }
                }
                Err(e) => {
                    error!(
                        "Failure getting run results: {:#} ({})",
                        e,
                        self.see_more("", &caselist_state)
                    );

                    for test in remaining_tests {
                        caselist_results.push(RunnerResult {
                            test: self.prefix().to_owned() + test.name(),
                            status: RunnerStatus::Missing,
                            duration: Default::default(),
                            subtest: false,
                        });
                    }
                    break;
                }
            }
        }

        Ok(caselist_results)
    }

    fn split_tests_to_groups(
        &self,
        mut tests: Vec<TestCase>,
        tests_per_group: usize,
        min_tests_per_group: usize,
        sub_config: &SubRunConfig,
        include_filters: &[RegexSet],
    ) -> Result<Vec<(&dyn TestCommand, Vec<TestCase>)>>
    where
        Self: Sized,
    {
        if tests_per_group < 1 {
            bail!("tests_per_group must be >= 1.");
        }

        // If you haven't requested the scaling-down behavior, make all groups
        // the same size.
        let min_tests_per_group = if min_tests_per_group == 0 {
            tests_per_group
        } else {
            min_tests_per_group
        };

        // Shuffle the test groups using a deterministic RNG so that every run gets the same shuffle.
        tests.shuffle(&mut StdRng::from_seed([0x3bu8; 32]));

        // Apply fraction after shuffling so the fraction is more uniform across
        // test namespace (and hopefully test space as well).
        let mut tests = tests
            .into_iter()
            .skip(sub_config.fraction_start - 1)
            .step_by(sub_config.fraction)
            .filter(|test| {
                let name = self.prefix().to_owned() + test.name();
                include_filters.iter().all(|x| x.is_match(&name))
            })
            .collect::<Vec<TestCase>>();

        let rayon_threads = rayon::current_num_threads();
        let tests_per_group = usize::max(
            1,
            usize::min(
                (tests.len() + rayon_threads - 1) / rayon_threads,
                tests_per_group,
            ),
        );

        // Make test groups of tests_per_group() (512) tests, or if
        // min_tests_per_group() is lower than that, then 1/32nd of the
        // remaining tests down to that limit.
        let mut test_groups: Vec<(&dyn TestCommand, Vec<TestCase>)> = Vec::new();
        let mut remaining = tests.len();
        while remaining != 0 {
            let min = usize::min(min_tests_per_group, remaining);
            let group_len = usize::min(usize::max(remaining / 32, min), tests_per_group);
            remaining -= group_len;

            if remaining == 0 {
                // Free the memory remaining in the original test vector, because split_off(0) won't.
                tests.shrink_to_fit();
            }

            test_groups.push((self, tests.split_off(remaining)));
        }

        Ok(test_groups)
    }

    fn caselist_file_path(&self, caselist_state: &CaselistState, suffix: &str) -> Result<PathBuf> {
        // deqp must be run from its directory, so make sure all the filenames we pass in are absolute.
        let output_dir = self.config().output_dir.canonicalize()?;

        Ok(output_dir.join(format!(
            "c{}.r{}.{}",
            caselist_state.caselist_id, caselist_state.run_id, suffix
        )))
    }

    /// Optional prefix to add to the test names, to distinguish between
    /// testsuite variations (like enabled debug knobs) in a `deqp-runner suite`
    /// invocation.
    fn prefix(&self) -> &str {
        ""
    }
}

/// Some [TestCommand] implementations need more than a string test name to
/// invoke their test binary.  All of those variants are in this enum so that
/// the core [parallel_test] can talk about lists of TestCases, while the
/// specific [TestCommand] implementation can match the subtype out to get at
/// the test's details.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TestCase {
    Named(String),
    Binary(Box<BinaryTest>),
}

/// TestCase struct for test commands where each test is a separate binary and set of args.
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct BinaryTest {
    pub name: String,
    pub binary: String,
    pub args: Vec<String>,
}

impl TestCase {
    pub fn name(&self) -> &str {
        match self {
            TestCase::Named(name) => name,
            TestCase::Binary(test) => &test.name,
        }
    }
}

impl From<BinaryTest> for TestCase {
    fn from(value: BinaryTest) -> Self {
        TestCase::Binary(Box::new(value))
    }
}

/// trait for TestCommands that run a single test per test group.
pub trait SingleTestCommand: TestCommand {
    fn test_groups<'d>(
        &'d self,
        sub_config: &SubRunConfig,
        filters: &[String],
        tests: Vec<TestCase>,
    ) -> Result<Vec<(&'d dyn TestCommand, Vec<TestCase>)>>
    where
        Self: Sized,
    {
        let mut include_filters = Vec::new();
        if !sub_config.include.is_empty() {
            include_filters.push(
                parse_regex_set(&sub_config.include)
                    .context("compiling sub_config include filters")?,
            );
        }
        if !filters.is_empty() {
            include_filters.push(parse_regex_set(filters).context("compiling include filters")?);
        }

        let groups = self.split_tests_to_groups(tests, 1, 1, sub_config, &include_filters)?;

        println!(
            "Running {} {} tests on {} threads",
            groups.iter().map(|x| x.1.len()).sum::<usize>(),
            self.name(),
            rayon::current_num_threads()
        );

        Ok(groups)
    }
}

/// trait for TestCommands that run a single BinaryTest per test group.
pub trait SingleBinaryTestCommand: SingleTestCommand {
    /// Sanity-checks and returns the BinaryTest that the test list contains.
    fn current_test<'a>(&self, tests: &'a [&TestCase]) -> &'a BinaryTest {
        assert_eq!(tests.len(), 1);
        let test = &tests[0];

        match test {
            TestCase::Binary(t) => t,
            _ => panic!("Invalid case"),
        }
    }
}

/// trait for TestCommands that run a single TestCase::Named() per test group.
pub trait SingleNamedTestCommand: SingleTestCommand {
    /// Sanity-checks and returns the test name in the test list.
    fn current_test<'a>(&self, tests: &'a [&TestCase]) -> &'a String {
        assert_eq!(tests.len(), 1);
        let test = &tests[0];

        match test {
            TestCase::Named(t) => t,
            _ => panic!("Invalid case"),
        }
    }
}

impl AsRef<str> for TestCase {
    fn as_ref(&self) -> &str {
        self.name()
    }
}

impl AsRef<TestCase> for TestCase {
    fn as_ref(&self) -> &TestCase {
        self
    }
}

fn results_collection<W: Write>(
    status_output: &mut W,
    run_results: &mut RunnerResults,
    total_tests: u32,
    receiver: Receiver<Result<Vec<RunnerResult>>>,
) {
    let update_interval = Duration::new(2, 0);

    run_results.status_update(status_output, total_tests);
    let mut last_status_update = Instant::now();

    for group_results in receiver {
        match group_results {
            Ok(group_results) => {
                for result in group_results {
                    run_results.record_result(result);
                }
            }
            Err(e) => {
                println!("Error: {}", e);
            }
        }
        if last_status_update.elapsed() >= update_interval {
            run_results.status_update(status_output, total_tests);
            last_status_update = Instant::now();
        }
    }

    // Always print the final results
    run_results.status_update(status_output, total_tests);
}

/// The heart of deqp-runner: this distributes the list of test groups (each a
/// [TestCommand] and list of tests to run an invocation of that command) to the
/// thread pool to parallelize running them across cores, collecting the results
/// to the [RunnerResults] and logging incremental status to the console along
/// the way.
pub fn parallel_test(
    status_output: impl Write + Sync + Send,
    test_groups: Vec<(&dyn TestCommand, Vec<TestCase>)>,
) -> Result<RunnerResults> {
    let test_count = test_groups.iter().map(|x| x.1.len() as u32).sum();

    let mut run_results = RunnerResults::new();

    // Make a channel for the parallel iterator to send results to, which is what will be
    // printing the console status output but also computing the run_results.
    let (sender, receiver) = channel::<Result<Vec<RunnerResult>>>();

    let mut status_output = status_output;

    crossbeam_utils::thread::scope(|s| {
        // Spawn the results collection in a crossbeam scope, so that it doesn't
        // take a slot in rayon's thread pool.
        s.spawn(|_| results_collection(&mut status_output, &mut run_results, test_count, receiver));

        // Rayon parallel iterator takes our vector and runs it on its thread
        // pool.
        test_groups
            .into_iter()
            .enumerate()
            .par_bridge()
            .try_for_each_with(sender, |sender, (i, (deqp, tests))| {
                sender.send(deqp.process_caselist(tests, i as u32))
            })
            .unwrap();

        // As we leave this scope, crossbeam will join the results collection
        // thread.  Note that it terminates cleanly because we moved the sender
        // into the rayon iterator.
    })
    .unwrap();

    Ok(run_results)
}

/// Returns the thread index, used in various [TestCommand] implementations to
/// have separate paths per thread to avoid races between tests being run in
/// parallel.
pub fn runner_thread_index() -> Result<usize> {
    rayon::current_thread_index().context("getting thread id within rayon global thread pool")
}

/// Parses a deqp-runner regex set list, used for the skips/flakes lists that
/// the user specifies.  We ignore empty lines and lines starting with "#", so
/// you can keep notes about why tests are disabled or flaky.
pub fn parse_regex_set<I, S>(exprs: I) -> Result<RegexSet>
where
    S: AsRef<str>,
    I: IntoIterator<Item = S>,
{
    let lines: Vec<_> = exprs
        .into_iter()
        .filter(|x| !x.as_ref().is_empty() && !x.as_ref().starts_with('#'))
        .collect();

    for line in &lines {
        if let Some(after_comma) = line.as_ref().rsplit(',').next() {
            if after_comma.parse::<RunnerStatus>().is_ok() {
                bail!(
                    "regex line appears to incorrectly contain a result status: {}",
                    line.as_ref()
                );
            }
        }
    }
    RegexSet::new(lines).context("Parsing regex set")
}

/// Common helper for reading the lines of a caselist/baseline/skips/flakes
/// file, producing useful error context.
pub fn read_lines<I: IntoIterator<Item = impl AsRef<Path>>>(files: I) -> Result<Vec<String>> {
    let mut lines: Vec<String> = Vec::new();

    for path in files {
        let path = path.as_ref();
        for line in BufReader::new(
            File::open(path).with_context(|| format!("opening path: {}", path.display()))?,
        )
        .lines()
        {
            let line = line.with_context(|| format!("reading line from {}", path.display()))?;
            // In newer dEQP, vk-master.txt just contains a list of .txt
            // caselist files relative to its current path, so recursively read
            // thoseand append their contents.
            if line.ends_with(".txt") {
                let sub_path = path.parent().context("Getting path parent dir")?.join(line);

                lines.extend_from_slice(
                    &read_lines([sub_path.as_path()])
                        .with_context(|| format!("reading sub-caselist {}", sub_path.display()))?,
                );
            } else {
                lines.push(line)
            }
        }
    }
    Ok(lines)
}

/// Called with the results of a [parallel_test], this outputs the `results.csv`
/// (all results with timings) and the `failures.csv` (unexpected results for
/// the user the user needs to debug, or to use as their `--baseline` for future
/// regression testing).  Also prints a summary of failures, flakes, and
/// runtimes for management of test runs.
pub fn process_results(
    results: &RunnerResults,
    output_dir: &Path,
    summary_limit: usize,
) -> Result<()> {
    results.write_results(&mut File::create(output_dir.join("results.csv"))?)?;
    results.write_failures(&mut File::create(output_dir.join("failures.csv"))?)?;

    results.print_summary(if summary_limit == 0 {
        std::usize::MAX
    } else {
        summary_limit
    });

    if !results.is_success() {
        std::process::exit(1);
    }

    Ok(())
}

/// Reads in the `--baseline` argument of expected-failing tests.  Baselines are
/// of the same format as a `failures.csv` so you can use your first run as the
/// baseline for future runs.
pub fn read_baseline(path: Option<&PathBuf>) -> Result<RunnerResults> {
    match path {
        Some(path) => {
            let mut file = File::open(path).context("Reading baseline")?;
            RunnerResults::from_csv(&mut file)
        }
        None => Ok(RunnerResults::new()),
    }
}
