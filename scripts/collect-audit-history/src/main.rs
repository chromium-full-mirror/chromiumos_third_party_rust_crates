// Copyright 2023 The ChromiumOS Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.
#![deny(unsafe_code)]

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

type UtcTimestamp = i64;

// The SHA where `audits.toml` first came into existence.
const CARGO_VET_LANDED_HASH: &str = "f814f16fc52b39df4730306fe2e9cdb865143d68";

/// A snapshot of the state of ChromeOS' third-party Rust audits. Generally sourced at each git
/// commit.
#[derive(Debug, Serialize)]
struct AuditSnapshot {
    timestamp_utc: UtcTimestamp,
    total_audits: u32,
    live_crates_fully_audited: u32,
    live_crates_partially_audited: u32,
    crates_with_exemptions: u32,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct CrateVer {
    crate_name: String,
    crate_ver: cargo_lock::Version,
}

fn find_audited_crates_in_audits_toml(audits_toml: &str) -> Result<Vec<CrateVer>> {
    #[derive(Deserialize)]
    struct AuditsToml {
        audits: HashMap<String, Vec<AuditEntry>>,
    }

    #[derive(Deserialize)]
    struct AuditEntry {
        #[serde(default)]
        version: Option<String>,
        #[serde(default)]
        delta: Option<String>,
    }

    let audits: AuditsToml = toml::from_str(audits_toml)?;
    let mut results = Vec::new();
    for (crate_name, crate_audits) in audits.audits {
        for audit in crate_audits {
            let version = match (&audit.version, &audit.delta) {
                (Some(ver), None) => ver,
                (None, Some(delta)) => {
                    let arrow = "->";
                    let last_arrow = delta
                        .rfind(arrow)
                        .ok_or_else(|| anyhow!("delta field has no {arrow:?}: {delta:?}"))?;
                    delta[last_arrow + arrow.len()..].trim_start()
                }
                (None, None) => {
                    bail!("unexpected: audit for {crate_name} has no `version` or `delta`")
                }
                (Some(_), Some(_)) => {
                    bail!("unexpected: audit for {crate_name} has both a `version` and `delta`")
                }
            };

            let crate_ver = cargo_lock::Version::parse(version)
                .with_context(|| format!("parsing {}", version))?;
            results.push(CrateVer {
                crate_name: crate_name.clone(),
                crate_ver,
            });
        }
    }
    // Sort so we have consistent output.
    results.sort_unstable();
    Ok(results)
}

fn find_exempted_crates_in_config_toml(config_toml: &str) -> Result<Vec<CrateVer>> {
    #[derive(Deserialize)]
    struct ConfigToml {
        #[serde(default)]
        exemptions: HashMap<String, Vec<Exemption>>,
    }

    #[derive(Deserialize)]
    struct Exemption {
        version: String,
    }

    let config: ConfigToml = toml::from_str(config_toml)?;
    let mut results = Vec::new();
    for (crate_name, exemptions) in config.exemptions {
        for exemption in exemptions {
            let crate_ver = cargo_lock::Version::parse(&exemption.version)
                .with_context(|| format!("parsing {}", exemption.version))?;
            results.push(CrateVer {
                crate_name: crate_name.clone(),
                crate_ver,
            });
        }
    }
    // Sort so we have consistent output.
    results.sort_unstable();
    Ok(results)
}

fn list_crates_in_cargo_lock(lockfile: &str) -> Result<Vec<CrateVer>> {
    let packages = lockfile.parse::<cargo_lock::Lockfile>()?.packages;
    let mut results = packages
        .into_iter()
        .map(|package| CrateVer {
            crate_name: package.name.to_string(),
            crate_ver: package.version,
        })
        .collect::<Vec<_>>();
    results.sort_unstable();
    Ok(results)
}

fn collect_one_audit_snapshot(rust_crates: &Path, sha: &str) -> Result<AuditSnapshot> {
    let git_show_file = |file_path_relative_to_rust_crates: &str| -> Result<String> {
        let file_contents_output = Command::new("git")
            .arg("show")
            .arg(format!("{sha}:{file_path_relative_to_rust_crates}"))
            .current_dir(rust_crates)
            .output()
            .with_context(|| format!("querying git for {}", file_path_relative_to_rust_crates))?;

        if !file_contents_output.status.success() {
            bail!(
                "`git show {}:{}` failed; stderr: {:?}",
                sha,
                file_path_relative_to_rust_crates,
                String::from_utf8_lossy(&file_contents_output.stderr)
            );
        }

        let stdout_str = String::from_utf8(file_contents_output.stdout).with_context(|| {
            format!(
                "decoding contents of {} at SHA {} as utf8",
                file_path_relative_to_rust_crates, sha
            )
        })?;
        Ok(stdout_str)
    };

    let git_commit_timestamp = || -> Result<UtcTimestamp> {
        let output = Command::new("git")
            .args(["log", "-n1", "--date=rfc2822", "--format=%cd", sha])
            .current_dir(rust_crates)
            .output()
            .with_context(|| format!("querying git for date of {}", sha))?;

        if !output.status.success() {
            bail!(
                "`git log -n1 --format=%cd {}` failed; stderr: {:?}",
                sha,
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let stdout_str = String::from_utf8(output.stdout).with_context(|| {
            format!(
                "decoding contents of the commit date at SHA {} as utf8",
                sha
            )
        })?;
        let date = stdout_str.trim();
        let result = chrono::DateTime::parse_from_rfc2822(date)
            .with_context(|| format!("parsing date {date}"))?;
        Ok(result.naive_utc().timestamp())
    };

    let ((audited_crates, exempted_crates), (crates_in_cargo_lock, commit_timestamp_utc)) =
        rayon::join(
            || {
                rayon::join(
                    || find_audited_crates_in_audits_toml(&git_show_file("cargo-vet/audits.toml")?),
                    || {
                        find_exempted_crates_in_config_toml(&git_show_file(
                            "cargo-vet/config.toml",
                        )?)
                    },
                )
            },
            || {
                rayon::join(
                    || list_crates_in_cargo_lock(&git_show_file("projects/Cargo.lock")?),
                    || git_commit_timestamp(),
                )
            },
        );

    let audited_crates = audited_crates?.into_iter().collect::<HashSet<_>>();
    let exempted_crates = exempted_crates?.into_iter().collect::<HashSet<_>>();
    let crates_in_cargo_lock = crates_in_cargo_lock?.into_iter().collect::<HashSet<_>>();
    let commit_timestamp_utc = commit_timestamp_utc?;
    let live_crates_partially_audited = audited_crates
        .intersection(&crates_in_cargo_lock)
        .filter(|x| exempted_crates.contains(x))
        .count() as u32;
    let live_crates_fully_audited = audited_crates
        .intersection(&crates_in_cargo_lock)
        .filter(|x| !exempted_crates.contains(x))
        .count() as u32;
    Ok(AuditSnapshot {
        timestamp_utc: commit_timestamp_utc,
        total_audits: audited_crates.len() as u32,
        live_crates_fully_audited,
        live_crates_partially_audited,
        crates_with_exemptions: exempted_crates.len() as u32,
    })
}

/// Returns a Vec of audit snapshots, ordered from the beginning of our auditing (earlier) -> now
/// (later).
fn collect_audit_snapshots(
    rust_crates: &Path,
    upstream_branch: &str,
) -> Result<Vec<AuditSnapshot>> {
    let shas_to_inspect_output = Command::new("git")
        .args(["log", "--format=%H"])
        .arg(format!("{}..{}", CARGO_VET_LANDED_HASH, upstream_branch))
        .args([
            "--",
            "cargo-vet/audits.toml",
            "cargo-vet/config.toml",
            "projects/Cargo.lock",
        ])
        .current_dir(rust_crates)
        .output()
        .context("querying git for SHAs that impacted audits")?;

    if !shas_to_inspect_output.status.success() {
        bail!(
            "`git log` failed; stderr: {:?}",
            String::from_utf8_lossy(&shas_to_inspect_output.stderr)
        );
    }

    let raw_hash_list = std::str::from_utf8(&shas_to_inspect_output.stdout)
        .context("parsing git's output for hashes")?;

    // Note that git prints the hash list in reverse, but we want to emit `AuditSnapshot`s in order
    // of increasing `time`.
    let hash_list = raw_hash_list
        .lines()
        .rev()
        .map(str::trim)
        .collect::<Vec<&str>>();

    let mut results = hash_list
        .into_par_iter()
        .map(|sha| collect_one_audit_snapshot(rust_crates, sha))
        .collect::<Result<Vec<_>>>()?;

    // If nothing changed from one commit to the next, ignore the next commit.
    results.dedup_by_key(|x| {
        (
            x.total_audits,
            x.live_crates_fully_audited,
            x.live_crates_partially_audited,
            x.crates_with_exemptions,
        )
    });

    Ok(results)
}

/// This program scrapes the git history of `rust_crates` to create a CSV which describes our
/// cargo-vet progress over time.
///
/// Please note that these numbers may disagree with cargo-vet's somewhat: it's hard to _go back in
/// history_ since rust_crates has used different versions of cargo-vet in the past, so this has
/// its own simple audit-counting logic. This diverges somewhat from cargo-vet's logic for showing
/// partially-audited crates. In particular, this script counts every crate that has an audit _and_
/// that has a live exemption as "partially-audited". cargo-vet does not.
#[derive(Parser)]
struct Args {
    /// Path to rust_crates. Optional.
    #[clap(long)]
    rust_crates: Option<PathBuf>,

    /// File to write CSV output to. Writes to stdout if not specified.
    #[clap(long)]
    output_file: Option<PathBuf>,

    /// The name of the upstream branch to read the history of.
    #[clap(long, default_value = "cros/main")]
    upstream_branch: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let rust_crates = match args.rust_crates {
        Some(x) => x,
        None => {
            let mut exe = std::env::current_exe()?;
            // current_exe will point to:
            // rust_crates/scripts/collect-audit-history/target/${profile}/collect-audit-history
            for _ in 0..5 {
                assert!(exe.pop());
            }
            println!("Using {} for rust_crates", exe.display());
            exe
        }
    };

    let audit_snapshots = collect_audit_snapshots(&rust_crates, &args.upstream_branch)?;

    let mut csv_builder = csv::WriterBuilder::new();
    csv_builder.has_headers(true);
    match args.output_file {
        None => {
            let stdout = std::io::stdout();
            let mut w = csv_builder.from_writer(stdout.lock());
            for s in audit_snapshots {
                w.serialize(&s)?;
            }
        }
        Some(output_file) => {
            let mut w = csv_builder.from_path(output_file)?;
            for s in audit_snapshots {
                w.serialize(&s)?;
            }
            std::mem::drop(w);
            println!("CSV file was successfully written");
        }
    }

    Ok(())
}
