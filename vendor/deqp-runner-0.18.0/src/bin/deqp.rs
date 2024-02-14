use anyhow::{Context, Result};
use deqp_runner::cl_cts_command::{ClCtsCommand, ClCtsTomlConfig};
use deqp_runner::deqp_command::DeqpCommand;
use deqp_runner::mock_deqp::{mock_deqp, MockDeqp};
use deqp_runner::piglit_command::{PiglitCommand, PiglitTomlConfig};
use deqp_runner::skqp_command::{SkqpCommand, SkqpTomlConfig};
use deqp_runner::{
    parallel_test, parse_regex_set, process_results, read_lines, CommandLineRunOptions,
    RunnerResults, SubRunConfig, TestCase, TestCommand,
};
use deqp_runner::{JunitGeneratorOptions, TestConfiguration};
use log::*;
use serde::Deserialize;
use std::fs::File;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(
    author = "Emma Anholt <emma@anholt.net>",
    about = "Runs dEQP in parallel"
)]
struct Opts {
    #[structopt(subcommand)]
    subcmd: SubCommand,
}

#[derive(Debug, StructOpt)]
enum SubCommand {
    #[structopt(name = "run")]
    Run(Run),

    #[structopt(name = "junit")]
    Junit(Junit),

    #[structopt(name = "suite")]
    Suite(Suite),

    #[structopt(
        name = "mock-deqp",
        help = "deqp-runner internal mock deqp binary for testing"
    )]
    MockDeqp(MockDeqp),
}

#[derive(Debug, StructOpt)]
pub struct DeqpRunnerGlobalOptions {
    #[structopt(
        long,
        help = "Optional path to store the deqp-vk .shader_cache files.  Must not be shared with any other deqp-runner invocations in progress."
    )]
    shader_cache_dir: Option<PathBuf>,

    #[structopt(
        long,
        help = "Optional path to executor/testlog-to-xml, for converting QPA files to usable XML"
    )]
    testlog_to_xml: Option<PathBuf>,
}

#[derive(Debug, StructOpt)]
pub struct Run {
    #[structopt(long, help = "path to deqp binary")]
    deqp: PathBuf,

    #[structopt(long, help = "path to deqp caselist (such as *-mustpass.txt)")]
    caselist: Vec<PathBuf>,

    #[structopt(flatten)]
    common: CommandLineRunOptions,

    #[structopt(flatten)]
    deqp_global: DeqpRunnerGlobalOptions,

    #[structopt(flatten)]
    deqp_config: DeqpRunConfig,
}

#[derive(Debug, StructOpt)]
pub struct Junit {
    #[structopt(long, help = "Path to source results.csv or failures.csv")]
    results: PathBuf,

    #[structopt(long, short = "o", help = "Path to write junit XML to")]
    output: PathBuf,

    #[structopt(flatten)]
    junit_generator_options: JunitGeneratorOptions,
}

#[derive(Debug, StructOpt)]
pub struct Suite {
    #[structopt(long, help = "Path to suite.toml")]
    suite: PathBuf,

    #[structopt(flatten)]
    common: CommandLineRunOptions,

    #[structopt(flatten)]
    deqp_global: DeqpRunnerGlobalOptions,
}

fn default_deqp_tests_per_group() -> usize {
    500
}

// CLI/toml options for a dEQP run
#[derive(Debug, Deserialize, StructOpt)]
struct DeqpRunConfig {
    #[structopt(
        long,
        default_value = "500",
        help = "Starting number of tests to include in each deqp invocation (mitigates deqp startup overhead)"
    )]
    #[serde(default = "default_deqp_tests_per_group")]
    tests_per_group: usize,

    // The runner  corehas clever code I inherited from the parallel-deqp-runner
    // project to, near the end of the list of test of tests to shard out, make
    // smaller groups of tests so that you don't end up with a long test list
    // left in one deqp process limiting your total runtime.
    //
    // The problem that cleverness faces is this table of deqp startup time:
    //
    //             freedreno  radeon
    // deqp-gles2  0.2s       0.08s
    // deqp-vk     2.0s       0.6s
    //
    // Even if all the slowest tests I have (a few deqp-vks on radeon at 6-7s)
    // land in the same test list near the end of the run, you're still going to
    // end up spawning a ton of extra deqp processes trying to be clever, costing
    // more than your tests.
    //
    // So, disable it by default by using our normal group size the whole time
    // unless someone sets the option to something else.
    #[structopt(
        long,
        default_value = "0",
        help = "Minimum number of tests to scale down to in each deqp invocation (defaults to 0 to match tests_per_group)"
    )]
    #[serde(default)]
    min_tests_per_group: usize,

    #[structopt(
        long,
        default_value = "",
        help = "regex to match against the GL_RENDERER or VK deviceName"
    )]
    #[serde(default)]
    renderer_check: String,

    #[structopt(
        long,
        default_value = "",
        help = "text file of GL/EGL extensions to match the impl as supporting"
    )]
    #[serde(default)]
    extensions_check: String,

    #[structopt(
        long,
        default_value = "",
        help = "regex to match against the GL_VERSION"
    )]
    #[serde(default)]
    version_check: String,

    #[structopt(help = "arguments to deqp binary")]
    #[serde(default)]
    deqp_args: Vec<String>,
}

// Common structure for configuring a deqp run between Run (single run) and Suite (muliple Runs)
#[derive(Deserialize)]
struct DeqpTomlConfig {
    deqp: PathBuf,

    caselists: Vec<PathBuf>,

    #[serde(flatten)]
    pub sub_config: SubRunConfig,

    #[serde(flatten)]
    pub deqp_config: DeqpRunConfig,

    #[serde(default)]
    prefix: String,
}

impl DeqpTomlConfig {
    fn deqp(
        &self,
        run: &CommandLineRunOptions,
        deqp_global: &DeqpRunnerGlobalOptions,
    ) -> Result<DeqpCommand> {
        Ok(DeqpCommand {
            deqp: self.deqp.clone(),
            args: self.deqp_config.deqp_args.clone(),
            config: TestConfiguration::from_suite_config(run, &self.sub_config)?,
            shader_cache_dir: deqp_global
                .shader_cache_dir
                .as_ref()
                .unwrap_or(&run.output_dir)
                .clone(),
            qpa_to_xml: deqp_global.testlog_to_xml.clone(),
            prefix: self.prefix.to_owned(),
        })
    }

    fn test_groups<'d>(
        &self,
        deqp: &'d DeqpCommand,
        filters: &[String],
    ) -> Result<Vec<(&'d dyn TestCommand, Vec<TestCase>)>> {
        if self.deqp_config.tests_per_group < 1 {
            eprintln!("tests_per_group must be >= 1.");
            std::process::exit(1);
        }

        let rayon_threads = rayon::current_num_threads();

        let mut include_filters = Vec::new();
        if !self.sub_config.include.is_empty() {
            include_filters.push(
                parse_regex_set(&self.sub_config.include)
                    .context("compiling sub_config include filters")?,
            );
        }
        if !filters.is_empty() {
            include_filters.push(parse_regex_set(filters).context("compiling include filters")?);
        }

        let test_names: Vec<TestCase> = read_lines(&self.caselists)?
            .into_iter()
            .map(TestCase::Named)
            .collect::<Vec<TestCase>>();

        if !test_names.is_empty() {
            // Look at the testcases in the caselist to decide how to probe the
            // renderer.  Note that we do some inference, because the caselist
            // might not contain dEQP-GLESn.info.renderer or whatever in it.
            //
            // The alternative would be to use the testcase binary to infer, but
            // then we don't know if we should be printing
            // KHR-GLES32.info.renderer or KHR-GL33.info.renderer or whatever.
            let deqp_version = test_names[0]
                .name()
                .split('.')
                .next()
                .context("finding dEQP case prefix")?;

            match deqp_version {
                "dEQP-VK" => {
                    if !deqp.qpa_vk_device_name_check(&self.deqp_config.renderer_check)? {
                        error!("Renderer mismatch ({})", &self.deqp_config.renderer_check);
                        std::process::exit(1);
                    }
                }

                "dEQP-EGL" => {
                    if !self.deqp_config.renderer_check.is_empty()
                        || !self.deqp_config.version_check.is_empty()
                    {
                        anyhow::bail!("No renderer/version check implemented for EGL.");
                    }
                    let testcase = "dEQP-EGL.info.extensions";
                    let qpa = deqp.deqp_test_qpa_output(testcase, &format!("{}.qpa", testcase))?;
                    if !deqp.qpa_extensions_check(
                        &qpa,
                        testcase,
                        &self.deqp_config.extensions_check,
                    )? {
                        std::process::exit(1);
                    }
                }

                "KHR-GLES2" | "KHR-GLES3" | "KHR-GLES31" => {
                    if !self.deqp_config.renderer_check.is_empty()
                        || !self.deqp_config.version_check.is_empty()
                        || !self.deqp_config.extensions_check.is_empty()
                    {
                        anyhow::bail!(
                            "Can't do a renderer check based on testcase name {}",
                            test_names[0].name()
                        );
                    }
                }

                _ => {
                    let deqp_info_renderer = format!("{}.info.renderer", deqp_version);
                    let deqp_info_version = format!("{}.info.version", deqp_version);
                    let deqp_info_extensions = format!("{}.info.extensions", deqp_version);

                    // Make sure that we test (and thus log) both GL renderer and version
                    // before exiting out due to a mismatch.
                    let qpa = deqp.deqp_test_qpa_output(
                        &[
                            deqp_info_renderer.as_str(),
                            deqp_info_version.as_str(),
                            deqp_info_extensions.as_str(),
                        ]
                        .join(","),
                        &format!("{}.info.qpa", deqp_version),
                    )?;
                    let renderer_ok = deqp
                        .qpa_gl_renderer_version_check(
                            &qpa,
                            &deqp_info_renderer,
                            &self.deqp_config.renderer_check,
                            "renderer",
                        )
                        .context("checking renderer")?;
                    let version_ok = deqp
                        .qpa_gl_renderer_version_check(
                            &qpa,
                            &deqp_info_version,
                            &self.deqp_config.version_check,
                            "version",
                        )
                        .context("checking version")?;

                    if !renderer_ok {
                        error!("Renderer mismatch ({})", &self.deqp_config.renderer_check);
                        std::process::exit(1);
                    }

                    if !version_ok {
                        error!("Version mismatch ({})", &self.deqp_config.version_check);
                        std::process::exit(1);
                    }

                    if !deqp.qpa_extensions_check(
                        &qpa,
                        &deqp_info_extensions,
                        &self.deqp_config.extensions_check,
                    )? {
                        std::process::exit(1);
                    }
                }
            };
        }

        let groups = deqp.split_tests_to_groups(
            test_names,
            self.deqp_config.tests_per_group,
            self.deqp_config.min_tests_per_group,
            &self.sub_config,
            &include_filters,
        )?;

        println!(
            "Running dEQP on {} threads in {}-test groups",
            rayon_threads,
            if let Some((_, tests)) = groups.get(0) {
                tests.len()
            } else {
                0
            }
        );

        Ok(groups)
    }
}

#[derive(Deserialize)]
struct SuiteConfig {
    #[serde(default)]
    deqp: Vec<DeqpTomlConfig>,
    #[serde(default)]
    piglit: Vec<PiglitTomlConfig>,
    #[serde(default)]
    cl_cts: Vec<ClCtsTomlConfig>,
    #[serde(default)]
    skqp: Vec<SkqpTomlConfig>,
}

fn main() -> Result<()> {
    let opts = Opts::from_args();

    match opts.subcmd {
        SubCommand::Run(run) => {
            run.common.setup()?;

            let config = DeqpTomlConfig {
                deqp: run.deqp,
                caselists: run.caselist,
                sub_config: run.common.sub_config.clone(),
                deqp_config: run.deqp_config,
                prefix: "".to_owned(),
            };
            let deqp = config.deqp(&run.common, &run.deqp_global)?;

            let results = parallel_test(std::io::stdout(), config.test_groups(&deqp, &[])?)?;
            process_results(&results, &run.common.output_dir, run.common.summary_limit)?;
        }

        SubCommand::Suite(suite) => {
            suite.common.setup()?;

            let toml_str = std::fs::read_to_string(&suite.suite).context("Reading config TOML")?;
            let mut suite_config =
                toml::from_str::<SuiteConfig>(toml_str.as_str()).context("Parsing config TOML")?;

            // Apply defaults to the configs.
            for config in &mut suite_config.deqp {
                config
                    .sub_config
                    .apply_suite_top_config(&suite.common.sub_config);
            }

            for config in &mut suite_config.piglit {
                config
                    .sub_config
                    .apply_suite_top_config(&suite.common.sub_config);
            }

            for config in &mut suite_config.cl_cts {
                config
                    .sub_config
                    .apply_suite_top_config(&suite.common.sub_config);
            }

            for config in &mut suite_config.skqp {
                config
                    .sub_config
                    .apply_suite_top_config(&suite.common.sub_config);
            }

            let mut deqp = Vec::new();
            for config in &suite_config.deqp {
                deqp.push(config.deqp(&suite.common, &suite.deqp_global)?);
            }

            let mut piglit = Vec::new();
            for config in &suite_config.piglit {
                piglit.push(PiglitCommand {
                    piglit_folder: config.piglit_config.piglit_folder.clone(),
                    config: TestConfiguration::from_suite_config(
                        &suite.common,
                        &config.sub_config,
                    )?,
                    prefix: config.prefix.clone(),
                });
            }

            let mut cl_cts = Vec::new();
            for config in &suite_config.cl_cts {
                cl_cts.push(ClCtsCommand {
                    conformance_dir: config.conformance_dir(),
                    config: TestConfiguration::from_suite_config(
                        &suite.common,
                        &config.sub_config,
                    )?,
                    prefix: config.prefix.clone(),
                });
            }

            let mut skqp = Vec::new();
            for config in &suite_config.skqp {
                skqp.push(SkqpCommand {
                    skqp: config.skqp_config.skqp.to_owned(),
                    config: TestConfiguration::from_suite_config(
                        &suite.common,
                        &config.sub_config,
                    )?,
                    skqp_assets: config.skqp_config.skqp_assets.to_owned(),
                });
            }

            let mut test_groups = Vec::new();
            for (config, deqp) in suite_config.deqp.iter().zip(deqp.iter()) {
                let mut groups = config.test_groups(deqp, &suite.common.sub_config.include)?;
                test_groups.append(&mut groups);
            }
            for (config, piglit) in suite_config.piglit.iter().zip(piglit.iter()) {
                let mut groups = config.test_groups(piglit, &suite.common.sub_config.include)?;
                test_groups.append(&mut groups);
            }
            for (config, cl_cts) in suite_config.cl_cts.iter().zip(cl_cts.iter()) {
                let mut groups = config.test_groups(cl_cts, &suite.common.sub_config.include)?;
                test_groups.append(&mut groups);
            }
            for (config, skqp) in suite_config.skqp.iter().zip(skqp.iter()) {
                let mut groups = config.test_groups(skqp, &suite.common.sub_config.include)?;
                test_groups.append(&mut groups);
            }

            let results = parallel_test(std::io::stdout(), test_groups)?;
            process_results(
                &results,
                &suite.common.output_dir,
                suite.common.summary_limit,
            )?;
        }

        SubCommand::Junit(junit) => {
            stderrlog::new().module(module_path!()).init().unwrap();

            let results = RunnerResults::from_csv(&mut File::open(&junit.results)?)
                .context("Reading in results csv")?;

            results.write_junit_failures(
                &mut File::create(&junit.output)?,
                &junit.junit_generator_options,
            )?;
        }

        SubCommand::MockDeqp(mock) => {
            stderrlog::new().module(module_path!()).init().unwrap();

            mock_deqp(&mock)?;
        }
    }

    Ok(())
}
