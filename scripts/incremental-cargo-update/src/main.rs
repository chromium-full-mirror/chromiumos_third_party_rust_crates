// Copyright 2023 The ChromiumOS Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.
#![deny(unsafe_code)]

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;
use log::{debug, error, info};

/// Convenience trait to replace this repetitive pattern:
/// ```ignore
/// let foo = Command::new(bar).status()?;
/// if !foo.success() {
///   bail!("some message");
/// }
/// // never use `foo` afterward
/// ```
/// with this:
/// ```ignore
/// Command::new(bar).status().status_to_result(|| "some message")?;
/// ```
trait StatusToResult: Sized {
    type Output;
    fn status_to_result<F: FnOnce() -> S, S: std::fmt::Display>(self, f: F)
        -> Result<Self::Output>;
}

fn exit_status_code_str(s: &std::process::ExitStatus) -> String {
    match s.code() {
        None => "no code".into(),
        Some(x) => format!("code {}", x),
    }
}

impl StatusToResult for std::process::ExitStatus {
    type Output = std::process::ExitStatus;
    fn status_to_result<F: FnOnce() -> S, S: std::fmt::Display>(
        self,
        f: F,
    ) -> Result<Self::Output> {
        if !self.success() {
            bail!("{} exited with {}", f(), exit_status_code_str(&self));
        }
        Ok(self)
    }
}

impl StatusToResult for std::process::Output {
    type Output = std::process::Output;
    fn status_to_result<F: FnOnce() -> S, S: std::fmt::Display>(
        self,
        f: F,
    ) -> Result<Self::Output> {
        if !self.status.success() {
            bail!(
                "{} exited with {}; stderr: {}",
                f(),
                exit_status_code_str(&self.status),
                String::from_utf8_lossy(&self.stderr)
            );
        }
        Ok(self)
    }
}

impl<T: StatusToResult> StatusToResult for std::io::Result<T> {
    type Output = T::Output;
    fn status_to_result<F: FnOnce() -> S, S: std::fmt::Display>(
        self,
        f: F,
    ) -> Result<Self::Output> {
        match self {
            Err(x) => bail!("{} failed: could not exec: {}", f(), x),
            Ok(x) => x.status_to_result(f),
        }
    }
}

/// Ensures there are no unstaged changes in the git repo rooted at `repo`. Returns an error if
/// there are, or if checking failed.
fn ensure_repo_is_clean(repo: &Path) -> Result<()> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo)
        .output()
        .status_to_result(|| format!("`git status` in {}", repo.display()))?;

    let has_changes = output.stdout.iter().any(|x| !x.is_ascii_whitespace());
    if has_changes {
        bail!(
            "uncommitted changes found in git repo at {}",
            repo.display()
        );
    }
    Ok(())
}

/// Given a directory, walks up until a `.git` directory entry can be found.
fn find_git_repo_root(dir: &Path) -> Option<PathBuf> {
    let mut buf = dir.to_path_buf();
    loop {
        buf.push(".git");
        let exists = buf.exists();
        buf.pop();
        if exists {
            return Some(buf);
        }
        if !buf.pop() {
            return None;
        }
    }
}

/// Information that can uniquely identify a package, for cargo-update's sake.
#[derive(Debug, PartialEq, Eq, Hash)]
struct PackageInfo {
    name: String,
    version: String,
}

impl std::fmt::Display for PackageInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        // This is the format required by cargo-update to uniquely identify a package.
        write!(f, "{}@{}", self.name, self.version)
    }
}

fn parse_cargo_lock_packages(path: &Path) -> Result<Vec<PackageInfo>> {
    let lockfile = cargo_lock::Lockfile::load(path)
        .with_context(|| format!("loading lockfile at {}", path.display()))?;
    let mut lockfile_packages = lockfile.packages;
    lockfile_packages.sort_unstable_by(|p1, p2| {
        p1.name
            .cmp(&p2.name)
            .then_with(|| p1.version.cmp(&p2.version))
    });

    Ok(lockfile_packages
        .into_iter()
        .filter_map(|package| {
            if package.source?.is_default_registry() {
                Some(PackageInfo {
                    name: package.name.to_string(),
                    version: package.version.to_string(),
                })
            } else {
                None
            }
        })
        .collect())
}

enum UpdateType {
    Offline,
    Online,
}

/// Dictates the package(s) to run `cargo update` on.
enum PackageSpec<'a> {
    /// Try to update all packages.
    All,
    /// Only try to update the given package. This may require transitive updates, but `cargo
    /// update` will not be asked to `--aggressive`ly update these, since aggressive updates can
    /// lead to us exceeding our soft update limit by more than necessary.
    Only(&'a PackageInfo),
}

/// Runs `cargo update` on the given lockfile.
fn cargo_update_packages(lock_file: &Path, package: PackageSpec<'_>, ty: UpdateType) -> Result<()> {
    let mut cmd = Command::new("cargo");
    cmd.arg("update");

    let parent = lock_file
        .parent()
        .ok_or_else(|| anyhow!("lockfiles shouldn't be parentless"))?;
    cmd.current_dir(parent);

    if let PackageSpec::Only(package) = package {
        cmd.arg("--package");
        cmd.arg(package.to_string());
    }

    if matches!(ty, UpdateType::Offline) {
        cmd.arg("--offline");
    }

    cmd.status()
        .status_to_result(|| format!("running cargo-update for {}", lock_file.display()))?;
    Ok(())
}

/// Returns a list of packages from `initial_listing` which no longer exist in `new_listing`.
fn find_updated_packages<'a>(
    initial_listing: &'a [PackageInfo],
    new_listing: &[PackageInfo],
) -> Vec<&'a PackageInfo> {
    let new_listing: HashSet<_> = new_listing.iter().collect();
    initial_listing
        .iter()
        .filter(|x| !new_listing.contains(x))
        .collect()
}

/// Runs `git checkout ${file}`.
fn revert_file_to_head(file: &Path) -> Result<()> {
    let mut cmd = Command::new("git");
    cmd.args(["checkout", "--"]);

    let file_name = file
        .file_name()
        .ok_or_else(|| anyhow!("{} has no file name", file.display()))?;
    cmd.arg(file_name);

    // Always `cd` into the parent directory, since $PWD might be outside of the git dir.
    let parent = file
        .parent()
        .ok_or_else(|| anyhow!("lockfiles shouldn't be parentless"))?;
    cmd.current_dir(parent);

    cmd.status()
        .status_to_result(|| format!("git-checkout {}", file.display()))?;
    Ok(())
}

/// Updates `cargo_lock` with a soft limit of `max_updates` updates. Update is done destructively,
/// and it's assumed that the git repo this is being run in is clean of updates.
///
/// Returns the number of packages that were ultimately updated.
fn perform_cargo_update(cargo_lock: &Path, max_updates: usize) -> Result<usize> {
    let initial_cargo_lock = parse_cargo_lock_packages(cargo_lock)?;
    debug!("Parsed {} Cargo.lock packages.", initial_cargo_lock.len());

    info!("Running initial `cargo update`...");
    cargo_update_packages(cargo_lock, PackageSpec::All, UpdateType::Online)?;

    let fully_updated_cargo_lock = parse_cargo_lock_packages(cargo_lock)?;
    let fully_updated_packages =
        find_updated_packages(&initial_cargo_lock, &fully_updated_cargo_lock);
    info!(
        "Found a total of {} possible package update(s).",
        fully_updated_packages.len()
    );
    if fully_updated_packages.len() <= max_updates {
        return Ok(fully_updated_packages.len());
    }

    // We have more updates than we'd like. A solution that makes these updates happen should:
    // - Be deterministic (if we assume crates.io doesn't change, which is fine for our purposes
    //   here).
    // - Not update unnecessarily many dependencies (if we want to update 15 things, and our second
    //   update requires 40 other updates, ...).
    // - Guarantee forward progress (if an update of foo-0.1 to foo-0.2 is only unblocked by
    //   upgrading bar, we should notice that foo wasn't properly upgraded & continue on).
    //
    // Since the speed of this program... does not really matter, just take the simple
    // one-at-a-time approach.
    revert_file_to_head(cargo_lock)?;

    let mut current_cargo_lock = parse_cargo_lock_packages(cargo_lock)?;
    for package in &fully_updated_packages {
        // Crates may disappear if an update of a prior crate required an update of `package`.
        // `cargo-update` will fail if `package` is not in `Cargo.lock`.
        if !current_cargo_lock.contains(package) {
            info!("Skipping update of {package}; it's no longer present.");
            continue;
        }

        info!("Updating {package} on its own...");
        // Use offline updates, since we should've updated crates.io with the online update above.
        cargo_update_packages(cargo_lock, PackageSpec::Only(package), UpdateType::Offline)?;
        current_cargo_lock = parse_cargo_lock_packages(cargo_lock)?;
        let newly_updated = find_updated_packages(&initial_cargo_lock, &current_cargo_lock);
        // `> max_updates` is OK, since `max_updates` is a soft limit: one update may require
        // others.
        if newly_updated.len() >= max_updates {
            debug!("Update limit hit; stopping update attempts");
            return Ok(newly_updated.len());
        }
        debug!(
            "{} package{} updated so far; trying again.",
            newly_updated.len(),
            if newly_updated.len() == 1 { "" } else { "s" },
        );
    }
    unreachable!("somehow `cargo update` updated more things than updating one-by-one?");
}

/// Convenient struct that cleans up a worktree on drop.
struct Worktree<'a> {
    base_path: &'a Path,
    location: PathBuf,
}

impl<'a> Worktree<'a> {
    fn new(git_root: &'a Path, checkout_ref: &str) -> Result<Worktree<'a>> {
        let temp_dir = tempdir::TempDir::new("cargo-update-worktree")?.into_path();
        Command::new("git")
            .args(["worktree", "add", "--force"])
            .arg(&temp_dir)
            .arg(checkout_ref)
            .stdin(Stdio::null())
            .current_dir(git_root)
            .status()
            .status_to_result(|| format!("creating worktree in {}", git_root.display()))?;
        Ok(Worktree {
            base_path: git_root,
            location: temp_dir,
        })
    }
}

impl<'a> Drop for Worktree<'a> {
    fn drop(&mut self) {
        let removal_result = Command::new("git")
            .args(["worktree", "remove", "--force"])
            .arg(&self.location)
            .current_dir(self.base_path)
            .output();
        match removal_result {
            Err(x) => error!(
                "Failed removing worktree at {}: {}",
                self.location.display(),
                x
            ),
            Ok(output) if !output.status.success() => error!(
                "Failed removing worktree at {}; stderr: {}",
                self.location.display(),
                String::from_utf8_lossy(&output.stderr)
            ),
            Ok(_) => (),
        }
    }
}

fn commit_all_changes(git_root: &Path, commit_message: &str) -> Result<()> {
    Command::new("git")
        .args(["commit", "-a", "-m", commit_message])
        .stdin(Stdio::null())
        .current_dir(git_root)
        .status()
        .status_to_result(|| format!("committing changes in {}", git_root.display()))?;
    Ok(())
}

#[derive(Parser)]
struct Args {
    /// Maximum number of packages to allow an update of. Note that this is a soft limit: sometimes
    /// upgrading one package will cause upgrades of many others.
    #[clap(long, default_value = "15")]
    max_updates: usize,

    /// Enable debug logging.
    #[clap(long)]
    debug: bool,

    /// The Cargo.lock file to update.
    #[clap(long)]
    cargo_lock: PathBuf,

    /// Rather than changing the git repo in-place, create a worktree checked out to `--branch`,
    /// and commit the changes on top of that. No commit will take place if no updates are
    /// possible.
    #[clap(long, requires = "commit_message")]
    branch: Option<String>,

    /// The commit message to use if `--branch` is specified.
    #[clap(long, requires = "branch")]
    commit_message: Option<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    simple_logger::init_with_level(if args.debug {
        log::Level::Debug
    } else {
        log::Level::Info
    })?;

    let non_worktree_git_root = find_git_repo_root(&args.cargo_lock).ok_or_else(|| {
        anyhow!(
            "{} doesn't appear to be in a git directory",
            args.cargo_lock.display()
        )
    })?;
    debug!("Detected repo at {}.", non_worktree_git_root.display());

    match (args.branch, args.commit_message) {
        (Some(branch), Some(commit_message)) => {
            debug!("Creating worktree...");
            let worktree = Worktree::new(&non_worktree_git_root, &branch)?;
            let worktree = &worktree.location;
            info!("Created a worktree at {}", worktree.display());
            let Ok(cargo_lock) = args.cargo_lock.strip_prefix(&non_worktree_git_root) else {
                bail!(
                    "{} doesn't start with git repo location at {}",
                    args.cargo_lock.display(),
                    worktree.display()
                );
            };

            let num_updates = perform_cargo_update(&worktree.join(cargo_lock), args.max_updates)?;
            info!("{num_updates} packages updated successfully.");
            if num_updates != 0 {
                commit_all_changes(worktree, &commit_message)?;
            }
            Ok(())
        }
        (None, None) => {
            ensure_repo_is_clean(&non_worktree_git_root)?;
            let num_updates = perform_cargo_update(&args.cargo_lock, args.max_updates)?;
            info!("{num_updates} packages updated successfully.");
            Ok(())
        }
        // Should be impossible due to clap's `requires` constraints.
        (Some(_), None) | (None, Some(_)) => unreachable!(),
    }
}
