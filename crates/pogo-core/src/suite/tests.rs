use std::path::Path;
use std::time::Duration;

use super::*;

const SUITE: &str = r#"
firmware = "build/app.elf"

[[test]]
name = "boots"
steps = [{ reset = true, until = "^READY$", within = "2s" }]

[[test]]
name = "ping"
steps = [
  { send = "ping", expect = "^pong$", timeout = "1s" },
  { wait = "^idle$" },
]
"#;

#[test]
fn parses_steps_with_defaults_and_resolves_firmware() {
    let s = parse(SUITE, Path::new("/proj")).unwrap();
    assert_eq!(
        s.firmware.as_deref(),
        Some(Path::new("/proj/build/app.elf"))
    );
    assert_eq!(
        s.tests[0].steps,
        [Step::Reset {
            until: Some("^READY$".into()),
            timeout: DEFAULT_RESET_TIMEOUT,
            within: Some(Duration::from_secs(2)),
        }]
    );
    assert_eq!(
        s.tests[1].steps[0],
        Step::Send {
            data: "ping".into(),
            expect: Some("^pong$".into()),
            timeout: Duration::from_secs(1),
        }
    );
    assert_eq!(
        s.tests[1].steps[1],
        Step::Wait {
            pattern: "^idle$".into(),
            timeout: DEFAULT_WAIT_TIMEOUT,
        }
    );
}

#[test]
fn bad_suites_name_the_test_and_step() {
    let err = |body: &str| parse(body, Path::new(".")).unwrap_err();
    assert!(err("").contains("no tests"));
    let two = "[[test]]\nname = \"a\"\nsteps = [{ reset = true, send = \"x\" }]\n";
    assert!(
        err(two).contains("test 'a' step 1: give exactly one"),
        "{}",
        err(two)
    );
    let misplaced = "[[test]]\nname = \"a\"\nsteps = [{ send = \"x\", until = \"y\" }]\n";
    assert!(err(misplaced).contains("`until` only goes with `reset`"));
    let regex = "[[test]]\nname = \"a\"\nsteps = [{ wait = \"(\" }]\n";
    assert!(err(regex).contains("invalid pattern '('"));
    let time = "[[test]]\nname = \"a\"\nsteps = [{ wait = \"x\", timeout = \"soon\" }]\n";
    assert!(err(time).contains("invalid duration 'soon'"));
    let dup = "[[test]]\nname = \"a\"\nsteps = [{ wait = \"x\" }]\n[[test]]\nname = \"a\"\nsteps = [{ wait = \"x\" }]\n";
    assert!(err(dup).contains("defined twice"));
    assert!(err("[[test]]\nname = \"a\"\nbogus = 1\n").contains("unknown field"));
}

#[test]
fn junit_marks_failures_and_escapes_device_text() {
    let device = crate::discovery::ResolvedDevice {
        alias: None,
        port: "/dev/x".into(),
        usb_serial: None,
        kind: crate::discovery::DeviceKind::Esp32Target,
        usb_ids: None,
    };
    let d = SuiteData {
        file: "pogo-test.toml".into(),
        device,
        flash: None,
        tests: vec![
            TestResult {
                name: "boots".into(),
                status: Status::Passed,
                duration_ms: 1500,
                failure: None,
            },
            TestResult {
                name: "ping <fast>".into(),
                status: Status::Failed,
                duration_ms: 250,
                failure: Some(Failure {
                    step: 1,
                    message: "no line matched \"pong\"".into(),
                    lines: vec!["a & b\u{1b}[0m".into()],
                }),
            },
        ],
        passed: 1,
        failed: 1,
        junit_path: None,
    };
    let x = junit::render(&d);
    assert!(
        x.contains(r#"<testsuites name="pogo" tests="2" failures="1" time="1.750">"#),
        "{x}"
    );
    assert!(x.contains(r#"<testcase name="boots" classname="pogo-test.toml" time="1.500"/>"#));
    assert!(x.contains(r#"<testcase name="ping &lt;fast&gt;""#));
    assert!(
        x.contains(
            r#"<failure message="step 1: no line matched &quot;pong&quot;">a &amp; b[0m</failure>"#
        ),
        "{x}"
    );
}
