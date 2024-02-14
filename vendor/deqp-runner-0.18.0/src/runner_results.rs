use crate::TestStatus;
use anyhow::{Context, Result};
use log::*;
use regex::Regex;
use std::borrow::Borrow;
use std::collections::HashSet;
use std::fmt;
use std::io::prelude::*;
use std::io::{BufReader, BufWriter};
use std::str::FromStr;
use std::time::{Duration, Instant};
use structopt::StructOpt;

// Wrapper for displaying a duration in h:m:s (integer seconds, rounded down)
struct HMSDuration(Duration);
impl fmt::Display for HMSDuration {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut secs = self.0.as_secs();
        let hours = secs / 3600;
        secs %= 3600;
        let mins = secs / 60;
        secs %= 60;

        if hours > 0 {
            write!(f, "{}:{:02}:{:02}", hours, mins, secs)
        } else if mins > 0 {
            write!(f, "{}:{:02}", mins, secs)
        } else {
            write!(f, "{}", secs)
        }
    }
}

/// The result types logged to the console: like [TestStatus] but with
/// `ExpectedFail` (yeah, it failed, but you knew it failed in the baseline
/// anyway and weren't expecting to have fixed it) and `UnexpectedImprovement`
/// (congrats, you've fixed a test, go clear the xfail out of your baseline!).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunnerStatus {
    Pass,
    Fail,
    Skip,
    Crash,
    Flake,
    KnownFlake,
    Warn,
    Missing,
    ExpectedFail,
    UnexpectedImprovement(TestStatus),
    Timeout,
}

impl FromStr for RunnerStatus {
    type Err = anyhow::Error;

    // Parses the status name from dEQP's output.
    fn from_str(input: &str) -> Result<RunnerStatus, Self::Err> {
        if let Some(status_with_paren) = input.strip_prefix("UnexpectedImprovement(") {
            if let Some(status) = status_with_paren.strip_suffix(')') {
                if let Ok(status) = status.parse::<TestStatus>() {
                    return Ok(RunnerStatus::UnexpectedImprovement(status));
                }
            }
        }
        match input {
            "Pass" => Ok(RunnerStatus::Pass),
            "Fail" => Ok(RunnerStatus::Fail),
            "Crash" => Ok(RunnerStatus::Crash),
            "Skip" => Ok(RunnerStatus::Skip),
            "Flake" => Ok(RunnerStatus::Flake),
            "KnownFlake" => Ok(RunnerStatus::KnownFlake),
            "Warn" => Ok(RunnerStatus::Warn),
            "Missing" => Ok(RunnerStatus::Missing),
            "ExpectedFail" => Ok(RunnerStatus::ExpectedFail),
            "Timeout" => Ok(RunnerStatus::Timeout),
            _ => anyhow::bail!("unknown runner status '{}'", input),
        }
    }
}

impl RunnerStatus {
    pub fn is_success(&self) -> bool {
        match self {
            RunnerStatus::Pass
            | RunnerStatus::Skip
            | RunnerStatus::Warn
            | RunnerStatus::Flake
            | RunnerStatus::KnownFlake
            | RunnerStatus::ExpectedFail => true,
            RunnerStatus::Fail
            | RunnerStatus::Crash
            | RunnerStatus::Missing
            | RunnerStatus::UnexpectedImprovement(_)
            | RunnerStatus::Timeout => false,
        }
    }

    pub fn should_save_logs(&self, save_xfail_logs: bool) -> bool {
        !self.is_success()
            || *self == RunnerStatus::Flake
            || *self == RunnerStatus::KnownFlake
            || (*self == RunnerStatus::ExpectedFail && save_xfail_logs)
    }

    pub fn from_deqp(status: TestStatus) -> RunnerStatus {
        match status {
            TestStatus::Pass => RunnerStatus::Pass,
            TestStatus::Fail => RunnerStatus::Fail,
            TestStatus::Crash => RunnerStatus::Crash,
            TestStatus::Skip => RunnerStatus::Skip,
            TestStatus::Warn => RunnerStatus::Warn,
            TestStatus::Timeout => RunnerStatus::Timeout,
        }
    }

    pub fn with_baseline(self, baseline: Option<RunnerStatus>) -> RunnerStatus {
        use RunnerStatus::*;

        if let Some(baseline) = baseline {
            match self {
                Fail => match baseline {
                    Fail | ExpectedFail => ExpectedFail,
                    // This is a tricky one -- if you expected a crash and you got fail, that's
                    // an improvement where we should want to record the change in expectation.
                    Crash => UnexpectedImprovement(TestStatus::Fail),
                    // Ditto for timeouts
                    Timeout => UnexpectedImprovement(TestStatus::Fail),
                    _ => self,
                },
                Pass => match baseline {
                    Fail | Crash | Missing | Timeout => UnexpectedImprovement(TestStatus::Pass),
                    _ => Pass,
                },
                Crash => match baseline {
                    // If one is reusing a results.csv from a previous run with a baseline,
                    // then ExpectedFail might have been from a crash, so keep it as xfail.
                    Crash | ExpectedFail => ExpectedFail,
                    _ => Crash,
                },
                Warn => match baseline {
                    Fail | Crash => UnexpectedImprovement(TestStatus::Warn),
                    Warn => ExpectedFail,
                    _ => Warn,
                },
                Skip => match baseline {
                    Fail | Crash | Missing | Timeout => UnexpectedImprovement(TestStatus::Skip),
                    _ => self,
                },
                Flake => Flake,
                KnownFlake => KnownFlake,
                Timeout => match baseline {
                    Timeout => ExpectedFail,
                    _ => Timeout,
                },
                Missing | ExpectedFail | UnexpectedImprovement(_) => {
                    unreachable!("can't appear from TestStatus")
                }
            }
        } else {
            self
        }
    }
}
impl fmt::Display for RunnerStatus {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            RunnerStatus::Pass => f.write_str("Pass"),
            RunnerStatus::Fail => f.write_str("Fail"),
            RunnerStatus::Skip => f.write_str("Skip"),
            RunnerStatus::Crash => f.write_str("Crash"),
            RunnerStatus::Warn => f.write_str("Warn"),
            RunnerStatus::Flake => f.write_str("Flake"),
            RunnerStatus::KnownFlake => f.write_str("KnownFlake"),
            RunnerStatus::Missing => f.write_str("Missing"),
            RunnerStatus::ExpectedFail => f.write_str("ExpectedFail"),
            RunnerStatus::UnexpectedImprovement(s) => write!(f, "UnexpectedImprovement({})", s),
            RunnerStatus::Timeout => f.write_str("Timeout"),
        }
    }
}

#[derive(Debug)]
pub struct RunnerResult {
    pub test: String,
    pub status: RunnerStatus,
    pub duration: f32,
    pub subtest: bool,
}

// For comparing equality, we ignore the test runtime (particularly of use for the unit tests )
impl PartialEq for RunnerResult {
    fn eq(&self, other: &Self) -> bool {
        self.test == other.test && self.subtest == other.subtest && self.status == other.status
    }
}

pub struct RunnerResultNameHash(RunnerResult);

// Use the test name as the hash key, which lets us store a HashSet<RunnerResult> intead of HashMap<String,RunnerResult>.
impl core::hash::Hash for RunnerResultNameHash {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.test.hash(state);
    }
}

impl PartialEq for RunnerResultNameHash {
    fn eq(&self, other: &Self) -> bool {
        self.0.test == other.0.test
    }
}

impl Eq for RunnerResultNameHash {}

impl Borrow<str> for RunnerResultNameHash {
    fn borrow(&self) -> &str {
        &self.0.test
    }
}

#[derive(Eq, Clone, Copy, Hash, PartialEq)]
pub struct CaselistState {
    pub caselist_id: u32,
    pub run_id: u32,
}

#[derive(Default, PartialEq, Eq, Debug)]
pub struct ResultCounts {
    pub pass: u32,
    pub fail: u32,
    pub skip: u32,
    pub crash: u32,
    pub warn: u32,
    pub flake: u32,
    pub missing: u32,
    pub expected_fail: u32,
    pub unexpected_improvement: u32,
    pub timeout: u32,
    pub total: u32,
}
impl ResultCounts {
    pub fn new() -> ResultCounts {
        Default::default()
    }
    pub fn increment(&mut self, s: RunnerStatus) {
        match s {
            RunnerStatus::Pass => self.pass += 1,
            RunnerStatus::Fail => self.fail += 1,
            RunnerStatus::Skip => self.skip += 1,
            RunnerStatus::Crash => self.crash += 1,
            RunnerStatus::Warn => self.warn += 1,
            RunnerStatus::Flake | RunnerStatus::KnownFlake => self.flake += 1,
            RunnerStatus::Missing => self.missing += 1,
            RunnerStatus::ExpectedFail => self.expected_fail += 1,
            RunnerStatus::UnexpectedImprovement(_) => self.unexpected_improvement += 1,
            RunnerStatus::Timeout => self.timeout += 1,
        }
    }

    pub fn get_count(&self, status: RunnerStatus) -> u32 {
        use RunnerStatus::*;

        match status {
            Pass => self.pass,
            Fail => self.fail,
            Skip => self.skip,
            Crash => self.crash,
            Warn => self.warn,
            Flake | KnownFlake => self.flake,
            Missing => self.missing,
            ExpectedFail => self.expected_fail,
            UnexpectedImprovement(_) => self.unexpected_improvement,
            Timeout => self.timeout,
        }
    }
}
impl fmt::Display for ResultCounts {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use RunnerStatus::*;

        write!(f, "Pass: {}", self.pass)?;
        for status in &[
            Fail,
            Crash,
            UnexpectedImprovement(TestStatus::Pass), /* note: the inner status is ignored. */
            ExpectedFail,
            Warn,
            Skip,
            Timeout,
            Missing,
            Flake,
        ] {
            let count = self.get_count(*status);
            if count != 0 {
                match status {
                    // Don't print the dummy inner teststatus -- we don't track
                    // those in ResultCounts.
                    UnexpectedImprovement(_) => write!(f, ", UnexpectedImprovement: {}", count)?,
                    _ => write!(f, ", {}: {}", status, count)?,
                }
            }
        }
        Ok(())
    }
}

fn print_interesting_subset(
    results: &[(&String, RunnerStatus)],
    header: &str,
    filter: fn(RunnerStatus) -> bool,
    summary_limit: usize,
    failures_mode: bool,
) {
    let results: Vec<_> = results.iter().filter(|x| filter(x.1)).collect();
    if results.is_empty() {
        return;
    }

    println!();
    println!("{}", header);

    for test in results.iter().take(summary_limit) {
        if failures_mode {
            println!("  {},{}", test.0, test.1)
        } else {
            println!("  {}", test.0)
        }
    }
    if results.len() > summary_limit {
        println!(
            "  ... and more (see {})",
            if failures_mode {
                "failures.csv"
            } else {
                "results.csv"
            }
        );
    }
}

pub struct RunnerResults {
    pub tests: HashSet<RunnerResultNameHash>,
    pub result_counts: ResultCounts,
    pub time: Instant,
}

impl RunnerResults {
    pub fn new() -> RunnerResults {
        Default::default()
    }

    pub fn get(&self, test: &str) -> Option<&RunnerResult> {
        self.tests.get(test).map(|x| &x.0)
    }

    pub fn record_result(&mut self, result: RunnerResult) {
        let mut result = result;

        if self.get(&result.test).is_some() {
            error!(
                "Duplicate test result for {}, marking test failed",
                &result.test
            );
            result.status = RunnerStatus::Fail;
        } else if !result.subtest {
            self.result_counts.total += 1;
        }
        self.result_counts.increment(result.status);
        self.tests.replace(RunnerResultNameHash(result));
    }

    pub fn is_success(&self) -> bool {
        self.tests.iter().all(|result| result.0.status.is_success())
    }

    /// Returns a list of references to the results, sorted by test name.
    pub fn sorted_results(&self) -> Vec<&RunnerResult> {
        let mut sorted: Vec<_> = self.tests.iter().map(|x| &x.0).collect();
        sorted.sort_by_key(|x| &x.test);
        sorted
    }

    pub fn write_results<W: Write>(&self, writer: &mut W) -> Result<()> {
        let mut writer = BufWriter::new(writer);
        for result in self.sorted_results() {
            writeln!(
                writer,
                "{},{},{}",
                result.test, result.status, result.duration
            )?;
        }
        Ok(())
    }

    pub fn write_failures(&self, writer: &mut impl Write) -> Result<()> {
        let mut writer = BufWriter::new(writer);
        for result in self.sorted_results() {
            if !result.status.is_success() {
                writeln!(writer, "{},{}", result.test, result.status)?;
            }
        }
        Ok(())
    }

    pub fn write_junit_failures(
        &self,
        writer: &mut impl Write,
        options: &JunitGeneratorOptions,
    ) -> Result<()> {
        use junit_report::*;
        let limit = if options.limit == 0 {
            std::usize::MAX
        } else {
            options.limit
        };

        let mut testcases = Vec::new();
        for result in self.sorted_results().iter().take(limit) {
            let tc = if !result.status.is_success() {
                let message = options.template.replace("{{testcase}}", &result.test);

                let type_ = format!("{}", result.status);

                junit_report::TestCase::failure(
                    &result.test,
                    Duration::seconds(0),
                    &type_,
                    &message,
                )
            } else {
                junit_report::TestCase::success(&result.test, Duration::seconds(0))
            };
            testcases.push(tc);
        }

        let ts = junit_report::TestSuite::new(&options.testsuite).add_testcases(testcases);

        junit_report::Report::new()
            .add_testsuite(ts)
            .write_xml(BufWriter::new(writer))
            .context("writing XML output")
    }

    pub fn from_csv(r: &mut impl Read) -> Result<RunnerResults> {
        lazy_static! {
            static ref CSV_RE: Regex = Regex::new("^([^,]+),([^,]+)").unwrap();
        }

        let mut results = RunnerResults::new();
        let r = BufReader::new(r);
        for (lineno, line) in r.lines().enumerate() {
            let line = line.context("Reading CSV")?;
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some(cap) = CSV_RE.captures(&line) {
                let result = RunnerResult {
                    test: cap[1].to_string(),
                    status: cap[2].parse()?,
                    duration: 0.0,
                    subtest: false,
                };

                // If you have more than one result for a test in the CSV,
                // something has gone wrong (probably human error writing a
                // baseline list)
                if results.get(&result.test).is_some() {
                    anyhow::bail!("Found duplicate result for {} at line {}", line, lineno);
                }

                results.record_result(result);
            } else {
                anyhow::bail!(
                    "Failed to parse {} as CSV test,status[,duration] or comment at line {}",
                    line,
                    lineno
                );
            }
        }
        Ok(results)
    }

    pub fn status_update<W: Write>(&self, writer: &mut W, total_tests: u32) {
        let duration = self.time.elapsed();

        write!(
            writer,
            "{}, Duration: {duration}",
            self.result_counts,
            duration = HMSDuration(duration),
        )
        .context("status update")
        .unwrap_or_else(|e| error!("{}", e));

        // If we have some tests completed, use that to estimate remaining runtime.
        let duration = duration.as_secs_f32();
        if self.result_counts.total != 0 {
            let average_test_time = duration / self.result_counts.total as f32;
            let remaining = average_test_time * (total_tests - self.result_counts.total) as f32;

            write!(
                writer,
                ", Remaining: {}",
                HMSDuration(Duration::from_secs_f32(remaining))
            )
            .context("status update")
            .unwrap_or_else(|e| error!("{}", e));
        }

        writeln!(writer)
            .context("status update")
            .unwrap_or_else(|e| error!("{}", e));
    }

    pub fn print_summary(&self, summary_limit: usize) {
        if self.tests.is_empty() {
            return;
        }

        let mut slowest: Vec<_> = self
            .tests
            .iter()
            .map(|result| (&result.0.test, result.0.duration))
            .collect();
        // Sorting on duration and reversing because you can't negate a duration and you can't easily
        // sort on a f32.
        slowest.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        slowest.reverse();

        println!();
        println!("Slowest tests:");
        for test in slowest.iter().take(5) {
            println!("  {} ({:.02}s)", test.0, test.1);
        }

        // For printing interesting test results, first collect all the test
        // names/status that aren't pass/skip (trims the list to almost
        // nothing), sorted by name.
        let mut interesting_results: Vec<_> = self
            .tests
            .iter()
            .filter(|result| !matches!(result.0.status, RunnerStatus::Pass | RunnerStatus::Skip))
            .map(|result| (&result.0.test, result.0.status))
            .collect();
        interesting_results.sort_by_key(|x| x.0);

        print_interesting_subset(
            &interesting_results,
            "Some known flakes found:",
            |x| x == RunnerStatus::KnownFlake,
            summary_limit,
            false,
        );

        print_interesting_subset(
            &interesting_results,
            "Some new flakes found:",
            |x| x == RunnerStatus::Flake,
            summary_limit,
            false,
        );

        print_interesting_subset(
            &interesting_results,
            "Unexpected results:",
            |x| !x.is_success(),
            summary_limit,
            true,
        );
    }
}

impl Default for RunnerResults {
    fn default() -> RunnerResults {
        RunnerResults {
            tests: Default::default(),
            result_counts: Default::default(),
            time: Instant::now(),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_map() {
        use RunnerStatus::*;

        // Absence of baseline should translate in a straightforward way.
        assert_eq!(
            Pass,
            RunnerStatus::from_deqp(TestStatus::Pass).with_baseline(None)
        );
        assert_eq!(
            Fail,
            RunnerStatus::from_deqp(TestStatus::Fail).with_baseline(None)
        );
        assert_eq!(
            Crash,
            RunnerStatus::from_deqp(TestStatus::Crash).with_baseline(None)
        );
        assert_eq!(
            Warn,
            RunnerStatus::from_deqp(TestStatus::Warn).with_baseline(None)
        );

        // Basic expected failures handling.
        assert_eq!(
            ExpectedFail,
            RunnerStatus::from_deqp(TestStatus::Fail).with_baseline(Some(Fail))
        );
        assert_eq!(
            ExpectedFail,
            RunnerStatus::from_deqp(TestStatus::Crash).with_baseline(Some(Crash))
        );
        assert_eq!(
            ExpectedFail,
            RunnerStatus::from_deqp(TestStatus::Timeout).with_baseline(Some(Timeout))
        );
        assert_eq!(
            UnexpectedImprovement(TestStatus::Pass),
            RunnerStatus::from_deqp(TestStatus::Pass).with_baseline(Some(Fail))
        );

        assert_eq!(
            UnexpectedImprovement(TestStatus::Fail),
            RunnerStatus::from_deqp(TestStatus::Fail).with_baseline(Some(Crash))
        );
        assert_eq!(
            UnexpectedImprovement(TestStatus::Fail),
            RunnerStatus::from_deqp(TestStatus::Fail).with_baseline(Some(Timeout))
        );
        assert_eq!(
            UnexpectedImprovement(TestStatus::Pass),
            RunnerStatus::from_deqp(TestStatus::Pass).with_baseline(Some(Timeout))
        );

        // Should be able to fee a run with a baseline as a new baseline (though you lose some Crash details)
        assert_eq!(
            ExpectedFail,
            RunnerStatus::from_deqp(TestStatus::Fail).with_baseline(Some(ExpectedFail))
        );
        assert_eq!(
            ExpectedFail,
            RunnerStatus::from_deqp(TestStatus::Crash).with_baseline(Some(ExpectedFail))
        );

        // If we expected this internal runner error and it stops happening, time to update expectaions
        assert_eq!(
            UnexpectedImprovement(TestStatus::Pass),
            RunnerStatus::from_deqp(TestStatus::Pass).with_baseline(Some(Missing))
        );
    }

    #[test]
    fn csv_parse() -> Result<()> {
        let results = RunnerResults::from_csv(
            &mut "
# This is a comment in a baseline CSV file, with a comma, to make sure we skip them.

dEQP-GLES2.info.version,Fail
piglit@crashy@test,Crash"
                .as_bytes(),
        )?;
        assert_eq!(
            results.get("dEQP-GLES2.info.version"),
            Some(RunnerResult {
                test: "dEQP-GLES2.info.version".to_string(),
                status: RunnerStatus::Fail,
                duration: 0.0,
                subtest: false,
            })
            .as_ref()
        );
        assert_eq!(
            results.get("piglit@crashy@test"),
            Some(RunnerResult {
                test: "piglit@crashy@test".to_string(),
                status: RunnerStatus::Crash,
                duration: 0.0,
                subtest: false,
            })
            .as_ref()
        );

        Ok(())
    }

    #[test]
    fn csv_parse_dup() {
        assert!(RunnerResults::from_csv(
            &mut "
dEQP-GLES2.info.version,Fail
dEQP-GLES2.info.version,Pass"
                .as_bytes(),
        )
        .is_err());
    }

    #[test]
    #[allow(clippy::string_lit_as_bytes)]
    fn csv_test_missing_status() {
        assert!(RunnerResults::from_csv(&mut "dEQP-GLES2.info.version".as_bytes()).is_err());
    }

    #[test]
    fn hms_display() {
        assert_eq!(format!("{}", HMSDuration(Duration::new(15, 20))), "15");
        assert_eq!(format!("{}", HMSDuration(Duration::new(0, 20))), "0");
        assert_eq!(format!("{}", HMSDuration(Duration::new(70, 20))), "1:10");
        assert_eq!(format!("{}", HMSDuration(Duration::new(69, 20))), "1:09");
        assert_eq!(
            format!("{}", HMSDuration(Duration::new(60 * 60 + 3, 20))),
            "1:00:03"
        );
        assert_eq!(
            format!("{}", HMSDuration(Duration::new(3735, 20))),
            "1:02:15"
        );
    }

    fn add_result(results: &mut RunnerResults, test: &str, status: RunnerStatus) {
        results.record_result(RunnerResult {
            test: test.to_string(),
            status,
            duration: 0.0,
            subtest: false,
        });
    }

    #[test]
    fn results_is_success() {
        let mut results = RunnerResults::new();

        add_result(&mut results, "pass1", RunnerStatus::Pass);
        add_result(&mut results, "pass2", RunnerStatus::Pass);

        assert!(results.is_success());

        add_result(&mut results, "Crash", RunnerStatus::Crash);

        assert!(!results.is_success());
    }
}

#[derive(Debug, StructOpt)]
pub struct JunitGeneratorOptions {
    #[structopt(long, help = "Testsuite name for junit")]
    testsuite: String,

    #[structopt(
        long,
        default_value = "",
        help = "Failure message template (with {{testcase}} replaced with the test name)"
    )]
    template: String,

    #[structopt(
        long,
        default_value = "0",
        help = "Number of junit cases to list (or 0 for unlimited)"
    )]
    limit: usize,
}
