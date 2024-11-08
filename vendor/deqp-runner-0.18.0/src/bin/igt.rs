//! Module for invoking [IGT](https://gitlab.freedesktop.org/drm/igt-gpu-tools)
//! and processing its results.

use anyhow::Result;
use deqp_runner::igt_command::{IgtCommand, IgtRunConfig, IgtTomlConfig};
use deqp_runner::mock_igt::{mock_igt, MockIgt};
use deqp_runner::{parallel_test, process_results, CommandLineRunOptions, TestConfiguration};
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(
    author = "Tomeu Vizoso <tomeu.vizoso@collabora.com>",
    about = "Runs IGT"
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
        name = "mock-igt",
        help = "igt-runner internal mock IGT binary for testing"
    )]
    MockIgt(MockIgt),
}

#[derive(Debug, StructOpt)]
pub struct Run {
    #[structopt(flatten)]
    pub common: CommandLineRunOptions,

    #[structopt(flatten)]
    pub igt_config: IgtRunConfig,

    #[structopt(long, help = "path to igt caselist")]
    caselist: Vec<PathBuf>,
}

fn main() -> Result<()> {
    let opts = Opts::from_args();

    match opts.subcmd {
        SubCommand::Run(run) => {
            run.common.setup()?;

            let config = IgtTomlConfig {
                sub_config: run.common.sub_config.clone(),
                igt_config: run.igt_config,
                caselists: run.caselist,
            };

            let igt = IgtCommand {
                config: TestConfiguration::from_cli(&run.common)?,
                igt_folder: config.igt_config.igt_folder.clone(),
            };
            let results = parallel_test(std::io::stdout(), config.test_groups(&igt, &[])?)?;
            process_results(&results, &run.common.output_dir, run.common.summary_limit)?;
        }

        SubCommand::MockIgt(mock) => {
            stderrlog::new().module(module_path!()).init().unwrap();

            mock_igt(&mock)?;
        }
    }

    Ok(())
}
