Various scripts and helpers used in the gardening of this repository.

- `./run-cargo-vet.py` is used to run `cargo-vet` with special exclusion criteria.
  This will download and install a hermetic version of `cargo-vet` if necessary.
- `./cargo-audit.py` runs cargo-audit, and is run regularly by automation w/
  reporting on findings.
- `./incremental-cargo-update` is a tool used to help keep this repository
  up-to-date. It is run regularly by automation.
