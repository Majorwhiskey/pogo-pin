//! [`Flasher`] for Espressif chips, using the espflash library.

use std::time::Instant;

use espflash::connection::{Connection, ResetAfterOperation, ResetBeforeOperation};
use espflash::flasher::{FlashData as EspFlashData, FlashSettings, Flasher as EspflashFlasher};
use espflash::image_format::idf::IdfBootloaderFormat;
use espflash::image_format::{ImageFormat, Segment};
use espflash::target::{Chip, DefaultProgressCallback};

use super::{ChipInfo, FlashImage, FlashOptions, FlashReport, Flasher, Region, RegionKind};
use crate::error::PogoError;
use crate::reset::is_usb_serial_jtag;

/// The ROM bootloader always talks at this rate first.
const ROM_BAUD: u32 = 115_200;

pub struct EspFlasher {
    inner: EspflashFlasher,
}

impl EspFlasher {
    /// Reset `port` into the ROM bootloader and connect. This resets the target.
    /// `chip` overrides detection; `flash_baud` raises the rate after connecting.
    pub fn connect(
        port: &str,
        usb_ids: Option<(u16, u16)>,
        chip: Option<&str>,
        flash_baud: Option<u32>,
        verify: bool,
    ) -> Result<Self, PogoError> {
        let chip = chip
            .map(|c| {
                c.parse::<Chip>()
                    .map_err(|_| PogoError::Config(format!("unknown chip '{c}'")))
            })
            .transpose()?;
        let serial = serialport::new(port, ROM_BAUD)
            .open_native()
            .map_err(|e| PogoError::DeviceIo(format!("cannot open {port}: {e}")))?;
        let port_info = usb_port_info(port).ok_or_else(|| {
            PogoError::DeviceIo(format!("{port} is not a USB serial port espflash can use"))
        })?;
        let before = if is_usb_serial_jtag(usb_ids) {
            ResetBeforeOperation::UsbReset
        } else {
            ResetBeforeOperation::DefaultReset
        };
        let connection = Connection::new(
            serial,
            port_info,
            ResetAfterOperation::HardReset,
            before,
            ROM_BAUD,
        );
        let inner = EspflashFlasher::connect(connection, true, verify, true, chip, flash_baud)
            .map_err(esp_error)?;
        Ok(EspFlasher { inner })
    }
}

impl Flasher for EspFlasher {
    fn info(&mut self) -> Result<ChipInfo, PogoError> {
        let info = self.inner.device_info().map_err(esp_error)?;
        Ok(ChipInfo {
            chip: info.chip.to_string(),
            revision: info
                .revision
                .map(|(major, minor)| format!("v{major}.{minor}")),
            features: info.features,
            mac: info.mac_address,
            flash_size_bytes: Some(u64::from(info.flash_size.size())),
            crystal: Some(info.crystal_frequency.to_string()),
        })
    }

    fn flash(&mut self, image: &FlashImage, opts: &FlashOptions) -> Result<FlashReport, PogoError> {
        let start = Instant::now();
        let chip = self.inner.chip();
        if opts.erase_all {
            self.inner.erase_flash().map_err(esp_error)?;
        }
        let (segments, regions) = match image {
            FlashImage::Bin { data, address } => (
                vec![Segment::new(*address, data)],
                vec![region(*address, data.len(), RegionKind::Raw)],
            ),
            FlashImage::Elf(elf) => {
                let xtal = chip
                    .xtal_frequency(self.inner.connection())
                    .map_err(esp_error)?;
                let flash_data = EspFlashData::new(FlashSettings::default(), 0, None, chip, xtal);
                let format = IdfBootloaderFormat::new(
                    elf,
                    &flash_data,
                    opts.partition_table.as_deref(),
                    opts.bootloader.as_deref(),
                    None,
                    None,
                )
                .map_err(esp_error)?;
                let segments = ImageFormat::from(format).flash_segments();
                let regions = segments
                    .iter()
                    .enumerate()
                    .map(|(i, s)| region(s.addr, s.data().len(), elf_region_kind(i)))
                    .collect();
                (segments, regions)
            }
        };
        self.inner
            .write_bins_to_flash(&segments, &mut DefaultProgressCallback)
            .map_err(esp_error)?;
        if opts.reset_after {
            self.reset_to_run()?;
        }
        let info = self.inner.device_info().ok();
        Ok(FlashReport {
            chip: chip.to_string(),
            flash_size_bytes: info.map(|i| u64::from(i.flash_size.size())),
            regions,
            erased_all: opts.erase_all,
            verified: opts.verify,
            reset_after: opts.reset_after,
            duration_ms: u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX),
        })
    }

    fn reset_to_run(&mut self) -> Result<(), PogoError> {
        let chip = self.inner.chip();
        self.inner
            .connection()
            .reset_after(true, chip)
            .map_err(esp_error)
    }
}

/// espflash's IDF image is written as bootloader, partition table, then app.
fn elf_region_kind(index: usize) -> RegionKind {
    match index {
        0 => RegionKind::Bootloader,
        1 => RegionKind::PartitionTable,
        _ => RegionKind::App,
    }
}

fn region(address: u32, bytes: usize, kind: RegionKind) -> Region {
    Region {
        address: format!("{address:#x}"),
        bytes: bytes as u64,
        kind,
    }
}

fn usb_port_info(port: &str) -> Option<serialport::UsbPortInfo> {
    serialport::available_ports()
        .ok()?
        .into_iter()
        .find(|p| p.port_name == port)
        .and_then(|p| match p.port_type {
            serialport::SerialPortType::UsbPort(info) => Some(info),
            _ => None,
        })
}

fn esp_error(e: espflash::Error) -> PogoError {
    PogoError::DeviceIo(format!("espflash: {e}"))
}
