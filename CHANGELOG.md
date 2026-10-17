# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `hsplugin new --lang rust|go|js|python|csharp` scaffolds a plugin from the
  language template, from a local directory or a git URL via `--template`,
  rewriting the manifest's `id`, `name` and `version` and refusing a non-empty
  destination.
- `hsplugin build` compiles a plugin with the toolchain its project uses (cargo,
  TinyGo, npm, componentize-py or `dotnet publish`); non-Rust components are
  written to `build/plugin.wasm`, which `check` and `pack` find without
  `--wasm`.
- `hsplugin run <capability>.<function>` calls one function of a built plugin
  against a mock host that enforces the manifest's HTTP allowlist, the KV quota,
  the `host-files` staging root and the 30 second call deadline, printing the
  result as `{"ok": ...}` or `{"error": ...}`.

## [0.2.0] - 2026-10-16

### Added

- The `artwork` provider interface, its `artwork-subject` record and the
  `artwork-provider` world a plugin exports to resolve cover images for albums
  and portraits for artists.
- The `get-artist-info` function on the `metadata` interface with the
  `artist-info-request`, `artist-link` and `artist-info` records.
- The `hearthsound-plugin-artwork` crate, which generates the `artwork` provider
  world and the `export_artwork_provider!` macro.
- The `hearthsound-plugin-metadata-artwork` and
  `hearthsound-plugin-catalog-artwork` crates, whose worlds serve `metadata` or
  `catalog` alongside `artwork`, with the `export_metadata_artwork_provider!`
  and `export_catalog_artwork_provider!` macros.
- `artwork` as a layer `hsplugin check` accepts, so a component exporting the
  interface satisfies a manifest that provides the layer.

### Changed

- The WIT package version is now `0.2.0`, so the API string a manifest declares
  and a component is checked against is `hearthsound:plugin@0.2.0`.

## [0.1.1] - 2026-10-13

### Added

- The `metadata` and `acquisition` provider worlds in the new
  `hearthsound-plugin-metadata` and `hearthsound-plugin-acquisition` crates,
  re-exported by `hearthsound-plugin` with the `export_metadata_provider!` and
  `export_acquisition_provider!` macros.
- The `hearthsound-plugin-bindings` crate, which generates the shared `types`
  and host bindings once so that every provider world remaps its imports there.
- `hearthsound_plugin::host::files`, the `host-files` import an acquisition
  provider reports the files it produced through.
- The `hearthsound_plugin::http` and `hearthsound_plugin::kv` helpers: JSON
  requests with `http-error` and status mapping to `ProviderError`, and values
  stored and read through `host-kv`.

## [0.1.0] - 2026-10-13

### Added

- The `hearthsound:plugin@0.1.0` WIT package.
- The `hearthsound-plugin` crate with shared bindings, host import wrappers,
  config helpers, logging macros and the catalog provider export macro.
- A `hello-catalog` example plugin that builds for `wasm32-wasip2`.
- The draft-07 plugin manifest schema at `schema/manifest.schema.json`.
- The `hsplugin` CLI with a `check` subcommand that validates a manifest and
  the component rules of a compiled plugin.
- An `hsplugin pack` subcommand that writes a validated plugin to a
  `.hsplugin` archive.

[Unreleased]: https://github.com/hearthsound/plugin-sdk/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/hearthsound/plugin-sdk/releases/tag/v0.2.0
[0.1.1]: https://github.com/hearthsound/plugin-sdk/releases/tag/v0.1.1
[0.1.0]: https://github.com/hearthsound/plugin-sdk/releases/tag/v0.1.0
