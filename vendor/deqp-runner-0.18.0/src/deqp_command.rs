//! Module for invoking [VK-GL-CTS](https://github.com/KhronosGroup/) (aka
//! dEQP) tests.

use crate::parse::ResultParser;
use crate::parse_deqp::DeqpResultParser;
use crate::runner_results::*;
pub use crate::test_status::{CaselistResult, TestResult, TestStatus};
use crate::timeout::{TimeoutChildStdout, Timer};
use crate::{runner_thread_index, TestCase, TestCommand, TestConfiguration};
use anyhow::{Context, Result};
use log::*;
use regex::Regex;
use std::collections::HashSet;
use std::fs::File;
use std::io::prelude::*;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;

pub struct DeqpCommand {
    /// Path to the dEQP binary (deqp-gles2, deqp-vk, glcts).
    pub deqp: PathBuf,
    /// Directory for the per-thread dEQP shader cache to be stored in.
    pub shader_cache_dir: PathBuf,
    /// Command line arguments to the dEQP binary.
    pub args: Vec<String>,
    pub config: TestConfiguration,
    /// Path to the built dEQP `executor/testlog-to-xml` binary, for turning the
    /// produced .qpa files into something you can point a web browser at.
    pub qpa_to_xml: Option<PathBuf>,
    /// Optional prefix to add to the test names, to distinguish between
    /// testsuite variations (like enabled debug knobs) in a `deqp-runner suite`
    /// invocation.
    pub prefix: String,
}

fn write_caselist_file(filename: &Path, tests: &[&TestCase]) -> Result<()> {
    let file = File::create(filename)
        .with_context(|| format!("creating temp caselist file {}", filename.display()))?;
    let mut file = BufWriter::new(file);

    for test in tests.iter() {
        file.write(test.name().as_bytes())
            .context("writing temp caselist")?;
        file.write(b"\n").context("writing temp caselist")?;
    }
    Ok(())
}

fn add_filename_arg(args: &mut Vec<String>, arg: &str, path: &Path) -> Result<()> {
    args.push(format!(
        "{}={}",
        arg,
        path.to_str()
            .with_context(|| format!("filename to utf8 for {}", path.display()))?
    ));
    Ok(())
}

// Extracts the XML string within a given testcase's section in the QPA.
fn qpa_xml_for_testcase<'a>(qpa: &'a str, test: &str) -> Result<&'a str> {
    let start = format!("#beginTestCaseResult {}\n", test);

    let xml_after_start = qpa
        .split(&start)
        .nth(1)
        .with_context(|| format!("Finding QPA test start delimiter for {}", test))?;
    let xml_until_end = xml_after_start
        .split("#endTestCaseResult")
        .next()
        .context("Finding QPA test end delimiter")?;

    Ok(xml_until_end)
}

// Returns the text from inside the XML's <Text>...</Text> nodes.
fn qpa_xml_text(xml: &str) -> Result<String> {
    let doc = roxmltree::Document::parse(xml).context("Parsing QPA XML")?;

    Ok(doc
        .descendants()
        .filter(|n| n.has_tag_name("Text"))
        .filter_map(|node| node.text())
        .collect::<Vec<&str>>()
        .join("\n"))
}

/// deduplicing printer for renderer/version check information before starting the run.
pub fn log_info_deduped(header: &str, info: &str) {
    lazy_static! {
        static ref LOGGED: Mutex<HashSet<(String, String)>> = Mutex::new(HashSet::new());
    }

    if let Ok(mut logged) = LOGGED.lock() {
        if logged.insert((header.to_string(), info.to_string())) {
            println!("{}: {}", header, info);
        }
    }
}

/// Tries to find dEQP version output in the stdout contents passed in, and
/// prints it if so.  Deduplicates repeated dEQP versions.
pub fn log_deqp_version(output: &str) -> Option<String> {
    lazy_static! {
        static ref VERSIONS_SEEN: Mutex<HashSet<String>> = Mutex::new(HashSet::new());
        static ref DEQP_VERSION_REGEX: Regex = Regex::new(r"dEQP Core (.*) starting")
            .context("Compiling deqp version RE")
            .unwrap();
    }

    let deqp_version = DEQP_VERSION_REGEX.captures(output);
    if let Some(version) = deqp_version {
        let version = &version[1];
        log_info_deduped("dEQP version", version);
    }

    None
}

impl DeqpCommand {
    fn caselist_path(&self, caselist_state: &CaselistState) -> Result<PathBuf> {
        self.caselist_file_path(caselist_state, "caselist.txt")
            .context("caselist path")
    }

    fn qpa_path(&self, caselist_state: &CaselistState) -> Result<PathBuf> {
        self.caselist_file_path(caselist_state, "qpa")
            .context("caselist path")
    }

    fn cache_path(&self) -> Result<PathBuf> {
        Ok(self
            .shader_cache_dir
            .canonicalize()
            .context("cache path")?
            .join(format!("t{}.shader_cache", runner_thread_index()?)))
    }

    fn try_extract_qpa<S: AsRef<str>, P: AsRef<Path>>(&self, test: S, qpa_path: P) -> Result<()> {
        let qpa_path = qpa_path.as_ref();
        let test = test.as_ref();
        let output = filter_qpa(
            File::open(qpa_path).with_context(|| format!("Opening {}", qpa_path.display()))?,
            test,
        )?;

        if !output.is_empty() {
            let out_path = qpa_path.parent().unwrap().join(format!("{}.qpa", test));
            // Write the extracted QPA contents to an individual file.
            {
                let mut out_qpa = BufWriter::new(File::create(&out_path).with_context(|| {
                    format!("Opening output QPA file {:?}", qpa_path.display())
                })?);
                out_qpa.write_all(output.as_bytes())?;
            }

            // Now that the QPA file is written (and flushed, note the separate
            // block!), call out to testlog-to-xml to convert it to an XML file
            // for display.
            if let Some(qpa_to_xml) = self.qpa_to_xml() {
                let xml_path = out_path.with_extension("xml");
                let convert_output = Command::new(qpa_to_xml)
                    .current_dir(self.deqp.parent().unwrap_or_else(|| Path::new("/")))
                    .arg(&out_path)
                    .arg(xml_path)
                    .output()
                    .with_context(|| format!("Failed to spawn {}", qpa_to_xml.display()))?;
                if !convert_output.status.success() {
                    anyhow::bail!(
                        "Failed to run {}: {}",
                        qpa_to_xml.display(),
                        String::from_utf8_lossy(&convert_output.stderr)
                    );
                } else {
                    std::fs::remove_file(&out_path).context("removing converted QPA")?;
                }
            }
        }

        Ok(())
    }

    // Runs dEQP for a testcase and collects the QPA file to a string.
    pub fn deqp_test_qpa_output(&self, testcase: &str, filename: &str) -> Result<String> {
        let qpa_path = self
            .config
            .output_dir
            .canonicalize()
            .context("qpa check canonicalize")?
            .join(filename);

        let mut args: Vec<String> = Vec::new();

        // Add on the user's specified deqp arguments.
        for arg in &self.args {
            args.push(arg.clone());
        }

        args.push(format!("--deqp-case={}", &testcase));
        add_filename_arg(&mut args, "--deqp-log-filename", &qpa_path)
            .context("adding log to args")?;

        let output = Command::new(&self.deqp)
            .current_dir(self.deqp.parent().unwrap_or_else(|| Path::new("/")))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null())
            .args(args)
            // Disable MESA_DEBUG output by default for debug Mesa builds, which
            // otherwise fills the logs with warnings about GL errors that are
            // thrown (you're running deqp!  Of course it makes GL errors!)
            .env("MESA_DEBUG", "silent")
            .envs(self.config.env.iter())
            .output()
            .with_context(|| format!("Failed to spawn {}", &self.deqp.display()))?;

        if !output.status.success() {
            anyhow::bail!(
                "Failed to invoke dEQP for {}:\nstdout:\n{}\nstderr:\n{}",
                testcase,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }

        log_deqp_version(&String::from_utf8_lossy(&output.stdout));

        let mut qpa = String::new();
        File::open(qpa_path)
            .context("opening QPA")?
            .read_to_string(&mut qpa)
            .context("reading QPA")?;

        Ok(qpa)
    }

    pub fn qpa_vk_device_name_check(&self, regex: &str) -> Result<bool> {
        let testcase = "dEQP-VK.info.device";
        let qpa = self.deqp_test_qpa_output(testcase, testcase)?;
        let xml = qpa_xml_for_testcase(&qpa, testcase)?;

        for line in qpa_xml_text(xml)?.lines() {
            if line.starts_with("deviceName: ") {
                println!("{}", line);

                if !regex.is_empty() {
                    let regex = Regex::new(regex)
                        .with_context(|| format!("Compiling QPA text check RE '{}'", regex))?;
                    return Ok(regex.is_match(line));
                }
                return Ok(true);
            }
        }
        anyhow::bail!("Failed to find deviceName")
    }

    pub fn qpa_gl_renderer_version_check(
        &self,
        qpa: &str,
        testcase: &str,
        regex: &str,
        log_name: &str,
    ) -> Result<bool> {
        let xml = qpa_xml_for_testcase(qpa, testcase)?;
        let doc = roxmltree::Document::parse(xml)
            .with_context(|| format!("Parsing QPA XML for {}", testcase))?;

        for text in doc.descendants().filter(|n| n.has_tag_name("Text")) {
            if let Some(text) = text.text() {
                log_info_deduped(log_name, text);
                if regex.is_empty() {
                    return Ok(true);
                } else {
                    let regex = Regex::new(regex).with_context(|| {
                        format!("Compiling QPA renderer/version check RE '{}'", regex)
                    })?;
                    return Ok(regex.is_match(text));
                }
            }
        }
        anyhow::bail!("Failed to find {}", log_name)
    }

    pub fn qpa_extensions_check(
        &self,
        qpa: &str,
        testcase: &str,
        extensions_check: &str,
    ) -> Result<bool> {
        if extensions_check.is_empty() {
            return Ok(true);
        }

        let xml = qpa_xml_for_testcase(qpa, testcase)?;
        let probed_extensions = qpa_xml_text(xml)?
            .lines()
            .map(|x| x.trim().to_string())
            .collect::<HashSet<String>>();

        let expected_extensions = std::fs::read_to_string(extensions_check)
            .with_context(|| format!("Reading expected exts file {}", extensions_check))?
            .lines()
            .map(|x| x.trim().to_string())
            .filter(|x| !x.is_empty())
            .collect::<HashSet<String>>();

        if probed_extensions != expected_extensions {
            error!("Extensions mismatch:");

            for ext in probed_extensions.difference(&expected_extensions) {
                error!("Unexpected: {}", ext);
            }
            for ext in expected_extensions.difference(&probed_extensions) {
                error!("Missing: {}", ext);
            }
            return Ok(false);
        }

        Ok(true)
    }

    fn qpa_to_xml(&self) -> Option<&PathBuf> {
        self.qpa_to_xml.as_ref()
    }
}

impl TestCommand for DeqpCommand {
    fn name(&self) -> &str {
        "dEQP"
    }

    fn prepare(&self, caselist_state: &CaselistState, tests: &[&TestCase]) -> Result<Command> {
        let caselist_path = self.caselist_path(caselist_state)?;
        let qpa_path = self.qpa_path(caselist_state)?;
        let cache_path = self.cache_path()?;

        write_caselist_file(&caselist_path, tests).context("writing caselist file")?;

        let mut args: Vec<String> = Vec::new();

        // Add on the user's specified deqp arguments.
        for arg in &self.args {
            args.push(arg.clone());
        }

        add_filename_arg(&mut args, "--deqp-caselist-file", &caselist_path)
            .context("adding caselist to args")?;
        add_filename_arg(&mut args, "--deqp-log-filename", &qpa_path)
            .context("adding log to args")?;
        args.push("--deqp-log-flush=disable".to_string());

        // The shader cache is not multiprocess safe, use one per
        // caselist_state.  However, since we're spawning lots of separate dEQP
        // runs, disable truncation (which would otherwise mean we only
        // get caching within a single run_block(), which is pretty
        // small).
        add_filename_arg(&mut args, "--deqp-shadercache-filename", &cache_path)
            .context("adding cache to args")?;
        args.push("--deqp-shadercache-truncate=disable".to_string());

        debug!(
            "Begin caselist c{}.r{}",
            caselist_state.caselist_id, caselist_state.run_id
        );

        let mut command = Command::new(&self.deqp);
        command
            .current_dir(self.deqp.parent().unwrap_or_else(|| Path::new("/")))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null())
            .args(args)
            .env("DEQP_RUNNER_THREAD", runner_thread_index()?.to_string())
            // Disable MESA_DEBUG output by default for debug Mesa builds, which
            // otherwise fills the logs with warnings about GL errors that are
            // thrown (you're running deqp!  Of course it makes GL errors!)
            .env("MESA_DEBUG", "silent")
            .envs(self.config.env.iter());
        Ok(command)
    }

    fn clean(
        &self,
        caselist_state: &CaselistState,
        _tests: &[&TestCase],
        results: &[RunnerResult],
    ) -> Result<()> {
        let caselist_path = self.caselist_path(caselist_state)?;
        let qpa_path = self.qpa_path(caselist_state)?;

        // Something happens occasionally in runs (particularly with ASan) where
        // we get an -ENOENT from removing these files. We don't want to fail
        // the caselist for that if it has useful results.
        if !results.is_empty()
            && results
                .iter()
                .all(|x| !x.status.should_save_logs(self.config.save_xfail_logs))
        {
            if let Err(e) = std::fs::remove_file(&caselist_path)
                .with_context(|| format!("removing caselist at {:?}", &caselist_path))
            {
                error!("{:?}", e);
            }
        }
        if let Err(e) = std::fs::remove_file(&qpa_path)
            .with_context(|| format!("removing qpa at {:?}", &qpa_path))
        {
            error!("{:?}", e);
        };

        debug!(
            "End caselist c{}.r{}",
            caselist_state.caselist_id, caselist_state.run_id
        );

        Ok(())
    }

    fn parse_results(
        &self,
        _caselist_state: &CaselistState,
        _tests: &[&TestCase],
        stdout: TimeoutChildStdout,
        timer: Option<Timer>,
    ) -> Result<CaselistResult> {
        let parser = DeqpResultParser {};
        parser.parse_with_timer(stdout, timer)
    }

    fn handle_result(
        &self,
        caselist_state: &CaselistState,
        result: &TestResult,
        status: &RunnerStatus,
    ) -> Result<()> {
        if !status.is_success() {
            let qpa_path = self
                .caselist_file_path(caselist_state, "qpa")
                .context("qpa path")?;
            if let Err(e) =
                self.try_extract_qpa(result.name.trim_start_matches(&self.prefix), qpa_path)
            {
                warn!("Failed to extract QPA resuls: {}", e)
            }
        }

        Ok(())
    }

    fn see_more(&self, _name: &str, caselist_state: &CaselistState) -> String {
        // This is the same as run() did, so we should be safe to unwrap.
        let qpa_path = self.config.output_dir.join(
            format!(
                "c{}.r{}.log",
                caselist_state.caselist_id, caselist_state.run_id
            )
            .as_str(),
        );
        format!("See {:?}", qpa_path)
    }

    fn config(&self) -> &TestConfiguration {
        &self.config
    }

    fn prefix(&self) -> &str {
        &self.prefix
    }
}

fn filter_qpa<R: Read, S: AsRef<str>>(reader: R, test: S) -> Result<String> {
    let lines = BufReader::new(reader).lines();

    let start = format!("#beginTestCaseResult {}", test.as_ref());

    let mut found_case = false;
    let mut including = true;
    let mut output = String::new();
    for line in lines {
        let line = line.context("reading QPA")?;
        if line == start {
            found_case = true;
            including = true;
        }

        if including {
            output.push_str(&line);
            output.push('\n');
        }

        if line == "#beginSession" {
            including = false;
        }

        if including && line == "#endTestCaseResult" {
            break;
        }
    }

    if !found_case {
        anyhow::bail!("Failed to find {} in QPA", test.as_ref());
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn filter_qpa_success() {
        assert_eq!(
            include_str!("test_data/deqp-gles2-renderer.qpa"),
            filter_qpa(
                Cursor::new(include_str!("test_data/deqp-gles2-info.qpa")),
                "dEQP-GLES2.info.renderer"
            )
            .unwrap(),
        );
    }

    #[test]
    fn filter_qpa_no_results() {
        assert!(filter_qpa(
            Cursor::new(include_str!("test_data/deqp-empty.qpa")),
            "dEQP-GLES2.info.version"
        )
        .is_err());
    }

    #[test]
    fn filter_qpa_xml_success() -> Result<()> {
        assert_eq!(
            include_str!("test_data/deqp-gles2-renderer.xml"),
            qpa_xml_for_testcase(
                include_str!("test_data/deqp-gles2-info.qpa"),
                "dEQP-GLES2.info.renderer"
            )?
        );
        Ok(())
    }

    #[test]
    fn filter_qpa_xml_fail() -> Result<()> {
        assert!(qpa_xml_for_testcase(
            include_str!("test_data/deqp-gles2-info.qpa"),
            "dEQP-GLES2.info.notatest"
        )
        .is_err());
        Ok(())
    }
}
