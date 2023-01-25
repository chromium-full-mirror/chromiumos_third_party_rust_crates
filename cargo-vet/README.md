# cargo-vet

This directory contains files that [cargo-vet] needs in order to run.

(Just want to see how to placate the tool so `./vendor.py` will terminate
successfully? See [how do I audit](#how-do-i-audit) below)

[cargo-vet]: https://mozilla.github.io/cargo-vet/

## What does this do?

Within ChromeOS, we're trying to ensure that all third-party Rust code has been
audited by someone. `cargo-vet` is a tool that helps track audits, and gives a
nice interface with which we can interact with our audit backlog.

## Where do we source audits from?

Much of our existing audits were migrated from a project called CRAB, which
served a purpose similar to `cargo-vet`, but received significantly less SWE
time, and is not developed in the open.

Because of this historical detail, many audits in our repository are from e.g.,
"ChromeOS" or "Android Legacy" or similar. Going forward, we are trying to move
to more direct attribution.

As for remote audits, security requires that Googlers review crates that are
imported. Due to this, we cannot automatically use third-party sources of
audits, but we try to be good citizens if third-parties would like to use our
audits as a source. :)

## How do I audit?

### Symptoms that auditing is needed

If running `cargo-vet.py` complains, audits are needed:

```
Vetting Failed!

2 unvetted dependencies:
  const-sha1:0.2.0 missing ["safe-to-run", "crypto-safe"]
  rustyline-derive:0.4.0 missing ["safe-to-run", "crypto-safe"]

recommended audits for safe-to-run, crypto-safe:
    cargo vet diff rustyline-derive 0.6.0 0.4.0  (used by vendor_libs)
    cargo vet inspect const-sha1 0.2.0           (used by windows)

estimated audit backlog: 849 lines

Use |cargo vet certify| to record the audits.

cargo-vet audit failed. Please audit new packages. See cargo-vet/README.md if
you need help.
```

In this case, `const-sha1` version `0.2.0` needs audits to satisfy the criteria
"safe-to-run" and "crypto-safe", and `rustyline-derive` version `0.4.0` needs
the same. You'll have to audit these crates for these criteria as a result.

### Performing audits

Running `./scripts/cargo-vet.py` should print example commands for performing
audits. Using the examples in the previous section, only an audit of the crate's
diff is necessary (`./scripts/cargo-vet diff rustyline-derive 0.6.0 0.4.0`),
since we have an audit available for another version of rustyline-derive. On the
other hand, const-sha1 has no prior audits, so one must audit the entire thing
(`./scripts/cargo-vet inspect const-sha1 0.2.0`).

When you run `./scripts/cargo-vet.py inspect const-sha1 0.2.0`, you get a lot of
text describing the criteria you're being asked to audit for. If you would like
to consult descriptions of the criteria, the built-in criteria (safe-to-run,
safe-to-deploy) are located [located on an upstream webpage]. Our custom
criteria (rule-of-two-safe-to-deploy, crypto-safe, does-not-implement-crypto)
have descriptions listed in `audits.toml` in this directory.

`cargo-vet` has [more information on auditing here].

[located on an upstream webpage]: https://mozilla.github.io/cargo-vet/built-in-criteria.html
[more information on auditing here]: https://mozilla.github.io/cargo-vet/performing-audits.html#performing-audits

#### Guidance for auditing for `safe-to-run`

For those familiar, `cargo-vet`'s `safe-to-run` criteria is identical in spirit
to `code_is_not_malicious` from CRAB: no in-depth review of the code's logic is
expected to be able to certify this. What is expected is a cursory glance over
the code to ensure that nothing contrary to the crate's intent is obviously
happening (e.g., a crate meant to make error handling more convenient shouldn't
be connecting to the internet, or scraping the user's homedir).

The back-of-the-napkin estimate is that folks performing audits should be able
to certify a 1,000 line crate for this in a few minutes. If the crate is
extensively documented, has many tests, or if you're a seasoned auditor, you can
likely do better

## Forward intent

For folks wondering what the plans are with `cargo-vet`, the general consensus
seems to be that we should first get most crates reviewed as `safe-to-run` and
`crypto-safe`, since this catches most egregious classes of errors. As we reach
that goal, raising the bar to include `rule-of-two-safe-to-deploy` audits seems
appropriate for packages that end up on ChromeOS devices. However,
`rule-of-two-safe-to-deploy` audit velocity is meaningfully less than
`safe-to-run`, so low-hanging fruit and all.

If a reviewer would like to provide reviews for `rule-of-two-safe-to-deploy`,
that's highly appreciated! But it is also not required at this time.

### Isn't this all quite burdensome?

Initially auditing hundreds of crates will certainly take a while. `cargo-vet`
has powerful diffing tools to make incremental updates/upgrades proportional in
effort to a crate's diff. In other words, the hope is that the startup cost is
materially less than steady-state.

Less directly, our audits are made available to the FOSS community, and other
teams in Google may benefit from them in the future. (Conversely, we hope to
benefit from other teams' audits in the future, too. :) )
