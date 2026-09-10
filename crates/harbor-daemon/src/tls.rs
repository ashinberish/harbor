use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};

use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::server::ClientHello;
use rustls::sign::CertifiedKey;
use rustls::ServerConfig;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tracing::{debug, info, warn};

use crate::proxy::{self, ProxyState};

/// Per-domain TLS certificates, resolved dynamically by SNI. Falls back to
/// a fixed "default" certificate (for `localhost`) when the client didn't
/// send SNI or asked for a host we don't have a cert for — this keeps
/// path-prefix-routed apps (which don't need a domain at all) reachable
/// over HTTPS too.
pub struct CertStore {
    by_domain: RwLock<HashMap<String, Arc<CertifiedKey>>>,
    default: RwLock<Option<Arc<CertifiedKey>>>,
}

impl std::fmt::Debug for CertStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CertStore").finish_non_exhaustive()
    }
}

impl CertStore {
    pub fn new() -> Self {
        Self {
            by_domain: RwLock::new(HashMap::new()),
            default: RwLock::new(None),
        }
    }

    /// Install a certificate for exact-match SNI lookups, e.g. after ACME
    /// issuance or loading a user-supplied cert/key pair.
    pub fn insert(
        &self,
        domain: &str,
        cert_chain: Vec<CertificateDer<'static>>,
        key: PrivateKeyDer<'static>,
    ) -> anyhow::Result<()> {
        let certified = build_certified_key(cert_chain, key)?;
        self.by_domain.write().unwrap().insert(domain.to_string(), certified);
        Ok(())
    }

    pub fn set_default(&self, cert_chain: Vec<CertificateDer<'static>>, key: PrivateKeyDer<'static>) -> anyhow::Result<()> {
        let certified = build_certified_key(cert_chain, key)?;
        *self.default.write().unwrap() = Some(certified);
        Ok(())
    }

    pub fn has(&self, domain: &str) -> bool {
        self.by_domain.read().unwrap().contains_key(domain)
    }

    /// Load a cert/key pair for `domain` from `<cert_dir>/{cert.pem,key.pem}`
    /// if present, generating and persisting a fresh self-signed pair
    /// otherwise. ACME-issued certs (task: acme.rs) are written to the
    /// same location and take priority simply by being written to disk
    /// first, before this is called.
    pub fn ensure_self_signed(&self, domain: &str, cert_dir: &Path) -> anyhow::Result<()> {
        if self.has(domain) {
            return Ok(());
        }
        let (cert_pem, key_pem) = load_or_generate_pem(cert_dir, &[domain.to_string()])?;
        let cert_chain = parse_cert_pem(&cert_pem)?;
        let key = parse_key_pem(&key_pem)?;
        self.insert(domain, cert_chain, key)
    }

    pub fn ensure_default_self_signed(&self, cert_dir: &Path) -> anyhow::Result<()> {
        let (cert_pem, key_pem) = load_or_generate_pem(cert_dir, &["localhost".to_string()])?;
        let cert_chain = parse_cert_pem(&cert_pem)?;
        let key = parse_key_pem(&key_pem)?;
        self.set_default(cert_chain, key)
    }
}

impl Default for CertStore {
    fn default() -> Self {
        Self::new()
    }
}

impl rustls::server::ResolvesServerCert for CertStore {
    fn resolve(&self, client_hello: ClientHello) -> Option<Arc<CertifiedKey>> {
        if let Some(sni) = client_hello.server_name() {
            if let Some(key) = self.by_domain.read().unwrap().get(sni) {
                return Some(key.clone());
            }
        }
        self.default.read().unwrap().clone()
    }
}

fn build_certified_key(
    cert_chain: Vec<CertificateDer<'static>>,
    key: PrivateKeyDer<'static>,
) -> anyhow::Result<Arc<CertifiedKey>> {
    let provider = rustls::crypto::CryptoProvider::get_default()
        .expect("a default rustls CryptoProvider is installed at daemon startup");
    let signing_key = provider
        .key_provider
        .load_private_key(key)
        .map_err(|e| anyhow::anyhow!("loading TLS private key: {e}"))?;
    Ok(Arc::new(CertifiedKey::new(cert_chain, signing_key)))
}

/// Read an existing PEM cert/key pair from `cert_dir`, or generate a fresh
/// self-signed one and persist it there.
fn load_or_generate_pem(cert_dir: &Path, subject_alt_names: &[String]) -> anyhow::Result<(String, String)> {
    let cert_path = cert_dir.join("cert.pem");
    let key_path = cert_dir.join("key.pem");
    if cert_path.exists() && key_path.exists() {
        let cert_pem = std::fs::read_to_string(&cert_path)?;
        let key_pem = std::fs::read_to_string(&key_path)?;
        return Ok((cert_pem, key_pem));
    }

    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(subject_alt_names.to_vec())
            .map_err(|e| anyhow::anyhow!("generating self-signed certificate: {e}"))?;
    let cert_pem = cert.pem();
    let key_pem = signing_key.serialize_pem();

    std::fs::create_dir_all(cert_dir)?;
    std::fs::write(&cert_path, &cert_pem)?;
    std::fs::write(&key_path, &key_pem)?;
    info!("generated self-signed certificate for {subject_alt_names:?} at {}", cert_dir.display());

    Ok((cert_pem, key_pem))
}

pub(crate) fn parse_cert_pem(pem: &str) -> anyhow::Result<Vec<CertificateDer<'static>>> {
    rustls_pemfile::certs(&mut pem.as_bytes())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| anyhow::anyhow!("parsing certificate PEM: {e}"))
}

pub(crate) fn parse_key_pem(pem: &str) -> anyhow::Result<PrivateKeyDer<'static>> {
    rustls_pemfile::private_key(&mut pem.as_bytes())
        .map_err(|e| anyhow::anyhow!("parsing private key PEM: {e}"))?
        .ok_or_else(|| anyhow::anyhow!("no private key found"))
}

pub fn build_server_config(cert_store: Arc<CertStore>) -> anyhow::Result<Arc<ServerConfig>> {
    let mut config = ServerConfig::builder()
        .with_no_client_auth()
        .with_cert_resolver(cert_store);
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Arc::new(config))
}

/// Accept loop for the HTTPS proxy listener: TLS-terminate each connection
/// (resolving the certificate by SNI via `CertStore`) and hand the
/// decrypted stream to the same HTTP/1.1 handling `proxy::serve_http` uses.
pub async fn serve_https(listener: TcpListener, tls_config: Arc<ServerConfig>, state: ProxyState) {
    let acceptor = TlsAcceptor::from(tls_config);
    loop {
        let (stream, client_addr) = match listener.accept().await {
            Ok(pair) => pair,
            Err(e) => {
                warn!("proxy HTTPS accept error: {e}");
                continue;
            }
        };
        let acceptor = acceptor.clone();
        let state = state.clone();
        tokio::spawn(async move {
            match acceptor.accept(stream).await {
                Ok(tls_stream) => {
                    proxy::serve_one_connection(hyper_util::rt::TokioIo::new(tls_stream), state, client_addr)
                        .await;
                }
                Err(e) => debug!("TLS handshake with {client_addr} failed: {e}"),
            }
        });
    }
}
