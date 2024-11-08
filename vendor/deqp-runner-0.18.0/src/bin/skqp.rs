use anyhow::Result;
use deqp_runner::mock_skqp::{mock_skqp, MockSkqp};
use deqp_runner::skqp_command::{SkqpCommand, SkqpRunConfig, SkqpTomlConfig};
use deqp_runner::{parallel_test, process_results, CommandLineRunOptions, TestConfiguration};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(
    author = "Helen Koike <helen.koike@collabora.com>",
    about = "Runs skqp"
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
        name = "mock-skqp",
        help = "skqp-runner internal mock skqp binary for testing"
    )]
    MockSkqp(MockSkqp),
}

#[derive(Debug, StructOpt)]
pub struct Run {
    #[structopt(flatten)]
    pub common: CommandLineRunOptions,

    #[structopt(flatten)]
    pub skqp_config: SkqpRunConfig,
}

fn main() -> Result<()> {
    let opts = Opts::from_args();

    match opts.subcmd {
        SubCommand::Run(run) => {
            run.common.setup()?;

            let config = SkqpTomlConfig {
                sub_config: run.common.sub_config.clone(),
                skqp_config: run.skqp_config,
            };

            let skqp = SkqpCommand {
                skqp: config.skqp_config.skqp.clone(),
                config: TestConfiguration::from_cli(&run.common)?,
                skqp_assets: config.skqp_config.skqp_assets.clone(),
            };
            let results = parallel_test(std::io::stdout(), config.test_groups(&skqp, &[])?)?;
            process_results(&results, &run.common.output_dir, run.common.summary_limit)?;
        }

        SubCommand::MockSkqp(mock) => {
            stderrlog::new().module(module_path!()).init().unwrap();

            mock_skqp(&mock)?;
        }
    }

    Ok(())
}
