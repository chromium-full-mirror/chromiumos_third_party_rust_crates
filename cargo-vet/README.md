# cargo-vet

This directory contains files that [cargo-vet] needs in order to run.

(Just want to see how to placate the tool so `./vendor.py` will terminate
successfully? See [how do I audit](#how-do-i-audit) below)

[cargo-vet]: https://mozilla.github.io/cargo-vet/

[TOC]

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

**NOTE**: audits are intended to be relatively lightweight, but some CLs might
pull in a ton of code. If your estimated audit backlog is 10,000 lines or more,
please see the [getting audit help](#getting-audit-help) section.

### Performing audits

There are two aspects to auditing: the content of the audit (e.g., "What am I
meant to look for?") and the mechanics of the audit (e.g., "How do I find the
code I'm meant to look at? how do I record what I've found?")

#### Mechanics

Running `./scripts/cargo-vet.py` should print example commands for performing
audits. Using the examples in the previous section, only an audit of the crate's
diff is necessary (`./scripts/cargo-vet diff rustyline-derive 0.6.0 0.4.0`),
since we have an audit available for another version of rustyline-derive. On the
other hand, const-sha1 has no prior audits, so one must audit the entire thing
(`./scripts/cargo-vet inspect const-sha1 0.2.0`).

When you run `./scripts/cargo-vet.py inspect const-sha1 0.2.0`, you'll get a lot
of text describing the criteria you're being asked to audit for. For your
convenience, the [audit content](#audit-content) section tries to reproduce this
all concisely.

If you would like to consult descriptions of the criteria, the built-in criteria
(`safe-to-run`, `safe-to-deploy`) are located [located on an upstream webpage].
Our custom criteria (`crypto-safe`, `does-not-implement-crypto`, `ub-risk-*`)
have individual descriptions listed in `audits.toml` in this directory, and have
much more detail [available upstream].

`cargo-vet` has [more information on auditing here].

[available upstream]: https://github.com/google/rust-crate-audits/blob/main/auditing_standards.md
[located on an upstream webpage]: https://mozilla.github.io/cargo-vet/built-in-criteria.html
[more information on auditing here]: https://mozilla.github.io/cargo-vet/performing-audits.html#performing-audits

#### Audit content

Note: this section is meant to summarize what folks are auditing for. As
mentioned in [mechanics](#mechanics), specifics on audit criteria are best left
to their full descriptions.

Audits that developers are being asked to perform essentially boil down to
answering two questions:

- Is there actively malicious code here (e.g., is it a terminal color crate on
  the tin which spawns a thread to mine ${cryptocurrency} for the author)? (this
  is `cargo-vet`'s `safe-to-run` criteria)
- Is the crate obviously attempting to implement cryptographic algorithms? (this
  is `cargo-vet`'s `does-not-implement-crypto` criteria)

For emphasis, we're looking for _locally obvious instances_ of the above. No
deep understanding of `unsafe` is required. No in-depth logic review is
required. In general, we expect someone who is mildly familiar with Rust to be
able to skim a 1,000 line crate for this criteria in under five minutes.

Tip: if you were familiar with CRAB audits, `safe-to-run` is equivalent to
`code_is_not_malicious`. `does-not-implement-crypto` is the opposite of
`implements_crypto`.

##### What if the code contains crypto?

We need to find someone who can review it. Please write an email to gbiv@,
allenwebb@, and palmer@, and they can help figure this out. If your velocity
would be meaningfully impacted by this review, please let us know; we can work
around audit requirements on a case-by-case basis.

##### What if the code contains something malicious?

Please don't import it. :)

We'd also probably want to make sure folks know that malicious code was found in
a crate. Please email chromeos-rust-steering-committee@ to get their guidance.

### Getting audit help

If you have specific questions about audits, or if you feel things need to be
clarified, contributions to docs are always appreciated. Questions can be sent
to chromeos-rust-steering-committee@.

Similarly, if you're hoping to pull in a large amount of third-party code
(guideline is 10,000 lines of audit backlog or more, but it's a soft guideline),
please reach out to chromeos-rust-steering-committee@. We can help you figure
out how to get it all audited, or place temporary audit exemptions to help
ensure this process doesn't slow things down.

### Can you outline a sample audit flow step-by-step?

Let's take const-sha1:0.2.0 from [the Symptoms that auditing is
needed](#symptoms-that-auditing-is-needed) section above as an example.

I run `./scripts/cargo-vet.py inspect const-sha1 0.2.0`, and `cargo-vet` prints
a description of what I'm meant to be auditing for. Further, it gives me a link
that I can click on to see the code to audit:
https://sourcegraph.com/crates/const-sha1@v0.2.0

I open that link in my web browser, and glance first at the `Cargo.toml` for
this crate. The `description` field describes the intent of this crate: it
provides a sha1 implementation for use in const contexts. `sha1` ==
cryptography, so we'll probably need a crypto review.

This crate has no dependencies, but if it did, I'd glance over them to see
whether they're all relevant to what the crate claims to want to do. If any
dependencies are made to interact with the network, I'd keep those in mind to
look a bit closely at how they're used in the code.

I then go into `src/` and skim through each Rust file (in this case, there's just
one). Skimming, I see quite a bit of math, a few struct declarations, a
reasonable public API, and a few tests. Since we never run third-party `#[test]`
or `#[cfg(test)]` code, the tests can be ignored.

The code isn't obviously malicious to me: there's no strange attempts to access
the network or disk, and the implementation of sha1... sure seems like an
implementation of _some kind of crypto_. I can mark the code as `safe-to-run`,
and reach out to the relevant people (see [what if the code contains
crypto](#what-if-the-code-contains-crypto)) to ask for next steps about the
cryptography implemented here.

To mark the code as `safe-to-run`, I interrupt the `cargo-vet inspect` command I
ran earlier, and run
`./scripts/cargo-vet.py certify const-sha1 0.2.0 --criteria=safe-to-run`. I'm
presented with an instance of `${EDITOR}` with text. I uncomment the statement
under "Uncomment the following statement," write the file, and exit the editor.
The audit is recorded so now I can create a commit with the audit changes and
put it up for review. I can then reach out to crypto folks for next steps.

Note that in this case, I uploaded the audit for review _even though `cargo-vet`
was failing_. Depending on crypto reviewers' availability and how quickly I need
to land my changes, I may be asked to either temporarily exempt `const-sha1`
from `crypto-safe` audit requirements. Alternatively, a friendly crypto reviewer
may upload their review on top of mine, which would placate `cargo-vet`.

## Forward intent

For folks wondering what the plans are with `cargo-vet`, the general consensus
seems to be that we should first get most crates reviewed as `safe-to-run` and
`crypto-safe`, since this catches most egregious classes of errors.

In the future, we hope to have more ub-risk reviews available through crates
(both locally, and sourced from the upstream Google audits repo), so folks with
correctness-critical applications can consult those to guide their crate
choices.

If a reviewer would like to provide reviews for any of the `ub-risk-*` levels
or `safe-to-deploy`, that's highly appreciated! But it is also not required at
this time.

### Isn't this all quite burdensome?

Initially auditing hundreds of crates will certainly take a while. `cargo-vet`
has powerful diffing tools to make incremental updates/upgrades proportional in
effort to a crate's diff. In other words, the hope is that the startup cost is
materially less than steady-state.

Less directly, our audits are made available to the FOSS community, and other
teams in Google may benefit from them in the future. (Conversely, we hope to
benefit from other teams' audits in the future, too. :) )
