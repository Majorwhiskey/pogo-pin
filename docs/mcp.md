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

**Using it with Claude Code:** the CLI already works through Claude Code's shell from day one. To add the MCP server too, register `pogo mcp` with `claude mcp add` or a project-level `.mcp.json` file; check the Claude Code MCP documentation for the current syntax.

[Back to README](../README.md)
