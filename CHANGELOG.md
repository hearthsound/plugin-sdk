# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- The `hearthsound:plugin@0.1.0` WIT package.
- The `hearthsound-plugin` crate with shared bindings, host import wrappers,
  config helpers, logging macros and the catalog provider export macro.
- A `hello-catalog` example plugin that builds for `wasm32-wasip2`.
- The draft-07 plugin manifest schema at `schema/manifest.schema.json`.
- The `hsplugin` CLI with a `check` subcommand that validates a manifest and
  the component rules of a compiled plugin.
