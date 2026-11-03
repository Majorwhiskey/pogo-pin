# 15. CI, release, and distribution

**CI (`.github/workflows/ci.yml`):**

- Matrix: `ubuntu-latest`, `macos-latest`, `windows-latest`. Install `libudev-dev` on Linux.
- Jobs: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test --workspace`, `cargo deny check` (licenses and advisories).
- Schema drift: regenerate schemas and fail on `git diff --exit-code schemas/`.
- Firmware: build `firmware/probe` for `thumbv6m-none-eabi` (RP2040) and `thumbv8m.main-none-eabihf` (RP2350); build `firmware/test-target` for `xtensa-esp32-none-elf` (ESP32, the bench board), using the `esp-rs/xtensa-toolchain` action.

**Release:**

- Use cargo-dist (`dist`) to build binaries for Linux x86_64 and aarch64, macOS arm64 and x86_64, and Windows x86_64, plus shell and PowerShell installers and a Homebrew formula in a tap repository.
- Publish crates to crates.io (`pogo-protocol`, `pogo-core`, `pogo-mcp`, `pogo-cli`, in that order). As of 2026-10-31 all four names are free; `pogo` itself is taken by an unrelated 2020 crate, so the install command is `cargo install pogo-cli` (it installs the `pogo` binary). No Homebrew core formula is named `pogo` or `pogo-pin`.
- Each crate ships `LICENSE-MIT` and `LICENSE-APACHE` (symlinks to the workspace root) and the workspace README. `cargo package --workspace --offline` checks that all four package and build before publishing.
- Attach probe `.uf2` files to each GitHub release.
- Semantic versioning for the tool; `schema_version` versions the JSON contract independently.
- Generate `CHANGELOG.md` from Conventional Commits with `git-cliff`.

**How it is set up:** `dist-workspace.toml` holds the dist config (cargo-dist 0.32.0); `dist generate` writes `.github/workflows/release.yml` from it, so edit the config, not the workflow. A release builds the five targets above and attaches them to a GitHub release with a shell and a PowerShell installer; Linux builds install `libudev-dev`, and aarch64 Linux builds natively on `ubuntu-24.04-arm`. Homebrew is left out for now (adding it back is `installers += "homebrew"`, `tap = "Majorwhiskey/homebrew-tap"`, `publish-jobs = ["homebrew"]`, a public `homebrew-tap` repo, and a `HOMEBREW_TAP_TOKEN` secret). `cliff.toml` configures git-cliff 2.14.2. The probe `.uf2` files are not attached yet; that waits for the probe firmware.

**Installing without a release:** `cargo install --git https://github.com/Majorwhiskey/pogo-pin pogo-cli`, or clone and `cargo install --path crates/pogo-cli` (see the README).

**Cutting a release:**

1. Set `version` in the workspace `Cargo.toml` and the `pogo-*` path dependency versions.
2. `git-cliff --tag vX.Y.Z -o CHANGELOG.md` and review it.
3. Commit, then `cargo package --workspace --offline` and the usual checks.
4. `git tag vX.Y.Z && git push origin vX.Y.Z`. The release workflow builds the binaries and installers and creates the GitHub release; it needs no secrets.
5. Optional, later: publish the crates with `cargo login`, then `cargo publish -p pogo-protocol`, `pogo-core`, `pogo-mcp`, `pogo-cli`, in that order. Publishing cannot be undone.

[Back to README](../README.md)
