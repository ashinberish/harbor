use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use harbor_core::{AcmeConfig, HarborPaths};
use instant_acme::{
    Account, AuthorizationStatus, ChallengeType, Identifier, NewAccount, NewOrder, OrderStatus,
    RetryPolicy,
};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::proxy::AcmeChallengeStore;
use crate::tls::CertStore;

/// Treat an ACME-issued cert as due for renewal after this long — a fixed,
/// conservative fraction of Let's Encrypt's normal 90-day lifetime rather
/// than parsing the certificate's actual `notAfter` (avoids an extra X.509
/// parsing dependency for what is, on any real ACME server, a fairly
/// predictable validity window).
const RENEW_AFTER: std::time::Duration = std::time::Duration::from_secs(60 * 24 * 60 * 60);

#[derive(Debug, Serialize, Deserialize)]
struct CertMeta {
    issued_at_unix: u64,
}

fn needs_issuance(cert_dir: &std::path::Path) -> bool {
    let meta_path = cert_dir.join("acme-meta.json");
    let Ok(text) = std::fs::read_to_string(&meta_path) else {
        return true;
    };
    let Ok(meta) = serde_json::from_str::<CertMeta>(&text) else {
        return true;
    };
    let issued_at = UNIX_EPOCH + std::time::Duration::from_secs(meta.issued_at_unix);
    SystemTime::now()
        .duration_since(issued_at)
        .map(|age| age > RENEW_AFTER)
        .unwrap_or(false)
}

/// Request (or renew) certificates for every domain in `domains` from the
/// configured ACME server, over HTTP-01. Each domain is independent: a
/// failure for one domain is logged and skipped, leaving that domain on
/// whatever self-signed certificate `tls::CertStore` already loaded rather
/// than taking its HTTPS listener down. Requires the domain to be
/// publicly resolvable and reachable on the proxy's HTTP port from the
/// ACME server for HTTP-01 validation to succeed.
pub async fn issue_for_domains(
    acme_config: &AcmeConfig,
    domains: &[String],
    paths: &HarborPaths,
    cert_store: Arc<CertStore>,
    challenges: AcmeChallengeStore,
) {
    let due: Vec<&String> = domains
        .iter()
        .filter(|d| needs_issuance(&paths.domain_cert_dir(d)))
        .collect();
    if due.is_empty() {
        return;
    }

    let account = match get_or_create_account(acme_config, paths).await {
        Ok(a) => a,
        Err(e) => {
            warn!("ACME: failed to set up account: {e:#}; keeping self-signed certificates");
            return;
        }
    };

    for domain in due {
        match issue_one(&account, domain, paths, &cert_store, &challenges).await {
            Ok(()) => info!("ACME: issued certificate for '{domain}'"),
            Err(e) => warn!(
                "ACME: failed to issue certificate for '{domain}': {e:#}; keeping self-signed certificate"
            ),
        }
    }
}

async fn get_or_create_account(acme_config: &AcmeConfig, paths: &HarborPaths) -> anyhow::Result<Account> {
    let creds_path = paths.cert_dir.join("acme-account.json");
    if let Ok(text) = std::fs::read_to_string(&creds_path) {
        if let Ok(creds) = serde_json::from_str(&text) {
            if let Ok(account) = Account::builder()?.from_credentials(creds).await {
                return Ok(account);
            }
            warn!("ACME: stored account credentials were rejected; creating a new account");
        }
    }

    let contact_owned = acme_config.email.as_deref().map(|e| format!("mailto:{e}"));
    let contact: Vec<&str> = contact_owned.as_deref().into_iter().collect();
    let (account, credentials) = Account::builder()?
        .create(
            &NewAccount {
                contact: &contact,
                terms_of_service_agreed: true,
                only_return_existing: false,
            },
            acme_config.directory_url.clone(),
            None,
        )
        .await?;

    if let Some(parent) = creds_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&creds_path, serde_json::to_string_pretty(&credentials)?)?;
    Ok(account)
}

async fn issue_one(
    account: &Account,
    domain: &str,
    paths: &HarborPaths,
    cert_store: &Arc<CertStore>,
    challenges: &AcmeChallengeStore,
) -> anyhow::Result<()> {
    let identifiers = [Identifier::Dns(domain.to_string())];
    let mut order = account.new_order(&NewOrder::new(&identifiers)).await?;

    let mut placed_tokens = Vec::new();
    let mut authorizations = order.authorizations();
    while let Some(result) = authorizations.next().await {
        let mut authz = result?;
        match authz.status {
            AuthorizationStatus::Pending => {}
            AuthorizationStatus::Valid => continue,
            other => anyhow::bail!("unexpected authorization status for {domain}: {other:?}"),
        }

        let mut challenge = authz.challenge(ChallengeType::Http01).ok_or_else(|| {
            anyhow::anyhow!("ACME server did not offer an http-01 challenge for {domain}")
        })?;
        let token = challenge.token.clone();
        let key_authorization = challenge.key_authorization().as_str().to_string();
        challenges.lock().unwrap().insert(token.clone(), key_authorization);
        placed_tokens.push(token);
        challenge.set_ready().await?;
    }

    let status = order.poll_ready(&RetryPolicy::default()).await;

    {
        let mut store = challenges.lock().unwrap();
        for token in &placed_tokens {
            store.remove(token);
        }
    }

    match status? {
        OrderStatus::Ready => {}
        other => anyhow::bail!("order for {domain} did not become ready (status: {other:?})"),
    }

    let key_pem = order.finalize().await?;
    let cert_pem = order
        .poll_certificate(&RetryPolicy::default())
        .await?;

    let cert_dir = paths.domain_cert_dir(domain);
    std::fs::create_dir_all(&cert_dir)?;
    std::fs::write(cert_dir.join("cert.pem"), &cert_pem)?;
    std::fs::write(cert_dir.join("key.pem"), &key_pem)?;
    let meta = CertMeta {
        issued_at_unix: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
    };
    std::fs::write(cert_dir.join("acme-meta.json"), serde_json::to_string(&meta)?)?;

    let cert_chain = crate::tls::parse_cert_pem(&cert_pem)?;
    let key = crate::tls::parse_key_pem(&key_pem)?;
    cert_store.insert(domain, cert_chain, key)?;

    Ok(())
}
