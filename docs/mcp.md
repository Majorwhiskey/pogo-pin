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
| `i2c_scan` / `i2c_read` | `i2c scan` / `i2c read` | No |
| `i2c_write` | `i2c write` | Yes |
| `gpio_get` | `gpio get` | No |
| `gpio_set` / `gpio_pulse` | `gpio set` / `gpio pulse` | Yes |
| `power_measure` | `power measure` | No |
| `power_switch` | `power on/off/cycle` | Yes |

**Rules:**

- Guardrails are enforced in the core, so the MCP layer cannot bypass them. `--erase-all` is not exposed via MCP at all.
- Tool descriptions are the agent's only documentation. Each must state units, default timeouts, and whether the tool changes hardware state. Set MCP tool annotations (such as read-only or destructive hints) where the SDK supports them.
- Every tool takes an optional `timeout` and `device` argument with the same semantics as the CLI.
- Errors return the same `{ code, message, hint }` object as the CLI.

**Implemented (Phase 3):** `list_devices`, `device_info`, `flash`, `reset`, `serial_capture`, `serial_send`, and `run`. `doctor` and the probe tools (`i2c_*`, `gpio_*`, `power_*`) are added with their commands; until then they are not listed.

**Results:** every tool returns structured content `{ ok, data, error, warnings }`. `data` is exactly the CLI's `data` for the same operation (tests compare them), `error` is the CLI's `{ code, message, hint, details }`, and a result with an error is marked as a tool error. Partial data is kept, e.g. a `TARGET_FAULT` capture still carries its lines and `panic`.

**Arguments:** durations are strings in `humantime` form (`"500ms"`, `"10s"`), addresses are hex strings (`"0x10000"`). The config comes from `POGO_CONFIG` or the usual search from the server's working directory. Tools do not wait for a busy device: a second call on the same device while one is running returns `DEVICE_BUSY`, so issue device calls one at a time. Lines are not streamed; they arrive in the result.

**Implementation:** the tools are adapters over `pogo_core::ops`, the same functions the CLI calls, run with `spawn_blocking`. Tool descriptions state units, defaults, and hardware effects, and tools carry `readOnlyHint` / `destructiveHint` annotations (`flash` and `run` are destructive; `list_devices` is read-only).

**Using it with Claude Code:** the CLI already works through Claude Code's shell from day one. To add the MCP server too, register `pogo mcp` with `claude mcp add` or a project-level `.mcp.json` file; check the Claude Code MCP documentation for the current syntax. For example, with `pogo` on `PATH` (`cargo install --path crates/pogo-cli`):

```console
claude mcp add pogo -- pogo mcp
```

or a project `.mcp.json`:

```json
{ "mcpServers": { "pogo": { "command": "pogo", "args": ["mcp"] } } }
```

[Back to README](../README.md)
