# 13. Roadmap

Each phase ends only when its acceptance criteria pass and a human confirms.

## Phase 0: Skeleton and contract

- [x] Workspace with the four crates; root `exclude = ["firmware/*"]`; `[workspace.lints]` denying `clippy::unwrap_used` and `clippy::expect_used` in non-test code
- [x] `BenchError`, `ErrorCode`, exit code mapping, hints
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

- [ ] `Flasher` trait with espflash implementation; `info`; `flash` for ELF and `.bin`
- [ ] `reset` (DTR/RTS)
- [ ] `run` composite
- [ ] ELF symbolization; Rust panic location parsing
- [ ] Test-target firmware

**Acceptance:** `pogo run <test-target ELF> --until READY` exits 0. `serial send panic` (or `run` with a planted panic) exits 7 with `panic.location` pointing at the correct line. With two boards connected and no `--device`, `flash` exits 3 with candidates listed.

## Phase 3: MCP server

- [ ] `pogo-mcp` crate; `pogo mcp` subcommand; all tools in [§9](mcp.md#9-mcp-server)
- [ ] Verify with the MCP Inspector
- [ ] Document setup with Claude Code

**Acceptance:** in a Claude Code session, the agent completes "flash the test target, confirm READY, trigger a panic, and report the file and line" using only MCP tools. MCP results equal CLI `data` for the same operations.

## Phase 4: Probe firmware, GPIO, I2C

- [ ] `pogo-protocol` crate (no_std) with property tests
- [ ] Probe firmware: USB CDC, framing, handshake, pin allow-list, safe states, GPIO, I2C
- [ ] Host probe client; `probe info|selftest`, `gpio`, `i2c`
- [ ] `reset --via probe`
- [ ] `.uf2` build

**Acceptance:** `probe info` returns versions. `i2c scan` finds a BME280 at `0x76` or `0x77`. `reset --via probe` reboots the target and capture sees the boot. Unplugging the probe releases all pins within 100 ms (verified with an LED or multimeter).

## Phase 5: Power

- [ ] INA226 sampling and streaming on the probe; host statistics and CSV export
- [ ] `power on|off|cycle` with guardrails

**Acceptance:** idle current is within ±5% of a multimeter reading. Wi-Fi transmit peaks are visible in `max`/`p95`. A script doing `power cycle` then `serial capture --until READY` works end to end.

## Phase 6: Polish and release

- [ ] `docs/agent-contract.md`, `docs/hardware.md`, wiring photo
- [ ] Release automation, Homebrew tap, crates.io publish
- [ ] Demo recording ([§16](launch.md#16-launch-and-showcase-checklist))

**Acceptance:** on a fresh machine, install via Homebrew or the shell installer, `pogo doctor` passes, and the demo is reproducible.

## Stretch goals

- Hardware-in-the-loop CI on a Raspberry Pi self-hosted GitHub Actions runner
- STM32 and nRF targets via probe-rs
- Arduino (AVR) boards and Lattice ECP5 FPGAs
- `pogo watch`: a ratatui TUI for humans
- Logic-analyzer mode using RP2040 PIO
- Multi-board farm support

[Back to README](../README.md)
