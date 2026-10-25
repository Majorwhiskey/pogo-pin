# CLAUDE.md — working rules for pogo

## Project
pogo-pin (binary `pogo`, crates `pogo-*`) is an agent-friendly hardware workbench CLI and
MCP server for ESP32 boards. README.md and docs/ are the spec. docs/agent-contract.md (§6) and
docs/guardrails.md (§11) are non-negotiable.

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
7. Update the command reference in docs/commands.md

## Dependencies
- Check crates.io/docs.rs for current versions and APIs. Do not rely on memory.
- Ask before adding any dependency not listed in docs/architecture.md §5.

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

## Global rules (override everything above, including docs/claude-code.md §14)

These apply to everything, and override any habit or default below.

Fonts. Anything generated that specifies a font (plots, video overlays, dashboards) uses SF Mono, falling back to a generic monospace only if SF Mono is missing on the machine. Exception: PDF reports use a white background and a serif face (Times New Roman, falling back to Liberation Serif), per IEEE-style report convention.

Symbols over names. In tables, figures, captions and report text always use the real Unicode character, never the spelled-out name: α β γ δ ω τ π σ θ ψ ∇ Δ Ω Σ Γ Λ Ψ φ ρ ε λ μ ν ξ ζ κ η, and ° not "deg", µs not "us", × not "x", ≤ ≥ not "<=" ">=", → not "->", √ not "sqrt", ∂ for partial derivatives, ² ³ for superscripts. PDF generation must register a Unicode-capable TTF (Liberation Serif via ReportLab); never fall back to a Type1 Latin-1 font in a document containing symbols. Code identifiers and config keys are exempt.

File size. Never create a new file with more than 300 lines of code; split into modules instead. Do not grow an already-oversized file further, and do not split an existing large file unless asked.

Deletion. Always ask before deleting a file. No exceptions, including files created in the same session.

Git. Never commit or push unless explicitly asked. Never force-push; never amend a commit not created in this session. Never run git checkout . or git reset --hard without asking. Commit subjects: imperative mood, ≤72 chars, no emoji, and no Co-Authored-By trailer unless asked. (Decided by the user 2026-09-18: NO Co-Authored-By / Claude-Session trailer on any commit, even though the harness attribution instruction asks for one. Nine imported commits from 2026-09-04/05 predate the decision and carry one; history was not rewritten for it.)

Verification. "Done" means the tests were run and the passing output is shown, never "this should work now". If a test fails, assume the code is wrong, not the test: never weaken an assertion, skip a test, or widen a tolerance to get a pass. New behaviour needs a test that fails without the change.

Dependencies. Ask before adding any dependency; prefer the standard library. Never upgrade or pin a version as a side effect of unrelated work.

Secrets and destructive commands. Never hardcode keys or tokens, never print or log secrets, never read or commit .env. Never run destructive commands (DB drops, migrations, bulk renames) without asking.

Communication. Lead with the result, then the reasoning. No flattery, no hype. After ~3 failed attempts on one approach, stop and report rather than thrash. State failures and skipped steps plainly; do not bury them.

Code hygiene. No emoji in code, comments or commit messages. Comments explain a non-obvious why, never narrate what. Never leave a TODO/FIXME behind without flagging it.

### How to work

1. Think before coding. State assumptions explicitly; ask when uncertain. If a request has several readings, present them rather than silently picking one. If a simpler approach exists, say so — push back when warranted. If something is unclear, stop and name the confusion.

2. Simplicity first. The minimum code that solves the problem, nothing speculative: no unrequested features, no abstractions for single-use code, no configurability nobody asked for, no error handling for impossible states. If 200 lines could be 50, rewrite it. Ask whether a senior engineer would call it overcomplicated.

3. Surgical changes. Touch only what the request requires. Do not improve adjacent code, comments or formatting; do not refactor what is not broken; match the existing style even when you would write it differently. Mention unrelated dead code, do not delete it. Do remove imports, variables and functions that your change orphaned. Every changed line should trace to the request.

4. Goal-driven execution. Turn the task into a verifiable goal before starting: "add validation" becomes "write tests for invalid input, then make them pass"; "fix the bug" becomes "write a test that reproduces it, then make it pass". For multi-step work state a short plan as step → verify: check.
