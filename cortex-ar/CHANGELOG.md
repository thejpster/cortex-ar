# Change Log

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/)
and this project adheres to [Semantic Versioning](http://semver.org/).

## [unreleased]

### Added

- General support for the Cortex-A architecture.
- New `sev` function in ASM module.
- Compiler fences for `dsb` and `isb`
- Added `nomem`, `nostack` and `preserves_flags` options for ASM where applicable.

## [v0.1.0]

Initial release

[unreleased]: https://github.com/rust-embedded/cortex-ar/compare/cortex-ar-v0.1.0...HEAD
[v0.1.0]: https://github.com/rust-embedded/cortex-ar/releases/tag/cortex-ar-v0.1.0
