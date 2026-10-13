# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/hearthsound/plugin-sdk/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/hearthsound/plugin-sdk/releases/tag/v0.1.0
