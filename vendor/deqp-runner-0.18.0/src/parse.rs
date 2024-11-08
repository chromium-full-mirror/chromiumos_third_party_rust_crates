use anyhow::Result;
use log::*;
use std::io::{BufRead, BufReader, ErrorKind, Read};
use std::mem;
use std::time::Instant;

use crate::timeout::Timer;
use crate::{CaselistResult, TestResult, TestStatus};

#[derive(Debug)]
pub enum ParserState {
    BeginTest(String),
    SubTest(String, TestStatus),
    EndTest(TestStatus),
}

pub trait ResultParser: Sized {
    fn initialize(&mut self) -> Option<ParserState> {
        None
    }

    fn parse_line(&mut self, line: &str) -> Result<Option<ParserState>>;

    fn parse(self, output: impl Read) -> Result<CaselistResult> {
        self.parse_with_timer(output, None)
    }

    /// Test status to return if no other test result was found.  Typically we
    /// emit Crash if we didn't get a proper output, but some test commands may
    /// want to create Skip or Missing results if a test didn't even start.
    fn default_test_status(&self) -> TestStatus {
        TestStatus::Crash
    }

    fn parse_with_timer(
        mut self,
        output: impl Read,
        timer: Option<Timer>,
    ) -> Result<CaselistResult> {
        let output = BufReader::new(output);
        let mut stdout: Vec<String> = Vec::new();

        let mut reason = None;
        let mut current_test = None;
        let mut results = Vec::new();
        let mut subtests = Vec::new();
        let mut subtest_start = None;

        fn push_result(
            results: &mut Vec<TestResult>,
            name: String,
            status: TestStatus,
            time: Instant,
            subtests: Vec<TestResult>,
        ) {
            // Check for duplicate test result
            if let Some(pos) = results.iter().position(|x: &TestResult| x.name == name) {
                error!("Duplicate test found, marking test failed: {}", name);
                results[pos].status = TestStatus::Fail;
            } else {
                results.push(TestResult {
                    name,
                    status,
                    duration: time.elapsed(),
                    subtests,
                });
            }
        }

        if let Some(state) = self.initialize() {
            match state {
                ParserState::BeginTest(name) => {
                    current_test = Some((name, Instant::now()));
                    subtest_start = Some(Instant::now());
                }
                _ => panic!("Unexpected state while initializing parser: {:?}", state),
            }
        }

        for line in output.lines() {
            let line = match line {
                Ok(line) => line,
                Err(e) => {
                    if let ErrorKind::TimedOut = e.kind() {
                        reason = Some(TestStatus::Timeout);
                    } else {
                        reason = Some(TestStatus::Crash);
                    }
                    break;
                }
            };

            stdout.push(line);
            let line = stdout.last().unwrap();

            if let Some(state) = self.parse_line(line)? {
                match state {
                    ParserState::BeginTest(name) => {
                        current_test = Some((name, Instant::now()));
                        subtest_start = Some(Instant::now());
                        if let Some(ref timer) = timer {
                            timer.reset();
                        }
                    }
                    ParserState::SubTest(name, status) => {
                        let time = subtest_start.replace(Instant::now()).unwrap();
                        push_result(&mut subtests, name, status, time, vec![]);
                        if let Some(ref timer) = timer {
                            timer.reset();
                        }
                    }
                    ParserState::EndTest(status) => {
                        if let Some((name, time)) = current_test.take() {
                            push_result(&mut results, name, status, time, mem::take(&mut subtests));
                        }
                        subtest_start = None;
                    }
                }
            }
        }

        // End current test if any
        if let Some((name, time)) = current_test.take() {
            let status = reason.unwrap_or_else(|| self.default_test_status());
            push_result(&mut results, name, status, time, mem::take(&mut subtests));
        }

        Ok(CaselistResult { results, stdout })
    }
}
