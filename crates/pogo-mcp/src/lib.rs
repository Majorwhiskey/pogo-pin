//! MCP server for pogo (docs/mcp.md §9): each tool runs the same core op as the CLI
//! command and returns the same `data`, plus `error` and `warnings`.

mod convert;
mod params;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Implementation, ServerCapabilities, ServerConfig};
use rmcp::{ServerHandler, ServiceExt, tool, tool_handler, tool_router};

use convert::{blocking, capture_request, flash_request, parse_duration, selection};
use params::{
    DeviceParams, DoctorParams, FlashParams, ListDevicesParams, ResetModeParam, ResetParams,
    RunParams, SerialCaptureParams, SerialSendParams,
};
use pogo_core::ops::{self, DEFAULT_CAPTURE_TIMEOUT, DEFAULT_SEND_TIMEOUT};

const INSTRUCTIONS: &str = "pogo drives ESP32 boards on this machine: list them, flash \
firmware, reset, capture serial output until a pattern or a crash, and send commands. \
Every result is { ok, data, error, warnings }; error has a stable code (DEVICE_NOT_FOUND, \
TIMEOUT, TARGET_FAULT, ...) and a hint. Start with list_devices. Use run to flash and \
watch a boot in one step.";

#[derive(Debug, Clone)]
pub struct PogoServer {
    tool_router: ToolRouter<Self>,
}

impl Default for PogoServer {
    fn default() -> Self {
        PogoServer {
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl PogoServer {
    #[tool(
        description = "List connected serial devices and classify them (esp32_target, \
            pogo_probe, unknown_serial). Never opens ports; read-only and fast.",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn list_devices(&self, Parameters(p): Parameters<ListDevicesParams>) -> CallToolResult {
        blocking(move || ops::devices::devices(None, p.all)).await
    }

    #[tool(
        description = "Check the environment: config parses, boards are visible, serial \
            ports are accessible (Linux groups, ModemManager), and whether a pogo probe is \
            connected. Each check is pass, warn, or fail with a hint; any fail sets error \
            CHECK_FAILED. Read-only unless deep, which also opens each board's port (no \
            reset).",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn doctor(&self, Parameters(p): Parameters<DoctorParams>) -> CallToolResult {
        let config = std::env::var_os("POGO_CONFIG").map(std::path::PathBuf::from);
        blocking(move || ops::doctor::doctor(config, p.deep)).await
    }

    #[tool(
        description = "Read chip, revision, features, MAC, and flash size from the ROM \
            bootloader. RESETS the target (it is restarted into its app afterwards). \
            Takes several seconds.",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn device_info(&self, Parameters(p): Parameters<DeviceParams>) -> CallToolResult {
        let sel = selection(p.device);
        blocking(move || ops::flash::info(&sel)).await
    }

    #[tool(
        description = "Flash an ELF (preferred: bootloader, partition table, and app) or a \
            raw .bin at `address`. CHANGES the device's firmware. Verifies and restarts the \
            app unless told not to. Raw writes below 0x10000 are refused unless \
            allow_bootloader_write. A full chip erase is not available here.",
        annotations(read_only_hint = false, destructive_hint = true)
    )]
    async fn flash(&self, Parameters(p): Parameters<FlashParams>) -> CallToolResult {
        let sel = selection(p.device.clone());
        let req = match flash_request(&p) {
            Ok(r) => r,
            Err(e) => return convert::fail(e),
        };
        let (reset_after, dry_run) = (!p.no_reset, p.dry_run);
        blocking(move || ops::flash::flash(&sel, &req, reset_after, dry_run)).await
    }

    #[tool(
        description = "Reset the target into its app (mode \"run\", default) or into the \
            ROM bootloader (mode \"bootloader\") by toggling DTR/RTS. Restarts the target. \
            Waits up to 5 s for native USB boards to re-enumerate.",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn reset(&self, Parameters(p): Parameters<ResetParams>) -> CallToolResult {
        let sel = selection(p.device);
        let mode = match p.mode {
            ResetModeParam::Run => ops::reset::ResetMode::Run,
            ResetModeParam::Bootloader => ops::reset::ResetMode::Bootloader,
        };
        let via = ops::reset::ResetVia::Auto;
        blocking(move || ops::reset::reset(&sel, mode, via, p.dry_run)).await
    }

    #[tool(
        description = "Capture serial output until a line matches `until` (outcome \
            matched), a fault is seen (panic, abort, brownout, watchdog, unexpected reboot: \
            outcome fault, error TARGET_FAULT), or the timeout passes (default 10 s; outcome \
            timeout if `until` was given, else completed). Lines carry t_ms (host ms since \
            start), level, tag, and device_ts_ms. Panics report file:line; with `elf` the \
            backtrace is symbolized. Does not touch the target unless reset is true.",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn serial_capture(
        &self,
        Parameters(p): Parameters<SerialCaptureParams>,
    ) -> CallToolResult {
        let sel = selection(p.device.clone());
        let (req, timeout) = match capture_request(&p.capture).and_then(|req| {
            Ok((
                req,
                parse_duration(p.timeout.as_deref(), DEFAULT_CAPTURE_TIMEOUT)?,
            ))
        }) {
            Ok(r) => r,
            Err(e) => return convert::fail(e),
        };
        let reset = p.reset;
        blocking(move || {
            let live = ops::capture::Live {
                on_line: &mut |_| {},
                cancel: None,
            };
            ops::capture::serial_capture(&sel, &req, reset, timeout, live)
        })
        .await
    }

    #[tool(
        description = "Write data to the target's serial port (line ending appended, \
            default lf). With `expect`, then capture until a reply line matches (default \
            timeout 5 s), with the same fault detection as serial_capture. Sends input to \
            the running firmware.",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn serial_send(&self, Parameters(p): Parameters<SerialSendParams>) -> CallToolResult {
        let sel = selection(p.device.clone());
        let parsed = convert::send_request(&p).and_then(|req| {
            Ok((
                req,
                parse_duration(p.timeout.as_deref(), DEFAULT_SEND_TIMEOUT)?,
            ))
        });
        let (req, timeout) = match parsed {
            Ok(r) => r,
            Err(e) => return convert::fail(e),
        };
        blocking(move || {
            let live = ops::capture::Live {
                on_line: &mut |_| {},
                cancel: None,
            };
            ops::send::serial_send(&sel, &req, timeout, live)
        })
        .await
    }

    #[tool(
        description = "The main loop for testing firmware: flash `image`, reset, and capture \
            from the first boot byte until `until` matches, a fault occurs, or the timeout \
            (default 10 s after flashing). CHANGES the device's firmware unless no_flash. \
            Panic backtraces are symbolized against the flashed ELF. data has device, \
            flash, and capture (same shape as serial_capture's data).",
        annotations(read_only_hint = false, destructive_hint = true)
    )]
    async fn run(&self, Parameters(p): Parameters<RunParams>) -> CallToolResult {
        let sel = selection(p.device.clone());
        let flash_params = FlashParams {
            image: p.image.clone(),
            address: p.address.clone(),
            ..FlashParams::default()
        };
        let parsed = flash_request(&flash_params).and_then(|f| {
            let c = capture_request(&p.capture)?;
            Ok((
                f,
                c,
                parse_duration(p.timeout.as_deref(), DEFAULT_CAPTURE_TIMEOUT)?,
            ))
        });
        let (flash, capture, timeout) = match parsed {
            Ok(r) => r,
            Err(e) => return convert::fail(e),
        };
        let (no_flash, dry_run) = (p.no_flash, p.dry_run);
        blocking(move || {
            let live = ops::capture::Live {
                on_line: &mut |_| {},
                cancel: None,
            };
            let flash = (!no_flash).then_some(&flash);
            ops::run::run(&sel, flash, &capture, dry_run, timeout, live)
        })
        .await
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for PogoServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("pogo", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }
}

/// Serve MCP on stdin/stdout until the client disconnects.
pub fn serve_stdio() -> Result<(), String> {
    let runtime = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    runtime.block_on(async {
        let service = PogoServer::default()
            .serve(rmcp::transport::stdio())
            .await
            .map_err(|e| e.to_string())?;
        service.waiting().await.map_err(|e| e.to_string())?;
        Ok(())
    })
}
