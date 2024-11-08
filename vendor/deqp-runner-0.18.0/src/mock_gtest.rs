use rand::Rng;
use std::fmt::Display;
use structopt::StructOpt;

/// Mock gtest that uses conventions in the test name to control behavior of the
/// test.  We use this for integration testing of gtest-runner.

#[derive(Debug, StructOpt)]
pub struct MockGTest {
    #[structopt(long = "gtest_filter", default_value = "")]
    gtest_filter: String,

    #[structopt(long = "gtest_list_tests")]
    gtest_list_tests: bool,
}

#[derive(Debug, PartialEq)]
enum GTestResult {
    Pass,
    Skip,
    Fail,
}

impl Display for GTestResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                GTestResult::Pass => "OK",
                GTestResult::Skip => "SKIPPED",
                GTestResult::Fail => "FAILED",
            }
        )
    }
}

fn gtest_result(test: &str, result: GTestResult) {
    let mut result = result;

    /* skips print an OK after the skip explanation. */
    if result == GTestResult::Skip {
        println!("[ {} ] some reason we couldn't run it", result);
        result = GTestResult::Pass;
    }

    println!("[ {} ] {} (123 ms)", result, test);

    std::process::exit(match result {
        GTestResult::Pass | GTestResult::Skip => 0,
        GTestResult::Fail => 1,
    });
}

fn start_test(test: &str) {
    println!("[ RUN      ] {}", test);
}

impl MockGTest {
    pub fn run(&self) {
        if self.gtest_list_tests {
            println!("Group1.");
            for i in 0..10 {
                println!("  pass/{}", i);
            }
            for i in 0..11 {
                println!("  skip/{}", i);
            }
            for i in 0..12 {
                println!("  fail/{}", i);
            }
            println!("Group2.");
            println!("  crash");
            println!("  flake");
            println!("  timeout");
            println!("  stderr");
            std::process::exit(0);
        }

        for test in self.gtest_filter.split(':') {
            if test.contains(".missing") {
                continue;
            }

            start_test(test);

            if test.contains(".pass") {
                gtest_result(test, GTestResult::Pass);
            } else if test.contains(".skip") {
                gtest_result(test, GTestResult::Skip);
            } else if test.contains(".fail") {
                gtest_result(test, GTestResult::Fail);
            } else if test.contains(".stderr") {
                eprintln!("Output to stderr");
                gtest_result(test, GTestResult::Fail);
            } else if test.contains(".flake") {
                if rand::thread_rng().gen::<bool>() {
                    gtest_result(test, GTestResult::Pass);
                } else {
                    gtest_result(test, GTestResult::Fail);
                }
            } else if test.contains(".crash") {
                panic!("crashing!")
            } else if test.contains(".timeout") {
                // Simulate a testcase that doesn't return in time by infinite
                // looping.
                #[allow(clippy::empty_loop)]
                loop {}
            }

            panic!("Unknown test name");
        }
    }
}
