# 14. Instructions for Claude Code

## 14.1 Kickoff prompt (for the human to paste)

> Read README.md and every file in docs/. Together they are the specification for this project. Create CLAUDE.md from the template in [§14.3](#143-claudemd-template), ask me the setup questions in [§14.2](#142-start-here), then start Phase 0. Work one phase at a time and stop for my confirmation when a phase's acceptance criteria pass.

## 14.2 Start here

1. Read README.md and all of docs/. [§6](agent-contract.md#6-the-agent-friendly-cli-contract) (the CLI contract) and [§11](guardrails.md#11-safety-guardrails) (guardrails) are non-negotiable.
2. Create `CLAUDE.md` from [§14.3](#143-claudemd-template).
3. Ask the human: Which target boards and OS are you using? Is a board connected right now? Do you have probe hardware yet?
4. Start Phase 0. Do not start a later phase until the current phase's acceptance criteria pass and the human confirms.
5. For each task: briefly plan, implement, write tests, run the checks in [§14.3](#143-claudemd-template), and commit with a Conventional Commits message.

## 14.3 CLAUDE.md template

```markdown
# CLAUDE.md — working rules for pogo

## Project
pogo is an agent-friendly hardware workbench CLI and MCP server for ESP32 boards.
README.md and docs/ are the spec. docs/agent-contract.md and docs/guardrails.md are
non-negotiable.

## Current phase
Phase 0 (update this line as phases complete)

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
```

## 14.4 Working rules (summary)

- Keep commits small and focused on one task.
- Prefer the mock and replay transports for development; real hardware is for acceptance checks.
- When a design decision isn't covered by this spec, write a short ADR in `docs/decisions/NNNN-title.md` (context, decision, consequences) and mention it to the human.
- If the spec seems wrong or contradictory, stop and ask rather than silently diverging.

[Back to README](../README.md)
