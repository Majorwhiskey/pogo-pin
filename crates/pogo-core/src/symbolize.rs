//! Backtrace addresses to function, file, and line, using the firmware ELF's DWARF.

use std::path::Path;

use addr2line::Loader;
use schemars::JsonSchema;
use serde::Serialize;

use crate::serial::detect::PanicReport;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Frame {
    /// The backtrace address this frame came from, e.g. `0x400d28cd`.
    pub address: String,
    pub function: Option<String>,
    pub file: Option<String>,
    pub line: Option<u32>,
    /// Code inlined into the next frame of the same address.
    pub inlined: bool,
}

/// Resolve each address; addresses the ELF does not cover give a frame with no details.
pub fn symbolize(elf: &Path, addresses: &[String]) -> Result<Vec<Frame>, String> {
    let loader = Loader::new(elf).map_err(|e| format!("cannot load {}: {e}", elf.display()))?;
    let mut frames = Vec::new();
    for address in addresses {
        let Some(pc) = parse_address(address) else {
            continue;
        };
        let start = frames.len();
        if let Ok(mut iter) = loader.find_frames(pc) {
            while let Ok(Some(frame)) = iter.next() {
                let location = frame.location;
                frames.push(Frame {
                    address: address.clone(),
                    function: frame
                        .function
                        .as_ref()
                        .and_then(|f| f.demangle().ok())
                        .map(|f| f.into_owned()),
                    file: location.as_ref().and_then(|l| l.file).map(str::to_string),
                    line: location.and_then(|l| l.line),
                    inlined: true,
                });
            }
        }
        match frames.get_mut(start..) {
            Some([.., last]) => last.inlined = false,
            _ => frames.push(Frame {
                address: address.clone(),
                function: loader.find_symbol(pc).map(str::to_string),
                file: None,
                line: None,
                inlined: false,
            }),
        }
    }
    Ok(frames)
}

/// Fill `report.frames` from its backtrace.
pub fn annotate(report: &mut PanicReport, elf: &Path) -> Result<(), String> {
    report.frames = symbolize(elf, &report.backtrace)?;
    Ok(())
}

fn parse_address(s: &str) -> Option<u64> {
    u64::from_str_radix(s.strip_prefix("0x")?, 16).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_elf_is_an_error() {
        let err = symbolize(Path::new("/nonexistent/app.elf"), &["0x1".into()]).unwrap_err();
        assert!(err.contains("/nonexistent/app.elf"), "{err}");
    }

    #[test]
    fn addresses_parse_as_hex() {
        assert_eq!(parse_address("0x400d28cd"), Some(0x400d_28cd));
        assert_eq!(parse_address("400d28cd"), None);
    }
}
