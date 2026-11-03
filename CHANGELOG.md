## [unreleased]

### Features

- *(core)* Add PogoError, stable error codes, exit codes, and hints
- Add JSON envelope and output formatting with stream discipline
- *(cli)* Define all commands and help text
- Add config loading, devices, and schema commands
- Add serial capture, serial send, device locks, and reconnect
- Add info, flash, reset, run, symbolization, and test firmware
- Add MCP server and move command logic into core ops
- Add pogo doctor and the CHECK_FAILED error code
- Cap output at about 200 KB with OUTPUT_TRUNCATED
- Add the probe wire protocol and host probe client
- Apply config defaults for format, timeout, parser, chip, reset
- Add wait_lock argument to MCP device tools
- Add probe, gpio, and i2c commands and MCP tools
- Add power commands and reset via the probe
- Reset through the probe in serial capture and run

### Bug Fixes

- *(cli)* Show bin name as pogo on every OS

### Documentation

- Rename benchctl to pogo and add CLAUDE.md working rules
- Split README spec into per-section files under docs/
- Add global working rules to CLAUDE.md
- Add no-new-branches rule to CLAUDE.md
- Mark Phase 0 complete and start Phase 1
- Document flash commands; audit serial send; build firmware in CI

### Refactor

- *(cli)* Split cli.rs into a cli module

### Testing

- Give each flash test its own scratch file
- Add opt-in hardware-in-the-loop tests
- Add Xtensa fault fixtures; report abort() line as panic message

### Miscellaneous Tasks

- Add test matrix, schema drift, and cargo-deny checks
- Add license files and crate metadata for release

### Other

- Set up cargo workspace with core, protocol, cli, and mcp crates
