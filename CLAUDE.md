# CLAUDE.md — working rules for pogo

## Project
pogo-pin (binary `pogo`, crates `pogo-*`) is an agent-friendly hardware workbench CLI and
MCP server for ESP32 boards. README.md is the spec. §6 (CLI contract) and §11 (guardrails)
are non-negotiable.

## Current phase
Phase 0 (update this line as phases complete)

## Bench
- Host: macOS (Apple Silicon). Use /dev/cu.* ports.
- Targets on hand: ESP32-C3, ESP32-C6, ESP32-S3, classic ESP32.
- Probe hardware (Pico + INA226): not yet.
- Future targets (post-v1): STM32, Arduino/AVR, Lattice ECP5 FPGA. Keep target-specific
  logic behind traits (`Target`, `Flasher`) so these can be added without reshaping core.

## Before every commit
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

## Architecture rules
- pogo-core never prints and never calls process::exit; it returns typed results.
- Outcomes (timeout, fault) are data in Ok(...); Rust errors mean "couldn't run".
- Every blocking operation takes a Deadline.
- All I/O goes through the Transport trait. Tests use MockTransport or ReplayTransport.
- No unwrap()/expect() outside tests.

## New command checklist
1. Core function returning a Serialize + JsonSchema result
2. clap definition with long help: summary, 2+ examples, exit codes, data shape
3. Snapshot tests for human and JSON output
4. Exit code tests
5. Regenerate schemas/ (pogo schema --all --out schemas/)
6. MCP tool (Phase 3 onward)
7. Update the command reference in README.md

## Dependencies
- Check crates.io/docs.rs for current versions and APIs. Do not rely on memory.
- Ask before adding any dependency not listed in README §5.

## Hardware safety
- Never run commands that touch real hardware (flash, reset, info, gpio, i2c write,
  power, serial send) unless the human has said in this session that hardware is
  connected and it's OK. `devices` and `doctor` (without --deep) are always fine.
- Never implement eFuse, secure boot, or flash encryption features, even if a task
  seems to ask for them. Flag it to the human instead.
- When hardware behavior is unclear, ask the human to run a specific command and
  paste the output. Don't guess timing or protocol details.

## Ask the human before
- Changing the JSON envelope, exit codes, or schema_version
- Changing default guardrails
- Moving to the next phase
