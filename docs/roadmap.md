# 13. Roadmap

Each phase ends only when its acceptance criteria pass and a human confirms.

## Phase 0: Skeleton and contract

- [x] Workspace with the four crates; root `exclude = ["firmware/*"]`; `[workspace.lints]` denying `clippy::unwrap_used` and `clippy::expect_used` in non-test code
- [x] `PogoError`, `ErrorCode`, exit code mapping, hints
- [x] Output module: envelope, `human`/`json`/`jsonl`, stream discipline, colors
- [x] Config loading and merge; `config show|path|validate`
- [x] clap definitions for **all** commands; unimplemented ones return `NOT_IMPLEMENTED`
- [x] `devices` fully implemented; `schema` command; `completions`
- [x] CI ([§15](release.md#15-ci-release-and-distribution))

**Acceptance:** `cargo test` passes on Linux, macOS, and Windows. `pogo devices -f json` prints a valid envelope matching its schema. Every exit code has a test. Help snapshots exist for every command.

## Phase 1: Serial

- [x] `Transport` trait with serial, mock, and replay implementations
- [x] Line splitter, ANSI stripping, `esp-idf` and `rust-log` parsers
- [x] Capture engine: `--until`, `--fail-on`, defaults, `--settle`, `--max-lines`, boot counting, reset reasons, panic block detection
- [x] `serial capture`, `serial send`, `--record`
- [x] Lock files, `DEVICE_BUSY`, `--wait-lock`
- [x] Reconnect by USB serial after re-enumeration
- [x] Ctrl-C handling with partial results

**Acceptance:** all required fixtures ([§12](testing.md#12-testing-strategy)) replay with the correct outcome and exit code. On real hardware, `serial capture --reset --until "rst:0x"` exits 0. Unplugging the board mid-capture returns `DEVICE_IO` with a hint instead of hanging.

## Phase 2: Flash, reset, run, symbolization

- [x] `Flasher` trait with espflash implementation; `info`; `flash` for ELF and `.bin`
- [x] `reset` (DTR/RTS)
- [x] `run` composite
- [x] ELF symbolization; Rust panic location parsing
- [x] Test-target firmware

**Acceptance:** `pogo run <test-target ELF> --until READY` exits 0. `serial send panic` (or `run` with a planted panic) exits 7 with `panic.location` pointing at the correct line. With two boards connected and no `--device`, `flash` exits 3 with candidates listed.

Acceptance met 2026-11-03 on two classic ESP32 boards (an ESP32-WROOM-32 devkit with a CH340, and a CP2102 devkit): all three checks pass, and the hardware tests pass on each board.

## Phase 3: MCP server

- [x] `pogo-mcp` crate; `pogo mcp` subcommand; all tools in [§9](mcp.md#9-mcp-server)
- [ ] Verify with the MCP Inspector
- [x] Document setup with Claude Code

Acceptance met 2026-11-03: a Claude Code session flashed the test target on an ESP32-WROOM-32, saw READY, triggered the panic, and reported `src/main.rs:77:25` using only MCP tools.

**Acceptance:** in a Claude Code session, the agent completes "flash the test target, confirm READY, trigger a panic, and report the file and line" using only MCP tools. MCP results equal CLI `data` for the same operations.

## Phase 4: Probe firmware, GPIO, I2C

- [x] `pogo-protocol` crate (no_std) with property tests
- [ ] Probe firmware: USB CDC, framing, handshake, pin allow-list, safe states, GPIO, I2C
- [x] Host probe client; `probe info|selftest`, `gpio`, `i2c` (tested against a simulated probe)
- [x] `reset --via probe` (host side; also used by `serial capture --reset` and `run`)
- [ ] `.uf2` build

The host side is done; the firmware and the acceptance checks wait for probe hardware (a Pico and an INA226).

**Acceptance:** `probe info` returns versions. `i2c scan` finds a BME280 at `0x76` or `0x77`. `reset --via probe` reboots the target and capture sees the boot. Unplugging the probe releases all pins within 100 ms (verified with an LED or multimeter).

## Phase 5: Power

- [ ] INA226 sampling and streaming on the probe
- [x] Host statistics and CSV export (tested against a simulated probe)
- [x] `power on|off|cycle` with guardrails (host side)

**Acceptance:** idle current is within ±5% of a multimeter reading. Wi-Fi transmit peaks are visible in `max`/`p95`. A script doing `power cycle` then `serial capture --until READY` works end to end.

## Phase 6: Polish and release

- [ ] `docs/agent-contract.md`, `docs/hardware.md`, wiring photo (the docs exist; the photo waits for the probe bench)
- [x] Release automation: cargo-dist builds five platforms with shell and PowerShell installers; v0.1.0 released 2026-11-03
- [ ] Homebrew tap, crates.io publish (deferred; see [release.md](release.md))
- [ ] Demo recording ([§16](launch.md#16-launch-and-showcase-checklist)); firmware, prompt, and steps are ready in [demo.md](demo.md)

**Acceptance:** on a fresh machine, install via Homebrew or the shell installer, `pogo doctor` passes, and the demo is reproducible.

## Stretch goals

- Hardware-in-the-loop CI on a Raspberry Pi self-hosted GitHub Actions runner
- STM32 and nRF targets via probe-rs
- Arduino (AVR) boards and Lattice ECP5 FPGAs
- `pogo watch`: a ratatui TUI for humans
- Logic-analyzer mode using RP2040 PIO
- Multi-board farm support

[Back to README](../README.md)
