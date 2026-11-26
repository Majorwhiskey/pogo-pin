//! What the menu shows: banner, status, and the numbered items. Rendering is pure so
//! it can be snapshot-tested without a terminal.

use crate::output::Style;

const BANNER: [&str; 6] = [
    "██████╗  ██████╗  ██████╗  ██████╗ ",
    "██╔══██╗██╔═══██╗██╔════╝ ██╔═══██╗",
    "██████╔╝██║   ██║██║  ███╗██║   ██║",
    "██╔═══╝ ██║   ██║██║   ██║██║   ██║",
    "██║     ╚██████╔╝╚██████╔╝╚██████╔╝",
    "╚═╝      ╚═════╝  ╚═════╝  ╚═════╝ ",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Run `pogo` with these arguments.
    Run(&'static [&'static str]),
    /// Ask for an ELF and a pattern, confirm, then `pogo run`.
    FlashRun,
    /// Confirm, then `pogo reset`.
    Reset,
    /// Ask for a suite file, then `pogo test`.
    Tests,
    Quickstart,
    Exit,
}

pub struct Item {
    pub key: &'static str,
    pub title: &'static str,
    pub about: &'static str,
    pub action: Action,
}

pub const ITEMS: &[Item] = &[
    item(
        "1",
        "Devices",
        "List connected boards and probes",
        Action::Run(&["devices"]),
    ),
    item(
        "2",
        "Doctor",
        "Check drivers, permissions, and config",
        Action::Run(&["doctor"]),
    ),
    item(
        "3",
        "Flash & Run",
        "Flash an ELF, reset, and watch it boot",
        Action::FlashRun,
    ),
    item(
        "4",
        "Serial Monitor",
        "Watch the board's serial output (Ctrl-C to stop)",
        Action::Run(&["serial", "capture", "--timeout", "1h"]),
    ),
    item(
        "5",
        "Reset Board",
        "Reboot the board into its app",
        Action::Reset,
    ),
    item(
        "6",
        "Run Tests",
        "Run a pogo-test.toml suite on the board",
        Action::Tests,
    ),
    item(
        "7",
        "Set Up Claude Code",
        "Register pogo as an MCP server for Claude Code",
        Action::Run(&["setup", "claude"]),
    ),
    item(
        "8",
        "Start MCP Server",
        "Serve the tools over HTTP on 127.0.0.1:7646 (Ctrl-C to stop)",
        Action::Run(&["serve"]),
    ),
    item(
        "9",
        "Quickstart Guide",
        "Command cheat sheet for terminals and agents",
        Action::Quickstart,
    ),
    item("0", "Exit", "Close pogo", Action::Exit),
];

const fn item(key: &'static str, title: &'static str, about: &'static str, action: Action) -> Item {
    Item {
        key,
        title,
        about,
        action,
    }
}

pub const QUICKSTART: &str = "\
Quickstart
  pogo devices                            list boards and probes
  pogo doctor                             check the setup
  pogo run app.elf --until READY          flash, reset, and watch the boot
  pogo serial capture --until READY       watch output without flashing
  pogo serial send status --expect ok     send a line and wait for the reply
  pogo test                               run the suite in pogo-test.toml
  pogo setup claude                       add pogo to Claude Code
  pogo <command> -f json                  machine-readable output for agents
  pogo <command> --help                   every flag, examples, and exit codes
";

/// What was found on the USB ports when the menu was drawn.
pub struct Status {
    pub boards: Vec<String>,
    pub probe: Option<String>,
}

pub fn render(style: &Style, version: &str, status: &Status) -> String {
    let mut out = String::from("\n");
    for line in BANNER {
        out.push_str(&style.cyan(line));
        out.push('\n');
    }
    out.push('\n');
    out.push_str(&style.bold("Flash, watch, and probe ESP32 boards from the terminal."));
    out.push('\n');
    out.push_str(&style.dim(&format!("v{version} • agent-friendly hardware workbench")));
    out.push_str("\n\n");
    let boards = if status.boards.is_empty() {
        style.yellow("none found; plug one in and pick Devices")
    } else {
        style.green(&status.boards.join(", "))
    };
    out.push_str(&format!("  Boards: {boards}\n"));
    let probe = match &status.probe {
        Some(port) => style.green(port),
        None => style.dim("not connected"),
    };
    out.push_str(&format!("  Probe:  {probe}\n"));
    out.push_str(&format!(
        "  Agents: {} + {}\n\n",
        style.cyan("pogo mcp"),
        style.cyan("-f json")
    ));
    out.push_str(&format!("{:>24}{}\n", "", style.bold("Menu")));
    for i in ITEMS {
        out.push_str(&format!(
            "  {}  {}  {}\n",
            style.cyan(&format!("[{}]", i.key)),
            style.bold(&format!("{:<18}", i.title)),
            style.dim(i.about)
        ));
    }
    out.push('\n');
    out
}

pub fn prompt(style: &Style) -> String {
    format!("{} ", style.cyan("Select an option [0-9] (1):"))
}

/// The item for what was typed; an empty line picks the default, Devices.
pub fn pick(choice: &str) -> Option<&'static Item> {
    let key = match choice.trim() {
        "" => "1",
        k => k,
    };
    ITEMS.iter().find(|i| i.key == key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_without_color() {
        let style = Style { color: false };
        let status = Status {
            boards: vec!["/dev/ttyUSB0".into()],
            probe: None,
        };
        insta::assert_snapshot!(render(&style, "0.1.1", &status));
    }

    #[test]
    fn no_board_says_how_to_find_one() {
        let style = Style { color: false };
        let status = Status {
            boards: vec![],
            probe: Some("/dev/ttyACM1".into()),
        };
        let screen = render(&style, "0.1.1", &status);
        assert!(screen.contains("Boards: none found"), "{screen}");
        assert!(screen.contains("Probe:  /dev/ttyACM1"), "{screen}");
    }

    #[test]
    fn picking_items() {
        assert_eq!(pick("").map(|i| i.title), Some("Devices"));
        assert_eq!(pick(" 3 \n").map(|i| i.action), Some(Action::FlashRun));
        assert_eq!(pick("0").map(|i| i.action), Some(Action::Exit));
        assert!(pick("10").is_none());
        assert!(pick("x").is_none());
    }

    #[test]
    fn every_item_runs_a_real_command() {
        use clap::Parser;
        for i in ITEMS {
            if let Action::Run(args) = i.action {
                let argv = std::iter::once("pogo").chain(args.iter().copied());
                assert!(crate::cli::Cli::try_parse_from(argv).is_ok(), "{}", i.title);
            }
        }
    }
}
