//! `pogo serve`: the same MCP tools over Streamable HTTP, so agents on other machines (or
//! in the cloud) can drive the boards on this one (docs/mcp.md §9).

use std::convert::Infallible;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use hyper::body::{Body, Bytes, Frame, Incoming, SizeHint};
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};

use crate::PogoServer;

/// The only path served: `http://<host>:<port>/mcp`.
pub const PATH: &str = "/mcp";

pub struct ServeOptions {
    pub bind: SocketAddr,
    /// Required on every request as `Authorization: Bearer <token>` when set.
    pub token: Option<String>,
    /// Accepted `Host` header values; empty means any (only used with a token).
    pub allowed_hosts: Option<Vec<String>>,
}

/// rmcp's response body, or a short plain-text refusal of our own.
enum ReplyBody<B> {
    Mcp(B),
    Text(Option<Bytes>),
}

impl<B: Body<Data = Bytes, Error = Infallible> + Unpin> Body for ReplyBody<B> {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Infallible>>> {
        match self.get_mut() {
            ReplyBody::Mcp(b) => Pin::new(b).poll_frame(cx),
            ReplyBody::Text(t) => Poll::Ready(t.take().map(|b| Ok(Frame::data(b)))),
        }
    }

    fn is_end_stream(&self) -> bool {
        match self {
            ReplyBody::Mcp(b) => b.is_end_stream(),
            ReplyBody::Text(t) => t.is_none(),
        }
    }

    fn size_hint(&self) -> SizeHint {
        match self {
            ReplyBody::Mcp(b) => b.size_hint(),
            ReplyBody::Text(t) => SizeHint::with_exact(t.as_ref().map_or(0, |b| b.len() as u64)),
        }
    }
}

fn refuse<B>(status: StatusCode, msg: &'static str) -> Response<ReplyBody<B>> {
    let mut r = Response::new(ReplyBody::Text(Some(Bytes::from_static(msg.as_bytes()))));
    *r.status_mut() = status;
    r
}

/// Compare without stopping at the first difference, so timing does not leak the token.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn authorized(req: &Request<Incoming>, token: &Option<String>) -> bool {
    let Some(token) = token else { return true };
    req.headers()
        .get(hyper::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|given| same(given.as_bytes(), token.as_bytes()))
}

/// Serve until the process is stopped. Each accepted connection is handled on its own
/// task; tool calls still run on blocking threads and take device locks as usual.
pub async fn serve(opts: ServeOptions) -> Result<(), String> {
    let mut config = StreamableHttpServerConfig::default();
    if let Some(hosts) = opts.allowed_hosts {
        config = if hosts.is_empty() {
            config.disable_allowed_hosts()
        } else {
            config.with_allowed_hosts(hosts)
        };
    }
    let service = StreamableHttpService::new(
        || Ok(PogoServer::default()),
        Arc::new(LocalSessionManager::default()),
        config,
    );
    let listener = tokio::net::TcpListener::bind(opts.bind)
        .await
        .map_err(|e| format!("cannot listen on {}: {e}", opts.bind))?;
    let token = Arc::new(opts.token);
    loop {
        let (stream, _) = listener
            .accept()
            .await
            .map_err(|e| format!("accept failed: {e}"))?;
        let service = service.clone();
        let token = Arc::clone(&token);
        tokio::spawn(async move {
            let handler = hyper::service::service_fn(move |req: Request<Incoming>| {
                let service = service.clone();
                let token = Arc::clone(&token);
                async move {
                    if req.uri().path() != PATH {
                        return Ok::<_, Infallible>(refuse(
                            StatusCode::NOT_FOUND,
                            "pogo serves MCP at /mcp\n",
                        ));
                    }
                    if !authorized(&req, &token) {
                        return Ok(refuse(
                            StatusCode::UNAUTHORIZED,
                            "missing or wrong bearer token\n",
                        ));
                    }
                    Ok(service.handle(req).await.map(ReplyBody::Mcp))
                }
            });
            // A client that disconnects mid-request ends only its own connection.
            let _ = hyper::server::conn::http1::Builder::new()
                .serve_connection(TokioIo::new(stream), handler)
                .await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::same;

    #[test]
    fn token_comparison() {
        assert!(same(b"s3cret", b"s3cret"));
        assert!(!same(b"s3cret", b"s3creT"));
        assert!(!same(b"s3cret", b"s3cre"));
        assert!(!same(b"", b"x"));
    }
}
