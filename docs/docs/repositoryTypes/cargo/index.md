# Cargo Registry

Pkgly implements a hosted Cargo registry that is compatible with the sparse index
workflow introduced in Cargo 1.68. The registry exposes the standard publish and download
routes, generates sparse index JSON documents, and stores crate archives alongside project
metadata so that they surface consistently in the packages tab.

## Configuration

- **Mode**: Hosted only. No additional configuration is required beyond enabling the type.
- **Authentication**: All write operations require Pkgly permissions (`Write` action). Reads
  respect repository visibility and the optional repository auth toggle.
- **Index**: Sparse index files are stored under `index/<prefix>/<crate>` following the path
  segmentation rules described in the Cargo book.citeturn1view0

## Client Set-up

Configure a custom registry in `~/.cargo/config.toml`:

```toml
[registries.pkgly]
index = "sparse+https://pkgly.example/repositories/<storage>/<repo>/index/"

[registry]
global-credential-providers = ["cargo:token"]
```

Replace `<storage>` and `<repo>` with the values shown in the UI helper.

Authentication tokens are created from the Pkgly UI. After generating a token, run:

```bash
cargo login --registry pkgly
```

## Publish & Download

- **Publish**: `cargo publish --registry pkgly` uploads the crate via
  `PUT /api/v1/crates/new`. Payload parsing follows the reference specification.
- **Download**: Cargo downloads crates through
  `GET /api/v1/crates/<name>/<version>/download`. Pkgly serves the stored `.crate` archive and
  records metadata for listing.
- **Login Helper**: `GET /api/v1/me` returns JSON guidance pointing back to the Pkgly token UI,
  mirroring the expected sparse registry login behaviour.

## UI Integration

The admin and public interfaces expose Cargo repositories with dedicated helper panels,
including registry URL snippets and a Rust icon for quick identification. Packages appear in
the standard repository packages tab with checksum, feature, and dependency metadata captured
during publish.
