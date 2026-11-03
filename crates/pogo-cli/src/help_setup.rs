//! Long help for `pogo setup` (help.rs is at its size limit).

pub const SETUP_CLAUDE: &str = "\
Registers `pogo mcp` with Claude Code by running `claude mcp add`, using the absolute path
of this pogo binary so the server starts whatever PATH Claude Code has. A server already
registered as `pogo` is left alone. With --plugin, installs the pogo plugin instead (the
MCP server plus skills that teach Claude the hardware workflow) from the Majorwhiskey/pogo-pin
marketplace. Use one or the other, not both. --dry-run shows the command without running it.

Examples:
  pogo setup claude
  pogo setup claude --scope project --dry-run -f json
  pogo setup claude --plugin

Exit codes: 0 ok (added, already registered, or planned), 1 the claude CLI is missing or
failed (CHECK_FAILED)

JSON data: { claude, method: mcp|plugin, scope, command, status: added|already_registered|planned,
next_steps }";
