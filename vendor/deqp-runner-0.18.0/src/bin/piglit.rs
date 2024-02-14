// Copyright (c) 2021 Advanced Micro Devices, Inc.
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of this software and associated documentation files (the "Software"),
// to deal in the Software without restriction, including without limitation
// the rights to use, copy, modify, merge, publish, distribute, sublicense,
// and/or sell copies of the Software, and to permit persons to whom the
// Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice (including the next
// paragraph) shall be included in all copies or substantial portions of the
// Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL
// THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
// DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR
// OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE
// USE OR OTHER DEALINGS IN THE SOFTWARE.

use anyhow::Result;
use deqp_runner::mock_piglit::{mock_piglit, MockPiglit};
use deqp_runner::piglit_command::{PiglitCommand, PiglitRunConfig, PiglitTomlConfig};
use deqp_runner::{parallel_test, process_results, CommandLineRunOptions, TestConfiguration};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(
    author = "Emma Anholt <emma@anholt.net>",
    about = "Runs piglit in parallel"
)]
struct Opts {
    #[structopt(subcommand)]
    subcmd: SubCommand,
}

#[derive(Debug, StructOpt)]
enum SubCommand {
    #[structopt(name = "run")]
    Run(Run),
    #[structopt(name = "mock-piglit")]
    MockPiglit(MockPiglit),
}

#[derive(Debug, StructOpt)]
pub struct Run {
    #[structopt(flatten)]
    pub common: CommandLineRunOptions,

    #[structopt(flatten)]
    pub piglit_config: PiglitRunConfig,
}

fn main() -> Result<()> {
    let opts = Opts::from_args();

    match opts.subcmd {
        SubCommand::Run(run) => {
            run.common.setup()?;

            let config = PiglitTomlConfig {
                sub_config: run.common.sub_config.clone(),
                piglit_config: run.piglit_config,
                prefix: "".to_owned(),
            };

            let piglit = PiglitCommand {
                piglit_folder: config.piglit_config.piglit_folder.clone(),
                config: TestConfiguration::from_cli(&run.common)?,
                prefix: "".to_owned(),
            };
            let results = parallel_test(std::io::stdout(), config.test_groups(&piglit, &[])?)?;
            process_results(&results, &run.common.output_dir, run.common.summary_limit)?;
        }

        SubCommand::MockPiglit(mock) => {
            stderrlog::new().module(module_path!()).init().unwrap();

            mock_piglit(&mock)?;
        }
    }

    Ok(())
}
