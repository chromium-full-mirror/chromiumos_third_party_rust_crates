The Cargo.toml files here exist to enumerate dependencies of third-party
packages which we directly pull (and sometimes `emerge`) into ChromeOS. In
general, either we'll simply depend on the package itself (so `[dependencies]`
will contain a single entry), _or_ we'll depend directly on what the package
depends on. The latter is more useful for picking up `[dev-dependencies]`, which
`cargo vendor` ignores for packages which exist solely as dependencies.

Conventionally, the  name of each package is `${third_party_name}-deps`. This is
so that `cargo-vet` doesn't conflate our local packages here with upstream
packages.
