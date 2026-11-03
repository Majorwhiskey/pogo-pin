# 9. MCP server

`pogo mcp` starts an MCP server on stdio using `rmcp`. It is a thin adapter: each tool calls the same core function as the CLI command (via `spawn_blocking`) and returns the same `data` object as JSON.

| MCP tool | CLI equivalent | Mutates hardware? |
|---|---|---|
| `list_devices` | `devices` | No |
| `doctor` | `doctor` | No |
| `device_info` | `info` | Resets target |
| `flash` | `flash` (no `--erase-all`) | Yes |
| `reset` | `reset` | Yes |
| `serial_capture` | `serial capture` | Only with `reset: true` |
| `serial_send` | `serial send` | Yes |
| `run` | `run` | Yes |
| `run_tests` | `test` | Yes (flashes the suite's firmware unless `no_flash`) |
| `i2c_scan` / `i2c_read` | `i2c scan` / `i2c read` | No |
| `i2c_write` | `i2c write` | Yes |
| `gpio_get` | `gpio get` | No |
| `gpio_set` / `gpio_pulse` | `gpio set` / `gpio pulse` | Yes |
| `power_measure` | `power measure` | No |
| `power_switch` | `power on/off/cycle` | Yes |

**Rules:**

- Guardrails are enforced in the core, so the MCP layer cannot bypass them. `--erase-all` is not exposed via MCP at all.
- Tool descriptions are the agent's only documentation. Each must state units, default timeouts, and whether the tool changes hardware state. Set MCP tool annotations (such as read-only or destructive hints) where the SDK supports them.
- Tools that act on a board take an optional `device` (like `--device`), and probe tools take `probe` (like `--probe`); both kinds take `wait_lock` like `--wait-lock`. Tools that can wait take `timeout` with the CLI's defaults: `flash` (120 s), `serial_capture` and `run` (10 s), `serial_send` (5 s); `power_measure` takes `duration` instead.
- Errors return the same `{ code, message, hint }` object as the CLI.

**Implemented:** `list_devices`, `doctor`, `device_info`, `flash`, `reset`, `serial_capture`, `serial_send`, `run`, `i2c_scan`, `i2c_read`, `i2c_write`, `gpio_get`, `gpio_set`, `gpio_pulse`, `power_measure`, `power_switch` (`action`: `on`, `off`, or `cycle` with `off_time`), and `run_tests` (`pogo test`). The probe tools take `probe` (like `--probe`) and `wait_lock` instead of `device`; I2C addresses and bytes are plain numbers.

**Results:** every tool returns structured content `{ ok, data, error, warnings }`. `data` is exactly the CLI's `data` for the same operation (tests compare them), `error` is the CLI's `{ code, message, hint, details }`, and a result with an error is marked as a tool error. Partial data is kept, e.g. a `TARGET_FAULT` capture still carries its lines and `panic`.

**Arguments:** durations are strings in `humantime` form (`"500ms"`, `"10s"`), addresses are hex strings (`"0x10000"`). The config comes from `POGO_CONFIG` or the usual search from the server's working directory. By default tools do not wait for a busy device: a call on a device another command is using returns `DEVICE_BUSY` at once. Every tool that takes `device` also takes `wait_lock` (a duration, same as `--wait-lock`) to wait up to that long for the device instead. Lines are not streamed; they arrive in the result. `serial_capture`, `serial_send`, and `run` symbolize panic backtraces against `elf` (for `run`, the flashed ELF), else the device's configured `elf`.

**Implementation:** the tools are adapters over `pogo_core::ops`, the same functions the CLI calls, run with `spawn_blocking`. Tool descriptions state units, defaults, and hardware effects, and tools carry `readOnlyHint` / `destructiveHint` annotations (`flash` and `run` are destructive; `list_devices` is read-only).

**Over the network:** `pogo serve` serves the same tools over Streamable HTTP at `/mcp`, with a bearer token required for any non-loopback address ([§7.16](commands.md#716-pogo-serve-remote-bench)).

**Using it with Claude Code:** the CLI works through Claude Code's shell with no setup. For the MCP tools, either:

- `pogo setup claude` registers the server (`claude mcp add --scope user pogo -- <path to pogo> mcp`). It uses pogo's absolute path, so the server starts even when Claude Code's PATH lacks `~/.cargo/bin`, and it leaves an existing `pogo` registration alone. `--scope local|project|user` and `--dry-run` work as usual.
- or the **pogo plugin**, which adds the same server plus skills that teach Claude the workflow (`esp32-hardware`, and `/pogo:test-on-board`): `claude plugin install pogo --marketplace Majorwhiskey/pogo-pin` (Claude Code 2.1.275 or later), or `pogo setup claude --plugin`. The plugin starts `pogo mcp` from PATH.

Use one or the other: with both, pogo's tools are listed twice. The plugin's files are in `plugin/` and the marketplace in `.claude-plugin/marketplace.json`; `claude plugin validate . --strict` checks both.

[Back to README](../README.md)
