# Hearthsound plugin SDK

Interface definitions, a Rust SDK, and a packaging CLI for Hearthsound plugins.

A Hearthsound plugin is a WebAssembly component that implements one or more of
the server's provider layers (catalog, metadata, artwork, acquisition,
verification, scrobble and discovery; lyrics and playlist import follow later).
This repository is the single source of truth for the interface those components
speak.

## Layout

| Path | Contents |
|---|---|
| `wit/` | The `hearthsound:plugin@0.4.0` WIT package. |
| `crates/hearthsound-plugin` | The Rust SDK a plugin depends on: the catalog provider world, host wrappers, the logging, config, HTTP and KV helpers, and the export macros. |
| `crates/hearthsound-plugin-bindings` | The shared `types` and host-import bindings every provider world remaps to. |
| `crates/hearthsound-plugin-metadata` | The metadata provider world. |
| `crates/hearthsound-plugin-artwork` | The artwork provider world. |
| `crates/hearthsound-plugin-metadata-artwork` | The metadata and artwork provider world. |
| `crates/hearthsound-plugin-catalog-artwork` | The catalog and artwork provider world. |
| `crates/hearthsound-plugin-acquisition` | The acquisition provider world. |
| `crates/hearthsound-plugin-process-acquisition` | The acquisition provider world with the `host-process` import, for plugins that run a configured tool. |
| `crates/hearthsound-plugin-verification` | The verification provider world. |
| `crates/hearthsound-plugin-scrobble-discovery-account-link` | The scrobble, discovery and account-link provider world. |
| `crates/hearthsound-plugin-cli` | The `hsplugin` CLI: `new`, `build`, `check`, `pack`, `run` and `test`. |
| `examples/hello-catalog` | A complete example plugin that answers every search with a greeting. |

## Using the SDK

Add the SDK as a dependency and export a provider:

```toml
[dependencies]
hearthsound-plugin = { git = "https://github.com/hearthsound/plugin-sdk", tag = "v0.4.0" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

The SDK and `hsplugin` check the API version they were built against. A plugin
for `hearthsound:plugin@0.3.0` is built and checked with `tag = "v0.3.0"`; the
server loads both 0.4.0 and 0.3.0 components.

One export macro per provider layer:

```rust,ignore
hearthsound_plugin::export_catalog_provider!(MyCatalog);
hearthsound_plugin::export_metadata_provider!(MyMetadata);
hearthsound_plugin::export_artwork_provider!(MyArtwork);
hearthsound_plugin::export_acquisition_provider!(MyAcquisition);
hearthsound_plugin::export_process_acquisition_provider!(MyToolAcquisition);
hearthsound_plugin::export_verification_provider!(MyVerification);
hearthsound_plugin::export_scrobble_discovery_link_provider!(MyScrobbler);
```

A plugin that serves a layer alongside `artwork` uses the combined macro
`export_metadata_artwork_provider!` or `export_catalog_artwork_provider!`
instead, so one component serves both layers. `export_scrobble_discovery_link_provider!`
exports `scrobble`, `discovery` and `account-link` from one component;
`account-link` is a capability rather than a layer, so the manifest declares
only the layers it provides.

A plugin implements the guest traits of the world it exports, which it finds
under `hearthsound_plugin::catalog_provider`,
`hearthsound_plugin::metadata_provider`,
`hearthsound_plugin::artwork_provider`,
`hearthsound_plugin::metadata_artwork_provider`,
`hearthsound_plugin::catalog_artwork_provider`,
`hearthsound_plugin::acquisition_provider`,
`hearthsound_plugin::process_acquisition_provider`,
`hearthsound_plugin::verification_provider` and
`hearthsound_plugin::scrobble_discovery_link_provider`; the shared `types`, host
imports and helpers are the same for every world. Every world except the catalog
one lives in its own crate because wit-bindgen allows one export macro per crate
for worlds that share an exported interface, and this crate re-exports them, so
a plugin still depends only on `hearthsound-plugin`.

`hearthsound_plugin::host::files` is available to acquisition plugins, which use
it to report the files they produced, and
`hearthsound_plugin::host::process` to an acquisition plugin that runs a tool
from its manifest's `permissions.process.tools`.

Build the component and package it:

```console
cargo build --release --target wasm32-wasip2
hsplugin check
hsplugin pack --out-dir dist
```

See `hearthsound/plugin-template-rust` for a ready-to-fork project.

## The `hsplugin` CLI

`hsplugin` creates, builds, checks, packages, runs and tests a plugin. Install
the repository head with:

```console
cargo install --locked --git https://github.com/hearthsound/plugin-sdk --tag v0.4.0 hearthsound-plugin-cli
```

Every command takes `--manifest <PATH>` (default `manifest.json`). Commands that
read the component take `--wasm <PATH>`; without it they use the release build
output: `build/plugin.wasm` for non-Rust projects, and the cargo output under
`target/wasm32-wasip2/release/` for Rust. A failing command prints
`error: <message>` to stderr and exits 1.

### `new`

```console
hsplugin new --lang rust --id com.example.my-plugin --name "My plugin" --out my-plugin
```

Copies the template for `--lang rust|go|js|python|csharp` from the official
`hearthsound/plugin-template-<lang>` repository, or from `--template <DIR|URL>`
(a local directory, or a git URL cloned at depth 1). It drops `.git`, rewrites
`id`, `name` and `version` (`1.0.0`) in `manifest.json`, and refuses a non-empty
destination. `--id` defaults to `com.example.my-plugin`; `--out` defaults to a
directory named after the id.

### `build`

```console
hsplugin build --manifest manifest.json
```

Chooses the toolchain from the marker file in the project directory:
`Cargo.toml` runs `cargo build --release --target wasm32-wasip2`; `go.mod` runs
`go generate` and then TinyGo with `-target=wasip2`; `package.json` runs
`npm install` and `npm run build`; `pyproject.toml` runs `componentize-py`; a
`*.csproj` runs `dotnet publish -c Release`. The non-Rust builds write
`build/plugin.wasm`.

### `check`

Validates the manifest and the component it names, then prints
`ok: <id> <version> (provides <layers>)`.

### `pack`

```console
hsplugin pack --out-dir dist
```

Runs the `check` validation, then writes `<id>-<version>.hsplugin` into
`--out-dir` (default `.`) and prints `packed <path>`.

### `run`

```console
hsplugin run catalog.search --input search.json --config config.json
```

Calls one function, named `<capability>.<function>` with the WIT names, through
the mock host. The capabilities are `catalog`, `metadata`, `artwork`,
`acquisition`, `verification`, `scrobble`, `discovery`, `account-link` and
`lifecycle`.

Input (`--input`, a JSON file): zero-argument functions need no file. A
single-argument function takes the argument's JSON value directly, a string
such as `"hello-track-1"` for `catalog.get-track` or a record object for
`catalog.search`. A function with several arguments takes an object keyed by
parameter name, in snake_case or WIT kebab-case, for example
`{"id": "hello-artist-1", "limit": 1, "offset": 0}` for `catalog.artist-albums`.
`catalog.search` takes `{"text": "hello", "kinds": ["track"], "limit": 5, "offset": 0}`;
an empty `kinds` list means every kind.

A `user-context` argument may be left out of `--input`: its `user-id` becomes
the nil UUID and its `config` becomes the `--user-config` JSON document, or
`none` when the flag is absent.

```console
hsplugin run scrobble.get-limits
hsplugin run discovery.similar-artists --input seed.json
hsplugin run account-link.verify-account --user-config '{"token":"good"}'
```

`--tool <name>=<path>` (repeatable) maps a tool name to the executable the
`host-process` import starts, for example
`--tool yt-dlp=/usr/local/bin/yt-dlp`. A tool the manifest declares under
`permissions.process.tools` without a mapping is `not-configured`.

Output: one JSON value on stdout. A call that returns a value prints
`{"ok": <result>}`, and a provider error prints `{"error": <variant>}`.
Unit variants are snake_case strings (`"not_found"`); variants with a payload
are one-key objects (`{"rate_limited": 30}`, `{"invalid_config": "..."}`);
`option` `none` is `null`; records use snake_case keys.

The mock host applies the same limits a server would: a 30 second call
deadline; no sockets and no preopened directories; `host-files` rooted at a
temporary staging directory; a process table that runs at most four tools at
once, clears the environment except `PATH`, `HOME` and the entries the plugin
supplies, keeps at most 1 MiB unread per stream and kills a tool once its
timeout passes; a key-value store of 256 KiB per value with a
1 MiB default quota, where a read always answers `none` for a miss because the
mock host has no storage that can fail, while a server reports a storage
failure as `err(reason)`; and HTTP only to hosts in the manifest allowlist (an
exact host, `*.domain`, or a host named by `$config:<field>`), with a 10 second
default timeout capped at 60 seconds, at most 5 redirects and a 10 MiB body cap.
`server-config()` is the `--config` document with the manifest schema defaults
filled in. The mock host is an independent implementation of these limits, not
the server runtime, so a passing run is evidence rather than a guarantee.

### `test`

```console
hsplugin test catalog --manifest manifest.json --config config.json
```

Runs a layer's conformance suite against the built component through the mock
host. The layer is `catalog`, `metadata`, `artwork`, `acquisition`, `scrobble`,
`discovery` or `verification`. Each check prints one line, `ok <n>: <check>` or
`error <n>: <check>: <detail>`, in order. The command exits 0 when every check
passes and 1 otherwise. A layer the component does not export fails each of its
checks with `component does not export ...`.

`--tool <name>=<path>` (repeatable) maps a tool name to the executable the
`host-process` import starts, as it does for `hsplugin run`, so the
`acquisition` suite runs a plugin that spawns a tool
(`hsplugin test acquisition --tool yt-dlp=/usr/bin/yt-dlp`); without a mapping
such a tool is `not-configured`.

Checks, by suite:

- `catalog` (12 checks): `validate-config` accepts the config; it rejects
 invalid JSON with a non-empty list; `health` returns a string; `search`
 returns track, album and artist lists; `get-track`, `get-album` and
 `get-artist` return each id the search produced; `artist-albums` respects
 `limit` and `offset`; `artist-top-tracks` respects `limit`; `get-track` reports
 `not-found` for an unknown id; `lookup-isrc` returns a list or `not-found`; a
 search with `limit` 0 returns empty lists.
- `metadata` (2 checks): `resolve-recording` on a generic hint returns a list or
 a known error (`not-found`, `invalid-config`); `get-artist-info` returns a
 result, and none is allowed.
- `artwork` (4 checks): `find-artwork` for track, album and artist subjects
 returns a list or `not-found`; a playlist subject returns an empty list.
- `acquisition` (4 checks): `get-capabilities` lists granularities;
 `start-search` on a generic track query returns a token; `poll-search` reaches
 `complete` or a provider error within 30 seconds, where a provider error is the
 failed state because the `search-state` variant has no failed case; an unknown
 token is answered with a provider error.
- `scrobble` (2 checks): `get-limits` reports a `max-batch` of at least 1 and a
 `min-interval-ms` of at most 60000, so a host can batch listens without
 flooding the service.
- `discovery` (1 check): `similar-artists` for a seed with a MusicBrainz id
 returns at most `limit` artists, each with a non-empty name.
- `verification` (1 check): `verify` of a fixed fingerprint answers a known
 verdict with a score from 0 to 1.

The acquisition, scrobble, discovery and verification suites are compiled and
clippy-checked. Only `acquisition` runs against a plugin: the CLI test fixture
under `crates/hearthsound-plugin-cli/tests/fixtures/process-acquisition` spawns
a tool in `start-search`, so the `--tool` mapping is covered. The scrobble,
discovery and verification results are unverified until a real provider plugin
runs them.

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your
option.
