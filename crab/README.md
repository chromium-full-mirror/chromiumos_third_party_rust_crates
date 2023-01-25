# CRAB

CRAB has been deprecated, and all third-party crate auditing is now done through
[cargo-vet](https://mozilla.github.io/cargo-vet/) . To run `cargo-vet` locally,
please try `../scripts/cargo-vet.py`.

For more information about cargo-vet within ChromeOS, check out [the
README](../cargo-vet/README.md).

If you have existing CRAB audits that you wouldn't like to use, the following
keys correspond exactly between CRAB and `cargo-vet`:

- CRAB's `code_is_not_malicious = true` is `cargo-vet`'s `safe-to-run` (if the
  code is malicious, please don't check it in or run it ;) )
- CRAB's `implements_crypto = false` is `cargo-vet`'s
  `does-not-implement-crypto`
- CRAB's `implements_crypto = true` requires either review or verification by a
  crypto expert. Please reach out to (FIXME: Who?) for more information.

Any other questions about how to migrate your review? gbiv@ can hopefully help -
email him. :)
