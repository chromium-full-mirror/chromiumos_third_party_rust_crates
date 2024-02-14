use anyhow::Result;
use rand::Rng;
use std::fmt::Display;
use std::io::prelude::*;
use structopt::StructOpt;

/// Mock piglit that uses conventions in the test name to control behavior of the
/// test.  We use this for integration testing of piglit-runner.

#[derive(Debug, StructOpt)]
pub struct MockPiglit {
    test: String,

    #[structopt(long)]
    #[allow(dead_code)]
    auto: bool,

    // Dump everything after '--' into here.  The -auto and -fbo will end up
    // here, since we can't represent them as clap args.
    #[allow(dead_code)]
    args: Vec<String>,
}

enum PiglitResult {
    Pass,
    Skip,
    Fail,
}

impl Display for PiglitResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                PiglitResult::Pass => "pass",
                PiglitResult::Skip => "skip",
                PiglitResult::Fail => "fail",
            }
        )
    }
}

fn piglit_result(result: PiglitResult) {
    println!("PIGLIT: {{\"result\": \"{}\" }}", result);

    std::process::exit(match result {
        PiglitResult::Pass | PiglitResult::Skip => 0,
        PiglitResult::Fail => 1,
    });
}

fn piglit_subtest_result(name: &str, result: PiglitResult) {
    println!(
        "PIGLIT: {{\"subtest\": {{\"{name}\" : \"{result}\"}}}}",
        name = name,
        result = result
    );
}

pub fn mock_piglit(mock: &MockPiglit) -> Result<()> {
    if mock.test.contains("@pass") {
        piglit_result(PiglitResult::Pass);
    } else if mock.test.contains("@skip") {
        piglit_result(PiglitResult::Skip);
    } else if mock.test.contains("@fail") {
        piglit_result(PiglitResult::Fail);
    } else if mock.test.contains("@subtest_statuses") {
        piglit_subtest_result("p", PiglitResult::Pass);
        piglit_subtest_result("f", PiglitResult::Fail);
        piglit_subtest_result("s", PiglitResult::Skip);
        piglit_result(PiglitResult::Fail);
    } else if mock.test.contains("@subtest_commas") {
        piglit_subtest_result(
            "GL_INTENSITY12, swizzled, border color only",
            PiglitResult::Fail,
        );
        piglit_subtest_result(
            "GL_INTENSITY16, swizzled, border color only",
            PiglitResult::Fail,
        );
        piglit_result(PiglitResult::Fail);
    } else if mock.test.contains("@subtest_dupe") {
        piglit_subtest_result("subtest", PiglitResult::Pass);
        piglit_subtest_result("subtest", PiglitResult::Pass);
        piglit_result(PiglitResult::Pass);
    } else if mock.test.contains("@flake") {
        if rand::thread_rng().gen::<bool>() {
            piglit_result(PiglitResult::Pass);
        } else {
            piglit_result(PiglitResult::Fail);
        }
    } else if mock.test.contains("@crash") {
        panic!("crashing!")
    } else if mock.test.contains("@late_crash") {
        println!("PIGLIT: {{\"result\": \"pass\" }}");
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
