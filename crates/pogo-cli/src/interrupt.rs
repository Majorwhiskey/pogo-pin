//! Ctrl-C and SIGTERM (docs/agent-contract.md §6.5). The first signal asks the running
//! command to stop and report partial results (exit 130); a second one exits at once, so
//! a command blocked somewhere that does not check the flag can still be stopped.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

static FLAG: OnceLock<Arc<AtomicBool>> = OnceLock::new();

/// Installs the handler on first use. Commands that never call this keep the default
/// signal behavior.
pub fn interrupt_flag() -> Arc<AtomicBool> {
    FLAG.get_or_init(|| {
        let flag = Arc::new(AtomicBool::new(false));
        let seen = Arc::clone(&flag);
        // If a handler cannot be installed, signals keep their default behavior.
        let _ = ctrlc::set_handler(move || {
            if seen.swap(true, Ordering::SeqCst) {
                std::process::exit(130);
            }
        });
        flag
    })
    .clone()
}
