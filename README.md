# Hearthsound plugin SDK

Interface definitions, a Rust SDK, and a packaging CLI for Hearthsound plugins.

A Hearthsound plugin is a WebAssembly component that implements one or more of
the server's provider layers (catalog, metadata, acquisition, and later artwork,
verification, scrobble, discovery, lyrics and playlist import). This repository
is the single source of truth for the interface those components speak.

## Layout

| Path | Contents |
|---|---|
| `wit/` | The `hearthsound:plugin@0.2.0` WIT package. |
| `crates/hearthsound-plugin` | The Rust SDK a plugin depends on: the catalog provider world, host wrappers, the logging, config, HTTP and KV helpers, and the export macros. |
| `crates/hearthsound-plugin-bindings` | The shared `types` and host-import bindings every provider world remaps to. |
| `crates/hearthsound-plugin-metadata` | The metadata provider world. |
| `crates/hearthsound-plugin-acquisition` | The acquisition provider world. |
| `crates/hearthsound-plugin-cli` | The `hsplugin` CLI: `check` and `pack`. |
| `examples/hello-catalog` | A complete example plugin that answers every search with a greeting. |

## Using the SDK

Add the SDK as a dependency and export a provider:

```toml
[dependencies]
hearthsound-plugin = { git = "https://github.com/hearthsound/plugin-sdk", tag = "v0.1.1" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

One export macro per provider layer:

```rust,ignore
hearthsound_plugin::export_catalog_provider!(MyCatalog);
hearthsound_plugin::export_metadata_provider!(MyMetadata);
hearthsound_plugin::export_acquisition_provider!(MyAcquisition);
```

A plugin implements the guest traits of the world it exports, which it finds
under `hearthsound_plugin::catalog_provider`, `hearthsound_plugin::metadata_provider`
and `hearthsound_plugin::acquisition_provider`; the shared `types`, host imports
and helpers are the same for every world. The metadata and acquisition worlds
live in their own crates because wit-bindgen allows one export macro per crate
for worlds that share an exported interface, and this crate re-exports them, so
a plugin still depends only on `hearthsound-plugin`.

`hearthsound_plugin::host::files` is available to acquisition plugins, which use
it to report the files they produced.

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
