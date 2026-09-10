use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use bytes::Bytes;
use http_body_util::{BodyExt, Empty, Full};
use hyper::body::Incoming;
use hyper::header::{CONNECTION, HOST, UPGRADE};
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;
use tracing::{debug, warn};

use crate::supervisor::Supervisor;

type ProxyError = Box<dyn std::error::Error + Send + Sync>;
pub type ProxyBody = http_body_util::combinators::BoxBody<Bytes, ProxyError>;

/// Pending ACME HTTP-01 challenge responses, keyed by token. Populated by
/// the ACME manager (see `acme.rs`) and consulted by the plain-HTTP
/// listener before applying any HTTPS redirect.
pub type AcmeChallengeStore = Arc<Mutex<std::collections::HashMap<String, String>>>;

#[derive(Clone)]
pub struct ProxyState {
    pub supervisor: Arc<Supervisor>,
    /// Whether requests on this listener arrived over TLS. Plain-HTTP and
    /// HTTPS listeners share this handler with different values.
    pub is_https: bool,
    /// Port the HTTPS listener is actually running on, if any — the
    /// plain-HTTP listener only redirects when there's somewhere useful to
    /// redirect to, and needs the port to build a correct `Location` when
    /// it isn't the standard 443 (e.g. Harbor's own non-privileged
    /// default of 8443).
    pub https_port: Option<u16>,
    pub https_redirect: bool,
    pub acme_challenges: AcmeChallengeStore,
}

fn full_body(bytes: impl Into<Bytes>) -> ProxyBody {
    Full::new(bytes.into())
        .map_err(|never: Infallible| match never {})
        .boxed()
}

fn empty_body() -> ProxyBody {
    Empty::new().map_err(|never: Infallible| match never {}).boxed()
}

fn text_response(status: StatusCode, body: impl Into<Bytes>) -> Response<ProxyBody> {
    Response::builder()
        .status(status)
        .header("content-type", "text/plain; charset=utf-8")
        .body(full_body(body))
        .expect("building a static text response never fails")
}

fn is_upgrade_request<B>(req: &Request<B>) -> bool {
    let has_upgrade_header = req
        .headers()
        .get(UPGRADE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.eq_ignore_ascii_case("websocket"))
        .unwrap_or(false);
    let connection_says_upgrade = req
        .headers()
        .get(CONNECTION)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_ascii_lowercase().contains("upgrade"))
        .unwrap_or(false);
    has_upgrade_header && connection_says_upgrade
}

/// Handle one request on either the plain-HTTP or the TLS listener.
pub async fn handle(
    mut req: Request<Incoming>,
    state: ProxyState,
    client_addr: SocketAddr,
) -> Result<Response<ProxyBody>, Infallible> {
    let start = Instant::now();
    let method = req.method().clone();
    let path = req.uri().path().to_string();

    if !state.is_https {
        if let Some(token) = path.strip_prefix("/.well-known/acme-challenge/") {
            let key_auth = state.acme_challenges.lock().unwrap().get(token).cloned();
            if let Some(key_auth) = key_auth {
                debug!("answering ACME HTTP-01 challenge for token {token}");
                return Ok(text_response(StatusCode::OK, key_auth));
            }
        }

        if let (true, Some(https_port)) = (state.https_redirect, state.https_port) {
            let host = req
                .headers()
                .get(HOST)
                .and_then(|v| v.to_str().ok())
                .map(|h| h.split(':').next().unwrap_or(h).to_string());
            if let Some(host) = host {
                let pq = req.uri().path_and_query().map(|pq| pq.as_str()).unwrap_or("/");
                let host_with_port = match https_port {
                    443 => host,
                    port => format!("{host}:{port}"),
                };
                let location = format!("https://{host_with_port}{pq}");
                return Ok(Response::builder()
                    .status(StatusCode::MOVED_PERMANENTLY)
                    .header("location", location)
                    .body(empty_body())
                    .expect("building a redirect response never fails"));
            }
        }
    }

    let host = req
        .headers()
        .get(HOST)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let route = state.supervisor.resolve_route(host.as_deref(), &path);

    let Some(route) = route else {
        return Ok(text_response(
            StatusCode::NOT_FOUND,
            "no app is configured for this host/path",
        ));
    };

    let forward_path = match &route.strip_prefix {
        Some(prefix) => {
            let stripped = path.strip_prefix(prefix.as_str()).unwrap_or(&path);
            if stripped.is_empty() {
                "/".to_string()
            } else if stripped.starts_with('/') {
                stripped.to_string()
            } else {
                format!("/{stripped}")
            }
        }
        None => path.clone(),
    };
    let forward_uri = match req.uri().query() {
        Some(q) => format!("{forward_path}?{q}"),
        None => forward_path,
    };

    let target_addr: SocketAddr = ([127, 0, 0, 1], route.target_port).into();
    let upgrade_requested = is_upgrade_request(&req);

    *req.uri_mut() = match forward_uri.parse() {
        Ok(uri) => uri,
        Err(e) => {
            warn!("app '{}': invalid forwarded URI: {e}", route.app_name);
            return Ok(text_response(StatusCode::BAD_GATEWAY, "invalid upstream request"));
        }
    };

    let result = if upgrade_requested {
        proxy_upgrade(req, target_addr).await
    } else {
        proxy_plain(req, target_addr).await
    };

    let response = match result {
        Ok(resp) => resp,
        Err(e) => {
            warn!(
                "app '{}': upstream request to 127.0.0.1:{} failed: {e}",
                route.app_name, route.target_port
            );
            text_response(StatusCode::BAD_GATEWAY, "upstream app is not reachable")
        }
    };

    log_access(
        &state.supervisor,
        &route.app_name,
        client_addr,
        &method,
        &path,
        response.status().as_u16(),
        start.elapsed(),
    );

    Ok(response)
}

async fn proxy_plain(
    req: Request<Incoming>,
    target_addr: SocketAddr,
) -> anyhow::Result<Response<ProxyBody>> {
    let stream = TcpStream::connect(target_addr).await?;
    let io = TokioIo::new(stream);
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await?;
    tokio::spawn(async move {
        if let Err(e) = conn.await {
            debug!("upstream connection closed: {e}");
        }
    });

    let resp = sender.send_request(req).await?;
    Ok(resp.map(|body| body.map_err(|e| Box::new(e) as ProxyError).boxed()))
}

/// Proxy a request that asked to be upgraded (WebSocket passthrough,
/// FR11). Opens a dedicated connection to the upstream app, forwards the
/// upgrade request, and — if the upstream agrees (101) — splices raw bytes
/// between the client and upstream connections once both sides have
/// completed their HTTP upgrade handshake.
async fn proxy_upgrade(
    mut req: Request<Incoming>,
    target_addr: SocketAddr,
) -> anyhow::Result<Response<ProxyBody>> {
    let client_upgrade = hyper::upgrade::on(&mut req);

    let stream = TcpStream::connect(target_addr).await?;
    let io = TokioIo::new(stream);
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await?;
    let conn = conn.with_upgrades();
    tokio::spawn(async move {
        if let Err(e) = conn.await {
            debug!("upstream connection closed: {e}");
        }
    });

    let mut upstream_resp = sender.send_request(req).await?;

    if upstream_resp.status() != StatusCode::SWITCHING_PROTOCOLS {
        return Ok(upstream_resp.map(|body| body.map_err(|e| Box::new(e) as ProxyError).boxed()));
    }

    let upstream_upgrade = hyper::upgrade::on(&mut upstream_resp);

    tokio::spawn(async move {
        let (client_io, upstream_io) = match tokio::try_join!(client_upgrade, upstream_upgrade) {
            Ok(pair) => pair,
            Err(e) => {
                warn!("websocket upgrade handshake failed: {e}");
                return;
            }
        };
        let mut client_io = TokioIo::new(client_io);
        let mut upstream_io = TokioIo::new(upstream_io);
        if let Err(e) = tokio::io::copy_bidirectional(&mut client_io, &mut upstream_io).await {
            debug!("websocket tunnel closed: {e}");
        }
    });

    Ok(upstream_resp.map(|_| empty_body()))
}

/// Accept loop for the plain-HTTP proxy listener. Runs until the listener
/// itself errors; individual connection failures are logged and skipped.
pub async fn serve_http(bind_addr: String, state: ProxyState) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .map_err(|e| anyhow::anyhow!("binding proxy HTTP listener on {bind_addr}: {e}"))?;
    tracing::info!("reverse proxy: HTTP listening on {bind_addr}");
    loop {
        let (stream, client_addr) = match listener.accept().await {
            Ok(pair) => pair,
            Err(e) => {
                warn!("proxy accept error: {e}");
                continue;
            }
        };
        let state = state.clone();
        tokio::spawn(async move {
            serve_one_connection(TokioIo::new(stream), state, client_addr).await;
        });
    }
}

/// Serve HTTP/1.1 (with upgrade support) over one already-established
/// connection, whether that's a plain TCP stream or a TLS stream wrapping
/// one — the TLS listener (`tls.rs`) hands this the decrypted stream.
pub async fn serve_one_connection<I>(io: I, state: ProxyState, client_addr: SocketAddr)
where
    I: hyper::rt::Read + hyper::rt::Write + Unpin + Send + 'static,
{
    let service = hyper::service::service_fn(move |req| handle(req, state.clone(), client_addr));
    if let Err(e) = hyper::server::conn::http1::Builder::new()
        .serve_connection(io, service)
        .with_upgrades()
        .await
    {
        debug!("proxy connection error: {e}");
    }
}

fn log_access(
    supervisor: &Supervisor,
    app_name: &str,
    client_addr: SocketAddr,
    method: &hyper::Method,
    path: &str,
    status: u16,
    elapsed: std::time::Duration,
) {
    let line = format!(
        "{} {} \"{} {}\" {} {}ms\n",
        chrono::Utc::now().to_rfc3339(),
        client_addr.ip(),
        method,
        path,
        status,
        elapsed.as_millis(),
    );
    let path = supervisor.paths.app_log_path(app_name, "access");
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        use std::io::Write;
        let _ = file.write_all(line.as_bytes());
    }
}
