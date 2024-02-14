use anyhow::Result;
use rand::Rng;
use std::fmt::Display;
use std::io::prelude::*;
use structopt::StructOpt;

/// Mock igt that uses conventions in the test name to control behavior of the
/// test.  We use this for integration testing of igt-runner.

#[derive(Debug, StructOpt)]
pub struct MockIgt {
    test: String,

    #[structopt(long)]
    #[allow(dead_code)]
    auto: bool,

    // Dump everything after '--' into here.  The -auto and -fbo will end up
    // here, since we can't represent them as clap args.
    #[allow(dead_code)]
    args: Vec<String>,
}

const IGT_EXIT_SUCCESS: i32 = 0;
const IGT_EXIT_INVALID: i32 = 79;
const IGT_EXIT_FAILURE: i32 = 98;
const IGT_EXIT_SKIP: i32 = 77;

enum IgtResult {
    Pass,
    Skip,
    Fail,
}

impl Display for IgtResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                IgtResult::Pass => "SUCCESS",
                IgtResult::Skip => "SKIP",
                IgtResult::Fail => "FAIL",
            }
        )
    }
}

impl IgtResult {
    fn exit_code(&self) -> i32 {
        match self {
            IgtResult::Pass => IGT_EXIT_SUCCESS,
            IgtResult::Skip => IGT_EXIT_SKIP,
            IgtResult::Fail => IGT_EXIT_FAILURE,
        }
    }
}

fn igt_result(result: IgtResult) {
    println!("{} (0.004s)", result);

    std::process::exit(result.exit_code());
}

fn igt_subtest_result(name: &str, result: IgtResult) {
    println!(
        "Subtest {name}: {result} (0.006s)",
        name = name,
        result = result
    );
}

pub fn mock_igt(mock: &MockIgt) -> Result<()> {
    if mock.test.contains("@pass") {
        igt_result(IgtResult::Pass);
    } else if mock.test.contains("@skip") {
        igt_result(IgtResult::Skip);
    } else if mock.test.contains("@fail") {
        igt_result(IgtResult::Fail);
    } else if mock.test.contains("@subtest_statuses") {
        igt_subtest_result("p", IgtResult::Pass);
        igt_subtest_result("f", IgtResult::Fail);
        igt_subtest_result("s", IgtResult::Skip);
        igt_result(IgtResult::Fail);
    } else if mock.test.contains("@invalid") {
        std::process::exit(IGT_EXIT_INVALID);
    } else if mock.test.contains("@subtest_dupe") {
        igt_subtest_result("subtest", IgtResult::Pass);
        igt_subtest_result("subtest", IgtResult::Pass);
        igt_result(IgtResult::Pass);
    } else if mock.test.contains("@flake") {
        if rand::thread_rng().gen::<bool>() {
            igt_result(IgtResult::Pass);
        } else {
            igt_result(IgtResult::Fail);
        }
    } else if mock.test.contains("@crash") {
        panic!("crashing!")
    } else if mock.test.contains("@late_crash") {
        println!("SUCCESS (0.004s)");
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
