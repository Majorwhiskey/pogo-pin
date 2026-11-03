//! Long help for `pogo test` (help.rs is at its size limit).

pub const TEST: &str = "\
A suite file lists named tests; each test is steps run in order on one locked board:
  { reset = true, until = \"^READY$\", within = \"2s\" }   reset, capture until a match
  { send = \"ping\", expect = \"^pong$\" }                 send a line, capture the reply
  { wait = \"^done$\" }                                  capture until a match, no input
Optional `timeout` per step (reset/wait 10s, send 5s). A top-level `firmware = \"path\"`
is flashed first (relative to the suite file) unless --no-flash. Panics and other
default fail-on patterns fail the step.

Examples:
  pogo test
  pogo test hil/pogo-test.toml --junit results.xml -f json

Exit codes: 0 all passed, 1 a test failed (CHECK_FAILED), 2 bad suite file,
3 device not found/ambiguous, 6 device I/O, 9 device busy

JSON data: { file, device, flash, tests: [ { name, status: passed|failed, duration_ms,
failure: { step, message, lines } } ], passed, failed, junit_path }";
