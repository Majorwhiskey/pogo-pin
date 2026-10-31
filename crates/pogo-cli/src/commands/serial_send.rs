//! `pogo serial send`: payload from the argument, `--hex`, or stdin; the rest is core.

use std::io::Read;

use pogo_core::ops::send::{SendRequest, serial_send};
use pogo_core::serial::LogLine;
use pogo_core::serial::send::{SerialSendData, parse_hex};
use pogo_core::{ErrorBody, ErrorCode};

use crate::cli::{GlobalArgs, LineEnding};
use crate::commands::serial::{live, stream_line};
use crate::commands::target::{reply, selection};
use crate::output::{Format, Render, Reply, Style};

pub struct SendArgs<'a> {
    pub data: &'a str,
    pub line_ending: LineEnding,
    pub hex: bool,
    pub expect: Option<&'a str>,
}

pub fn send(global: &GlobalArgs, args: SendArgs, format: Format, style: &Style) -> Reply {
    let mut bytes = match payload(&args) {
        Ok(b) => b,
        Err(msg) => return Reply::error(ErrorBody::new(ErrorCode::Usage, msg)),
    };
    bytes.extend_from_slice(match args.line_ending {
        LineEnding::Lf => b"\n",
        LineEnding::Crlf => b"\r\n",
        LineEnding::None => b"",
    });
    let req = SendRequest {
        bytes,
        expect: args.expect.map(str::to_string),
    };
    let mut stream = |line: &LogLine| stream_line(format, line);
    let result = serial_send(&selection(global), &req, global.timeout, live(&mut stream));
    reply(result, style)
}

/// The bytes to send before the line ending: the argument, or stdin for `-`.
fn payload(args: &SendArgs) -> Result<Vec<u8>, String> {
    let raw = if args.data == "-" {
        let mut buf = Vec::new();
        std::io::stdin()
            .read_to_end(&mut buf)
            .map_err(|e| format!("cannot read stdin: {e}"))?;
        buf
    } else {
        args.data.as_bytes().to_vec()
    };
    if args.hex {
        parse_hex(&String::from_utf8_lossy(&raw))
    } else {
        Ok(raw)
    }
}

impl Render for SerialSendData {
    fn render_human(&self, out: &mut String, style: &Style) {
        let mut summary = format!("sent {} bytes to {}", self.bytes_written, self.device.port);
        if let Some(m) = self.response.as_ref().and_then(|r| r.matched.as_ref()) {
            summary.push_str(&format!(
                "; reply matched /{}/ on line {} after {} ms",
                m.pattern, m.line_index, m.t_ms
            ));
        }
        out.push_str(&style.dim(&format!("-- {summary}")));
        out.push('\n');
    }
}
