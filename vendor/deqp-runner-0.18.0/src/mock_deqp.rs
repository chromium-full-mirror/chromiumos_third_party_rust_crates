use anyhow::{Context, Result};
use rand::Rng;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::{fs::File, path::PathBuf};
use structopt::StructOpt;

/// Mock deqp that uses conventions in the test name to control behavior of the
/// test.  We use this for integration testing of deqp-runner.

#[derive(Debug, StructOpt)]
pub struct MockDeqp {
    #[structopt(long, help = "Path to the caselist file")]
    deqp_caselist_file: Option<PathBuf>,

    #[structopt(long, default_value = "", help = "individual deqp case")]
    deqp_case: String,

    #[structopt(long, help = "Path to the QPA log output")]
    deqp_log_filename: PathBuf,

    #[structopt(long)]
    #[allow(dead_code)]
    deqp_log_flush: bool,

    #[structopt(long, default_value = "", help = "Path to the .shader_cache output")]
    #[allow(dead_code)]
    deqp_shadercache_filename: PathBuf,

    #[structopt(long)]
    #[allow(dead_code)]
    deqp_shadercache_truncate: bool,

    #[structopt(long)]
    all_pass: bool,
}

pub fn mock_deqp(mock: &MockDeqp) -> Result<()> {
    let tests = if let Some(path) = &mock.deqp_caselist_file {
        let file =
            File::open(path).with_context(|| format!("Opening {:?}", &mock.deqp_caselist_file))?;

        BufReader::new(file)
            .lines()
            .collect::<std::result::Result<Vec<String>, std::io::Error>>()
            .context("reading test caselist")?
    } else {
        mock.deqp_case.split(',').map(|x| x.to_string()).collect()
    };

    let qpa = File::create(&mock.deqp_log_filename).context("Creating QPA file")?;
    let mut qpa_writer = BufWriter::new(&qpa);

    // Set up our QPA file before starting to write tests.  We use a snapshot of
    let qpa_header = if tests[0].contains("dEQP-GLES2") {
        r#"
#sessionInfo releaseName git-39e5966401d69eba352d71404827230b90d3063b
#sessionInfo releaseId 0x39e59664
#sessionInfo targetName "Surfaceless"
#sessionInfo vendor "Mesa/X.org"
#sessionInfo renderer "llvmpipe (LLVM 11.0.1, 256 bits)"
#sessionInfo commandLineParameters "--deqp-log-images=enable --deqp-log-filename=/home/anholt/TestResults.qpa --deqp-surface-type=pbuffer --deqp-surface-width=256 --deqp-surface-height=256 --deqp-gl-config-name=rgba8888d24s8ms0 --deqp-case=dEQP-GLES2.info.version"

#beginSession
"#
    } else {
        r#"
#sessionInfo releaseName git-39e5966401d69eba352d71404827230b90d3063b
#sessionInfo releaseId 0x39e59664
#sessionInfo targetName "Surfaceless"
#sessionInfo vendorID 0x1002
#sessionInfo deviceID 0x687f
#sessionInfo commandLineParameters "--deqp-log-images=enable --deqp-log-filename=/home/anholt/TestResults.qpa --deqp-surface-type=pbuffer --deqp-surface-width=256 --deqp-surface-height=256 --deqp-gl-config-name=rgba8888d24s8ms0 --deqp-case=dEQP-VK.info.device"

#beginSession
"#
    };
    qpa_writer
        .write(qpa_header.as_bytes())
        .context("writing QPA header")?;

    for test_name in tests {
        // Missing tests won't appear in the output at all.
        if test_name.contains("dEQP-GLES2.test.m.") {
            continue;
        }

        println!("Test case '{}'..", test_name);
        writeln!(qpa_writer, "#beginTestCaseResult {}", test_name)
            .context("writing QPA test start")?;

        if test_name.contains(".timeout.") {
            // Simulate a testcase that doesn't return in time by infinite
            // looping.
            #[allow(clippy::empty_loop)]
            loop {}
        }

        if test_name.contains(".p.") || mock.all_pass {
            println!("  Pass (success case)");
        } else if test_name.contains(".f.") {
            println!("  Fail (failure case)");
        } else if test_name.contains(".flaky") {
            if rand::thread_rng().gen::<bool>() {
                println!("  Fail (failure case)");
            } else {
                println!("  Pass (success)");
            }
        } else if test_name.contains(".s.") {
            println!("  NotSupported (skip case)");
        } else if test_name.contains(".c.") {
            // In a crash, the output just stops before we get a result and
            // the process returns an error code.  parse_deqp_results() just
            // handles the deqp output unexpectedly ending as a crash.
            break;
        } else if test_name == "dEQP-GLES2.info.renderer" {
            qpa_writer
                .write(include_bytes!("test_data/deqp-gles2-renderer.xml"))
                .context("writing QPA XML")?;
            println!("  Pass (success case)");
        } else if test_name == "dEQP-GLES2.info.version" {
            qpa_writer
                .write(include_bytes!("test_data/deqp-gles2-version.xml"))
                .context("writing QPA XML")?;
            println!("  Pass (success case)");
        } else if test_name == "dEQP-GLES2.info.extensions" {
            qpa_writer
                .write(include_bytes!("test_data/deqp-gles2-extensions.xml"))
                .context("writing QPA XML")?;
            println!("  Pass (success case)");
        } else if test_name == "dEQP-VK.info.device" {
            qpa_writer
                .write(include_bytes!("test_data/deqp-vk-info-device.xml"))
                .context("writing QPA XML")?;
            println!("  Pass (success case)");
        } else {
            unimplemented!("unknown mock test name {}", test_name)
        }

        writeln!(qpa_writer, "\n#endTestCaseResult").context("writing QPA test end")?;
    }

    writeln!(
        qpa_writer,
        r#"
#endTestsCasesTime

#endSession
"#
    )
    .context("writing QPA footer")?;

    Ok(())
}
