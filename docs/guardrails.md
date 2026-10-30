# 11. Safety guardrails

| Operation | Risk | Guardrail |
|---|---|---|
| eFuse burning, secure boot, flash encryption | Permanently locks or bricks the chip | **Not implemented, ever.** Requests return `GUARDRAIL_REFUSED` with an explanation |
| Full chip erase | Wipes NVS (Wi-Fi credentials, calibration, keys) | Requires `--erase-all` **and** `--confirm <usb_serial>` matching the resolved device; not exposed via MCP |
| Flashing the wrong board | Overwrites another device | `auto` refuses when more than one candidate exists; output echoes the device's USB serial |
| Raw writes to bootloader/partition table area | Unbootable device | Refused unless `--allow-bootloader-write` |
| GPIO on undeclared pins | Short circuits, damaged pins | Only pins declared in config; firmware allow-list as a second layer |
| I2C writes to protected addresses | Corrupted EEPROM contents | `i2c_protected_addrs` (default `0x50–0x57`) |
| Rapid power cycling | Brownouts, flash corruption | Minimum off-time and cycles-per-minute limits |
| Concurrent access | Corrupted flashing or interleaved output | Per-device lock files ([§6.8](agent-contract.md#68-concurrency)) |

**`--dry-run`:** every mutating command supports it and returns the planned actions in `data.plan` without touching hardware.

**Audit log:** every mutating action is appended as JSONL to the platform state directory (for example `~/.local/state/pogo/audit.jsonl`) with timestamp, command, device serial, arguments, and result. Each line is `{ ts_unix_ms, command, port, usb_serial, args, result }`, where `result` is `ok` or the error code. On macOS and Windows, where there is no state directory, the log lives in the local data directory (`~/Library/Application Support/pogo/audit.jsonl` on macOS). `flash`, `reset`, and `serial send` are logged; `serial send` records the payload size, not its contents, since it may carry credentials. This gives a human a clear record of what an agent did to their hardware.

[Back to README](../README.md)
