use anyhow::Result;
use deqp_runner::gtest_command::GTestCommand;
use deqp_runner::mock_gtest::MockGTest;
use deqp_runner::{
    parallel_test, process_results, CommandLineRunOptions, TestCommand, TestConfiguration,
};
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(
    author = "Emma Anholt <emma@anholt.net>",
    about = "Runs gtest in parallel (gtest binary must be single-threaded)"
)]
struct Opts {
    #[structopt(subcommand)]
    subcmd: SubCommand,
}

#[derive(Debug, StructOpt)]
#[allow(clippy::large_enum_variant)]
enum SubCommand {
    #[structopt(name = "run")]
    Run(Run),

    #[structopt(
        name = "mock-gtest",
        help = "gtest-runner internal mock gtest binary for testing"
    )]
    MockGTest(MockGTest),
}

#[derive(Debug, StructOpt)]
pub struct Run {
    #[structopt(long, help = "path to gtest binary")]
    gtest: PathBuf,

    #[structopt(flatten)]
    common: CommandLineRunOptions,

    #[structopt(
        long,
        default_value = "500",
        help = "Starting number of tests to include in each bin invocation (mitigates startup overhead)"
    )]
    tests_per_group: usize,

    #[structopt(
        long,
        default_value = "0",
        help = "Minimum number of tests to scale down to in each bin invocation (defaults to 0 to match tests_per_group)"
    )]
    min_tests_per_group: usize,

    #[structopt(help = "arguments to gtest binary")]
    gtest_args: Vec<String>,
}

fn main() -> Result<()> {
    let opts = Opts::from_args();

    match opts.subcmd {
        SubCommand::Run(run) => {
            run.common.setup()?;

            let include_filter = run.common.includes_regex()?;

            let gtest = GTestCommand {
                bin: run.gtest,
                config: TestConfiguration::from_cli(&run.common)?,
                args: run.gtest_args,
            };

            let tests = gtest
                .list_tests()?
                .into_iter()
                .skip(run.common.sub_config.fraction_start - 1)
                .step_by(run.common.sub_config.fraction)
                .filter(|x| include_filter.is_match(x.name()))
                .collect();

            let groups = gtest.split_tests_to_groups(
                tests,
                run.tests_per_group,
                run.min_tests_per_group,
                &run.common.sub_config,
                &[include_filter],
            )?;

            println!(
                "Running gtest on {} threads in {}-test groups",
                rayon::current_num_threads(),
                if let Some((_, tests)) = groups.get(0) {
                    tests.len()
                } else {
                    0
                }
            );

            let results = parallel_test(std::io::stdout(), groups)?;
            process_results(&results, &run.common.output_dir, run.common.summary_limit)?;
        }

        SubCommand::MockGTest(mock) => {
            stderrlog::new().module(module_path!()).init().unwrap();

            mock.run();
        }
    }

    Ok(())
}
