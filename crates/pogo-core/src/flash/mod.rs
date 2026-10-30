//! Flashing and chip info behind a target-neutral [`Flasher`] trait (docs/commands.md
//! §7.3–7.4). ESP chips are implemented with espflash in [`esp`]; other targets (STM32,
//! AVR, ECP5) can add their own implementation without touching the commands.

pub mod esp;

use schemars::JsonSchema;
use serde::Serialize;

use crate::discovery::ResolvedDevice;
use crate::error::PogoError;

/// Raw writes below this offset hit the bootloader or partition table.
pub const APP_REGION_START: u32 = 0x10000;

pub enum FlashImage {
    /// Preferred: converted to an app image and written with bootloader and partition table.
    Elf(Vec<u8>),
    /// Written as-is at `address`.
    Bin { data: Vec<u8>, address: u32 },
}

impl FlashImage {
    /// ELF by its magic bytes; anything else is a raw binary and needs an address.
    pub fn from_bytes(data: Vec<u8>, address: Option<u32>) -> Result<Self, String> {
        if data.starts_with(b"\x7fELF") {
            return Ok(FlashImage::Elf(data));
        }
        match address {
            Some(address) => Ok(FlashImage::Bin { data, address }),
            None => Err("a raw image needs --address (or pass an ELF)".into()),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct FlashOptions {
    pub verify: bool,
    pub reset_after: bool,
    pub erase_all: bool,
    /// `--confirm`: must equal the device's USB serial for `--erase-all`.
    pub confirm: Option<String>,
    pub allow_bootloader_write: bool,
    /// Custom bootloader and partition table for ELF images (espflash defaults otherwise).
    pub bootloader: Option<std::path::PathBuf>,
    pub partition_table: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RegionKind {
    Bootloader,
    PartitionTable,
    App,
    /// A raw `.bin` written where the caller asked.
    Raw,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Region {
    /// Hex, e.g. `0x10000`.
    pub address: String,
    pub bytes: u64,
    pub kind: RegionKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ChipInfo {
    pub chip: String,
    /// `v3.1` style.
    pub revision: Option<String>,
    pub features: Vec<String>,
    pub mac: Option<String>,
    pub flash_size_bytes: Option<u64>,
    pub crystal: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct FlashReport {
    pub chip: String,
    pub flash_size_bytes: Option<u64>,
    pub regions: Vec<Region>,
    pub erased_all: bool,
    pub verified: bool,
    pub reset_after: bool,
    pub duration_ms: u64,
}

/// `data` for `pogo info`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct InfoData {
    pub device: ResolvedDevice,
    #[serde(flatten)]
    pub info: ChipInfo,
}

/// `data` for `pogo flash`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct FlashData {
    pub device: ResolvedDevice,
    /// Absent with `--dry-run`, which reports `plan` instead.
    #[serde(flatten)]
    pub report: Option<FlashReport>,
    /// What `--dry-run` would do; absent otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<FlashPlan>,
}

/// `--dry-run` output: the request after guardrail checks, without touching the device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct FlashPlan {
    /// `elf` or `bin`.
    pub image: String,
    pub bytes: u64,
    pub address: Option<String>,
    pub erase_all: bool,
    pub verify: bool,
    pub reset_after: bool,
}

impl FlashPlan {
    pub fn new(image: &FlashImage, opts: &FlashOptions) -> Self {
        let (kind, bytes, address) = match image {
            FlashImage::Elf(data) => ("elf", data.len(), None),
            FlashImage::Bin { data, address } => ("bin", data.len(), Some(format!("{address:#x}"))),
        };
        FlashPlan {
            image: kind.to_string(),
            bytes: bytes as u64,
            address,
            erase_all: opts.erase_all,
            verify: opts.verify,
            reset_after: opts.reset_after,
        }
    }
}

pub trait Flasher {
    fn info(&mut self) -> Result<ChipInfo, PogoError>;
    fn flash(&mut self, image: &FlashImage, opts: &FlashOptions) -> Result<FlashReport, PogoError>;
    /// Leave the bootloader and start the application.
    fn reset_to_run(&mut self) -> Result<(), PogoError>;
}

/// Refuse risky requests before touching the device (docs/guardrails.md §11).
pub fn check_guardrails(
    image: &FlashImage,
    opts: &FlashOptions,
    device: &ResolvedDevice,
) -> Result<(), PogoError> {
    if let FlashImage::Bin { address, .. } = image
        && *address < APP_REGION_START
        && !opts.allow_bootloader_write
    {
        return Err(PogoError::GuardrailRefused {
            reason: format!(
                "raw write at {address:#x} is below the app region ({APP_REGION_START:#x}) \
                 and would overwrite the bootloader or partition table; pass \
                 --allow-bootloader-write if that is intended"
            ),
        });
    }
    if opts.erase_all {
        let matches = match (&device.usb_serial, &opts.confirm) {
            (Some(serial), Some(confirm)) => serial.eq_ignore_ascii_case(confirm),
            _ => false,
        };
        if !matches {
            let reason = match &device.usb_serial {
                Some(serial) => format!(
                    "--erase-all wipes the whole chip; --confirm must equal this device's \
                     USB serial ({serial})"
                ),
                None => "--erase-all needs a device with a USB serial to confirm against".into(),
            };
            return Err(PogoError::GuardrailRefused { reason });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::DeviceKind;
    use crate::error::ErrorCode;

    fn device(serial: Option<&str>) -> ResolvedDevice {
        ResolvedDevice {
            alias: None,
            port: "/dev/cu.x".into(),
            usb_serial: serial.map(str::to_string),
            kind: DeviceKind::Esp32Target,
            usb_ids: None,
        }
    }

    fn bin(address: u32) -> FlashImage {
        FlashImage::Bin {
            data: vec![0; 4],
            address,
        }
    }

    #[test]
    fn image_kind_by_magic() {
        assert!(matches!(
            FlashImage::from_bytes(b"\x7fELF....".to_vec(), None),
            Ok(FlashImage::Elf(_))
        ));
        assert!(FlashImage::from_bytes(vec![0xe9, 0, 0, 0], None).is_err());
        assert!(matches!(
            FlashImage::from_bytes(vec![0xe9], Some(0x10000)),
            Ok(FlashImage::Bin {
                address: 0x10000,
                ..
            })
        ));
    }

    #[test]
    fn raw_writes_below_the_app_region_need_permission() {
        let d = device(Some("AA"));
        let opts = FlashOptions::default();
        let err = check_guardrails(&bin(0x8000), &opts, &d).unwrap_err();
        assert_eq!(err.code(), ErrorCode::GuardrailRefused);
        assert!(check_guardrails(&bin(0x10000), &opts, &d).is_ok());
        let allowed = FlashOptions {
            allow_bootloader_write: true,
            ..FlashOptions::default()
        };
        assert!(check_guardrails(&bin(0x0), &allowed, &d).is_ok());
    }

    #[test]
    fn erase_all_needs_the_matching_serial() {
        let elf = FlashImage::Elf(vec![]);
        let with = |confirm: Option<&str>| FlashOptions {
            erase_all: true,
            confirm: confirm.map(str::to_string),
            ..FlashOptions::default()
        };
        let d = device(Some("F4:12:FA"));
        assert!(check_guardrails(&elf, &with(Some("f4:12:fa")), &d).is_ok());
        let wrong = check_guardrails(&elf, &with(Some("00:00")), &d).unwrap_err();
        assert!(wrong.to_string().contains("F4:12:FA"), "{wrong}");
        let no_serial = check_guardrails(&elf, &with(Some("x")), &device(None)).unwrap_err();
        assert_eq!(no_serial.code(), ErrorCode::GuardrailRefused);
    }
}
