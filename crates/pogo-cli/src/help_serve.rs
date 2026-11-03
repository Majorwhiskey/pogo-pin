//! Long help for `pogo serve` (help.rs is at its size limit).

pub const SERVE: &str = "\
Serves the same MCP tools as `pogo mcp` over Streamable HTTP at http://<bind>/mcp, so
agents on other machines (Claude Code on the web, CI runners, teammates) can use the
boards connected here. Listens on 127.0.0.1:7646 unless --bind is given. Any address that
is not loopback requires a token, sent by clients as `Authorization: Bearer <token>`;
prefer POGO_SERVE_TOKEN over --token so it stays out of shell history. There is no TLS:
reach a remote bench through SSH port forwarding, Tailscale, or a TLS reverse proxy.
Runs until stopped (Ctrl-C). Device locks apply across all clients.

Examples:
  pogo serve
  POGO_SERVE_TOKEN=$(openssl rand -hex 32) pogo serve --bind 0.0.0.0:7646
  claude mcp add --transport http pogo http://bench.local:7646/mcp \\
      --header \"Authorization: Bearer $POGO_SERVE_TOKEN\"

Exit codes: 0 stopped, 2 refused (non-loopback address without a token), 1 server error";
