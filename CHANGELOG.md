# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.0] - 2026-10-25

### Added

- `artist.album-count` on the `types.artist` record, the provider's own album
  count for an artist.

### Changed

- `host-kv.get` returns `result<option<list<u8>>, string>`, so a plugin can
  tell a missing key from a storage failure;
  `hearthsound_plugin::kv::get_json` maps a read failure to
  `ProviderError::Internal`. The WIT package version is now `0.4.0`, so the API
  string a manifest declares and a component is checked against is
  `hearthsound:plugin@0.4.0`.

## [0.3.0] - 2026-10-23

### Added

- The `hearthsound-plugin-verification` crate, which generates the
  `verification` provider world and the `export_verification_provider!` macro.
- The `hearthsound-plugin-scrobble-discovery-account-link` crate, which
  generates the world for plugins that export `scrobble`, `discovery` and
  `account-link` together, with the
  `export_scrobble_discovery_link_provider!` macro.
- The `hearthsound-plugin-process-acquisition` crate, which generates the
  `acquisition` provider world with the `host-process` import and the
  `export_process_acquisition_provider!` macro.
- `hearthsound_plugin::host::process`, the bindings for the `host-process`
  import an acquisition plugin runs external tools through.
- The `scrobble` interface for now-playing updates, finished listens and
  love/hate feedback, exported through the `scrobble-provider` world.
- The `discovery` interface for similar tracks and artists, artist top tracks,
  recommendations and generated playlists, exported through the
  `discovery-provider` world.
- The `verification` interface, which decides whether a downloaded file is the
  recording the server asked for, exported through the
  `verification-provider` world.
- The `account-link` interface for linking a listener's account on a service,
  exported through the `account-link-provider` world and allowed alongside
  `scrobble` or `discovery`.
- The `host-process` host interface, through which an acquisition plugin runs an
  external tool the administrator configured, and the
  `process-acquisition-provider` world that imports it.
- `hsplugin run` accepts `verification`, `scrobble`, `discovery` and
  `account-link` as capabilities, fills a `user-context` argument from a
  `--user-config` document, and maps tool names to executables with repeatable
  `--tool <name>=<path>` flags.
- The mock host implements `host-process`: at most four live processes per run,
  an environment cleared except `PATH`, `HOME` and the plugin's allowlisted
  entries, relative paths confined to the staging directory, 1 MiB of unread
  output per stream and a wall-clock limit that kills the tool.
- The `scrobble`, `discovery` and `verification` conformance suites for
  `hsplugin test`.

### Changed

- `hsplugin check` accepts the `verification`, `scrobble` and `discovery`
  layers, allows an `account-link` export only when the manifest provides
  `scrobble` or `discovery`, and allows a `host-process` import only for an
  acquisition plugin that lists its tools under `permissions.process.tools`.
- The WIT package version is now `0.3.0`, so the API string a manifest declares
  and a component is checked against is `hearthsound:plugin@0.3.0`.
- `permissions.process.tools` lists the tool names a plugin may run rather than
  requiring an empty list, up to eight names of lowercase letters, digits,
  dots and dashes.

### Security

- Updated wasmtime and wasmtime-wasi to 49.0.4, rustls-webpki to 0.103.16 and
  webpki-root-certs to 1.0.10, fixing RUSTSEC-2026-0378, RUSTSEC-2026-0379,
  RUSTSEC-2026-0382 and RUSTSEC-2026-0383.

## [0.2.1] - 2026-10-17

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
- `hsplugin test <catalog|metadata|artwork|acquisition>` runs a layer's
  conformance suite against a built plugin through the mock host, printing
  `ok <n>: <check>` or `error <n>: <check>: <detail>` per check and exiting 1
  when any check fails.

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

[Unreleased]: https://github.com/hearthsound/plugin-sdk/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/hearthsound/plugin-sdk/releases/tag/v0.4.0
[0.3.0]: https://github.com/hearthsound/plugin-sdk/releases/tag/v0.3.0
[0.2.1]: https://github.com/hearthsound/plugin-sdk/releases/tag/v0.2.1
[0.2.0]: https://github.com/hearthsound/plugin-sdk/releases/tag/v0.2.0
[0.1.1]: https://github.com/hearthsound/plugin-sdk/releases/tag/v0.1.1
[0.1.0]: https://github.com/hearthsound/plugin-sdk/releases/tag/v0.1.0
