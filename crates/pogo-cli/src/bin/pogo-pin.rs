//! `pogo-pin`: the project's name as a command. It runs the `pogo` installed beside it
//! with the same arguments, so bare `pogo-pin` opens the menu and `pogo-pin devices`
//! is `pogo devices`.

use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let exe = std::env::current_exe().ok().and_then(|p| {
        Some(
            p.parent()?
                .join(format!("pogo{}", std::env::consts::EXE_SUFFIX)),
        )
    });
    let Some(pogo) = exe.filter(|p| p.is_file()) else {
        eprintln!("error: cannot find the `pogo` binary next to `pogo-pin`; reinstall pogo-pin");
        return ExitCode::from(1);
    };
    let mut cmd = Command::new(pogo);
    cmd.args(std::env::args_os().skip(1));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // exec replaces this process, so signals and the exit code are pogo's own.
        let err = cmd.exec();
        eprintln!("error: cannot start pogo: {err}");
        ExitCode::from(1)
    }
    #[cfg(not(unix))]
    match cmd.status() {
        Ok(s) => ExitCode::from(u8::try_from(s.code().unwrap_or(1)).unwrap_or(1)),
        Err(err) => {
            eprintln!("error: cannot start pogo: {err}");
            ExitCode::from(1)
        }
    }
}
