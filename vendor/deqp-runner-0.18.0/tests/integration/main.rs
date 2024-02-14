use ::deqp_runner::RunnerResults;
use anyhow::{Context, Result};
use std::io::prelude::*;
use std::path::{Path, PathBuf};

/// Integration test binary.  See https://matklad.github.io/2021/02/27/delete-cargo-integration-tests.html
mod deqp_runner;
mod gtest_runner;
mod piglit_runner;

// All the output we capture from an invocation of deqp-runner
struct RunnerCommandResult {
    status: std::process::ExitStatus,
    stdout: String,
    stderr: String,
    output_dir_contents: Vec<String>,

    results: Result<RunnerResults>,
}

pub fn tempfile<S: AsRef<str>>(data: S) -> Result<tempfile::TempPath> {
    let mut file = tempfile::NamedTempFile::new().context("creating tempfile")?;
    file.write(data.as_ref().as_bytes())
        .context("writing tempfile")?;
    Ok(file.into_temp_path())
}

pub fn lines_tempfile<S: AsRef<str>, I: IntoIterator<Item = S>>(
    lines: I,
) -> Result<tempfile::TempPath> {
    let mut file = tempfile::NamedTempFile::new().context("creating tempfile")?;
    for line in lines {
        writeln!(file, "{}", line.as_ref()).context("writing tempfile")?;
    }
    Ok(file.into_temp_path())
}

pub fn dir_contents(dir: &Path) -> Vec<String> {
    if let Ok(contents) = dir.read_dir() {
        contents
            .filter_map(|x| x.ok())
            .map(|x| x.file_name().to_string_lossy().as_ref().to_string())
            .collect()
    } else {
        Vec::new()
    }
}

// Returns the path to a test resource checked into git.
pub fn test_resource_path(filename: &str) -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.join("resources/test").join(filename)
}
