# 15. CI, release, and distribution

**CI (`.github/workflows/ci.yml`):**

- Matrix: `ubuntu-latest`, `macos-latest`, `windows-latest`. Install `libudev-dev` on Linux.
- Jobs: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test --workspace`, `cargo deny check` (licenses and advisories).
- Schema drift: regenerate schemas and fail on `git diff --exit-code schemas/`.
- Firmware: build `firmware/probe` for `thumbv6m-none-eabi` (RP2040) and `thumbv8m.main-none-eabihf` (RP2350); build `firmware/test-target` for `riscv32imc-unknown-none-elf` (ESP32-C3).

**Release:**

- Use cargo-dist (`dist`) to build binaries for Linux x86_64 and aarch64, macOS arm64 and x86_64, and Windows x86_64, plus shell and PowerShell installers and a Homebrew formula in a tap repository.
- Publish crates to crates.io (`pogo-protocol`, `pogo-core`, `pogo-cli`, `pogo-mcp`).
- Attach probe `.uf2` files to each GitHub release.
- Semantic versioning for the tool; `schema_version` versions the JSON contract independently.
- Generate `CHANGELOG.md` from Conventional Commits with `git-cliff`.

[Back to README](../README.md)
