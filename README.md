# Hearthsound plugin SDK

Interface definitions, a Rust SDK, and a packaging CLI for Hearthsound plugins.

A Hearthsound plugin is a WebAssembly component that implements one or more of
the server's provider layers (catalog, metadata, acquisition, and later artwork,
verification, scrobble, discovery, lyrics and playlist import). This repository
is the single source of truth for the interface those components speak.

## Layout

| Path | Contents |
|---|---|
| `wit/` | The `hearthsound:plugin@0.1.0` WIT package. |
| `crates/hearthsound-plugin` | Rust SDK: generated bindings, host imports, logging, config helpers and the export macro. |
| `crates/hearthsound-plugin-cli` | The `hsplugin` CLI: `check` and `pack`. |
| `examples/hello-catalog` | A complete example plugin that answers every search with a greeting. |

## Using the SDK

Add the SDK as a dependency and export a provider:

```toml
[dependencies]
hearthsound-plugin = { git = "https://github.com/hearthsound/plugin-sdk", tag = "v0.1.0" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

```rust,ignore
hearthsound_plugin::export_catalog_provider!(MyCatalog);
```

Build the component and package it:

```console
cargo build --release --target wasm32-wasip2
hsplugin check
hsplugin pack --out-dir dist
```

See `hearthsound/plugin-template-rust` for a ready-to-fork project.

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your
option.
