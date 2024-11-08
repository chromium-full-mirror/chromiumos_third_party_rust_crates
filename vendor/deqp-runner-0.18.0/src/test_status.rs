use std::{str::FromStr, time::Duration};

/// Enum for the basic types of test result that we track
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TestStatus {
    Pass,
    Fail,
    Warn,
    Skip,
    Crash,
    Timeout,
}

impl std::fmt::Display for TestStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl FromStr for TestStatus {
    type Err = anyhow::Error;

    // Parses the status name from dEQP's output.
    fn from_str(input: &str) -> Result<TestStatus, Self::Err> {
        match input {
            "Pass" => Ok(TestStatus::Pass),
            "Fail" => Ok(TestStatus::Fail),
            "Skip" => Ok(TestStatus::Skip),
            "Warn" => Ok(TestStatus::Warn),
            "Crash" => Ok(TestStatus::Crash),
            "Timeout" => Ok(TestStatus::Timeout),
            _ => anyhow::bail!("unknown test status '{}'", input),
        }
    }
}

/// A test result entry in a CaselistResult from running a group of tests
#[derive(Debug)]
pub struct TestResult {
    pub name: String,
    pub status: TestStatus,
    pub duration: Duration,
    pub subtests: Vec<TestResult>,
}

/// The collection of results from invoking a binary to run a bunch of tests (or
/// parsing an old results.csv back off disk).
#[derive(Debug)]
pub struct CaselistResult {
    pub results: Vec<TestResult>,
    pub stdout: Vec<String>,
}

// For comparing equality, we ignore the test runtime (particularly of use for the unit tests )
impl PartialEq for TestResult {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.status == other.status && self.subtests == other.subtests
    }
}

// For comparing equality, we ignore the test runtime (particularly of use for the unit tests )
impl PartialEq for CaselistResult {
    fn eq(&self, other: &Self) -> bool {
        self.results == other.results && self.stdout == other.stdout
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_status_display() {
        assert_eq!(format!("{}", TestStatus::Pass), "Pass".to_owned());
    }
    #[test]
    fn test_status_parse() {
        assert_eq!(
            "Warn".parse::<TestStatus>().expect("parsing"),
            TestStatus::Warn
        );
    }
}
