Various scripts and helpers used in the gardening of this repository.

- `./cargo-vet.py` is used to run `cargo-vet` with special exclusion criteria.
  This will download and install a hermetic version of `cargo-vet` if necessary.
- `./cargo-audit.py` runs cargo-audit, and is run regularly by automation w/
  reporting on findings.
- `./collect-audit-history` is a tool used to reason about how ChromeOS'
  cargo-vet audits have evolved over time. It outputs a CSV to power some
  dashboards. b/279206748 has info on how this is all plumbed, and the dashboard
  is available at go/cros-cargo-vet-dashboard.
- `./incremental-cargo-update` is a tool used to help keep this repository
  up-to-date. It is run regularly by automation.
