# Changelog

## [0.1.2](https://github.com/OS-Lihua/opencoder/compare/v0.1.1...v0.1.2) (2026-03-09)


### Features

* add Homebrew tap, cargo-binstall support, and install docs ([584784e](https://github.com/OS-Lihua/opencoder/commit/584784eff1aaac99f9df8c4008fcb87255a31b86))
* add TUI overlay system with permission and question dialogs ([a7d2659](https://github.com/OS-Lihua/opencoder/commit/a7d2659bbc677249a164a05fbb65b2e79e9b43f6))
* defer provider initialization to after TUI launch ([fd06872](https://github.com/OS-Lihua/opencoder/commit/fd0687298ebaf856b2aff5d112d27c8bec6813c8))
* P0-P1 feature completion — slash commands, snapshot/undo, LSP/MCP tools, markdown rendering, syntax highlighting, readline, file selector, model selector, configurable theme ([1261072](https://github.com/OS-Lihua/opencoder/commit/1261072f1ad3505f1ae8785043c72ee542d1ac69))
* TUI Phase 3-8 — ToolContext, permissions, auto-title, rendering, agent selector, search ([f2fa244](https://github.com/OS-Lihua/opencoder/commit/f2fa244d15dd582490a6c8eb067a8eed9ab4641f))


### Bug Fixes

* bump MSRV to 1.88 and fix PTY test formatting ([3c067b3](https://github.com/OS-Lihua/opencoder/commit/3c067b31f39e302471636f06f33166e2d0308c6f))
* MSRV toolchain action version mapping and cross-platform PTY tests ([edcfd9e](https://github.com/OS-Lihua/opencoder/commit/edcfd9e262eaecd1a5b380da4eace9840b553180))

## [0.1.1](https://github.com/OS-Lihua/opencoder/compare/v0.1.0...v0.1.1) (2026-03-06)


### Features

* initial implementation of opencoder ([265795a](https://github.com/OS-Lihua/opencoder/commit/265795adfdf5ba6c3d5095a8d0756c7ad2d9072c))


### Bug Fixes

* bump MSRV to 1.87 (let-chains) and fix release-please config ([8673278](https://github.com/OS-Lihua/opencoder/commit/867327885e26af8922e07326ae4af170eed71dff))
* resolve all clippy warnings for Rust 1.93+ ([7e4e474](https://github.com/OS-Lihua/opencoder/commit/7e4e474484584c0de2fc81b3c36b00f467f4eae3))
* resolve clippy errors on Rust 1.94 and fix release-please config ([e60ae6d](https://github.com/OS-Lihua/opencoder/commit/e60ae6dae18930f1c0af248367ece6bd2c54f7fb))
