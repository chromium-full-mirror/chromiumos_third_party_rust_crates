use anyhow::Result;
use rand::Rng;
use std::fmt::Display;
use std::io::prelude::*;
use structopt::StructOpt;

/// Mock skqp that uses conventions in the test name to control behavior of the
/// test.  We use this for integration testing of skqp-runner.

#[derive(Debug, StructOpt)]
pub struct MockSkqp {
    test: String,

    #[structopt(long)]
    #[allow(dead_code)]
    auto: bool,

    // Dump everything after '--' into here.  The -auto and -fbo will end up
    // here, since we can't represent them as clap args.
    #[allow(dead_code)]
    args: Vec<String>,
}

enum SkqpResult {
    Pass,
    Error,
    Fail,
}

impl Display for SkqpResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                SkqpResult::Pass => "Passed",
                SkqpResult::Error => "ERROR",
                SkqpResult::Fail => "FAILED",
            }
        )
    }
}

fn skqp_result(test_name: &str, result: SkqpResult) {
    println!("Starting: {}", test_name);
    println!("{}: {}", result, test_name);

    std::process::exit(match result {
        SkqpResult::Pass => 0,
        SkqpResult::Fail | SkqpResult::Error => 1,
    });
}

pub fn mock_skqp(mock: &MockSkqp) -> Result<()> {
    if mock.test.contains("@pass") {
        skqp_result("a", SkqpResult::Pass);
    } else if mock.test.contains("@error") {
        skqp_result("b", SkqpResult::Error);
    } else if mock.test.contains("@fail") {
        skqp_result("c", SkqpResult::Fail);
    } else if mock.test.contains("@test_statuses") {
        skqp_result("p", SkqpResult::Pass);
        skqp_result("f", SkqpResult::Fail);
        skqp_result("e", SkqpResult::Error);
    } else if mock.test.contains("@invalid") {
        std::process::exit(0);
    } else if mock.test.contains("@test_dupe") {
        skqp_result("test", SkqpResult::Pass);
        skqp_result("test", SkqpResult::Pass);
    } else if mock.test.contains("@flake") {
        if rand::thread_rng().gen::<bool>() {
            skqp_result("test", SkqpResult::Pass);
        } else {
            skqp_result("test", SkqpResult::Fail);
        }
    } else if mock.test.contains("@crash") {
        panic!("crashing!")
    } else if mock.test.contains("@late_crash") {
        println!("Starting: test");
        println!("Passed: test");
        std::io::stdout().flush().unwrap();
        panic!("crashing!")
    } else if mock.test.contains("@timeout") {
        // Simulate a testcase that doesn't return in time by infinite
        // looping.
        #[allow(clippy::empty_loop)]
        loop {}
    }

    panic!("Unknown test name");
}
