use std::sync::Arc;

use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use bytes::Bytes;
use http::{Method, StatusCode};
use pingora::{
    Result,
    http::ResponseHeader,
    proxy::{ProxyHttp, Session, http_proxy_service},
    server::Server,
    upstreams::peer::HttpPeer,
};

use crate::blocking::BlockPolicy;
use crate::categories::CategoryIndex;
use crate::certs::{GeneratedCerts, SniInfo};
use crate::config::{ServerConfig, Settings};
use crate::context::RuntimeContext;
use crate::controlplane::InMemControlPlane;
use crate::identity::account_from_sni;
use crate::pipeline::Pipeline;
use crate::policy::PolicyCache;
use crate::transport::DohTransport;

/// DoH endpoint that terminates TLS, identifies the account from the SNI, and
/// resolves the query through the shared pipeline (instead of transparently
/// proxying to a fixed upstream).
pub struct DohReceiver {
    port: u16,
    server: ServerConfig,
    certs: Arc<GeneratedCerts>,
    pipeline: Arc<Pipeline>,
}

impl DohReceiver {
    /// Defaults for everything except the listen port.
    pub fn new(port: u16) -> Self {
        let defaults = Settings::default();
        let ctx = Arc::new(RuntimeContext::new(
            Arc::new(InMemControlPlane::default()),
            Arc::new(CategoryIndex::new()),
            BlockPolicy::default(),
            Arc::new(PolicyCache::new()),
        ));
        Self::from_config(port, defaults.doh, &defaults.server, &defaults.certs, ctx)
            .expect("failed to load/generate certs")
    }

    /// Config struct plus the runtime context.
    pub fn from_config(
        port: u16,
        doh: crate::config::DohConfig,
        server: &ServerConfig,
        certs: &crate::config::CertsConfig,
        ctx: Arc<RuntimeContext>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let generated = crate::certs::load_or_generate(certs)?;
        let fallback = Arc::new(DohTransport::from_config(&doh));
        let pipeline = ctx.pipeline(fallback, "doh");
        Ok(Self {
            port,
            server: server.clone(),
            certs: Arc::new(generated),
            pipeline,
        })
    }

    /// Port this receiver is configured to listen on.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Resolve the account for this request from the SNI captured during the
    /// TLS handshake, falling back when no usable name was supplied.
    fn account_for(&self, session: &Session) -> String {
        let sni = session
            .downstream_session
            .digest()
            .and_then(|d| d.ssl_digest.as_ref())
            .and_then(|s| s.extension.get::<SniInfo>())
            .map(|info| info.sni.as_str());

        match sni.and_then(|name| account_from_sni(name, &self.server.domain)) {
            Some(account) => account,
            None => {
                // Never echo the raw SNI: it may carry arbitrary hostnames.
                log::warn!(
                    "no usable account in SNI; using fallback={}",
                    self.server.fallback_account
                );
                self.server.fallback_account.clone()
            }
        }
    }
}

#[async_trait]
impl ProxyHttp for DohReceiver {
    type CTX = ();
    fn new_ctx(&self) {}

    async fn request_filter(&self, session: &mut Session, _ctx: &mut Self::CTX) -> Result<bool> {
        let account = self.account_for(session);

        // Read the DNS query (GET ?dns= or POST body), then run the shared
        // pipeline and hand back the result. Any failure becomes an HTTP error.
        let query = match extract_query(session).await {
            Ok(Some(q)) => q,
            Ok(None) => {
                let _ = session
                    .respond_error_with_body(400, Bytes::from_static(b"missing dns query"))
                    .await;
                return Ok(true);
            }
            Err(e) => {
                log::warn!("failed to read DoH query: {e}");
                let _ = session
                    .respond_error_with_body(400, Bytes::from_static(b"bad dns query"))
                    .await;
                return Ok(true);
            }
        };

        let response = match self.pipeline.handle_query(&account, &query).await {
            Ok(r) => r,
            Err(e) => {
                log::warn!("query pipeline failed: {e}");
                let _ = session.respond_error(502).await;
                return Ok(true);
            }
        };

        let mut header = ResponseHeader::build(StatusCode::OK, None).map_err(|e| {
            pingora::Error::explain(pingora::ErrorType::InternalError, format!("{e}"))
        })?;
        header
            .insert_header("Content-Type", "application/dns-message")
            .map_err(|e| {
                pingora::Error::explain(pingora::ErrorType::InternalError, format!("{e}"))
            })?;
        header
            .insert_header("Content-Length", response.len().to_string())
            .map_err(|e| {
                pingora::Error::explain(pingora::ErrorType::InternalError, format!("{e}"))
            })?;
        session
            .write_response_header(Box::new(header), false)
            .await?;
        session
            .write_response_body(Some(Bytes::from(response)), true)
            .await?;

        // true: response fully handled, stop the proxy from forwarding.
        Ok(true)
    }

    async fn upstream_peer(&self, _session: &mut Session, _ctx: &mut ()) -> Result<Box<HttpPeer>> {
        // Unreachable: request_filter always short-circuits.
        Err(pingora::Error::explain(
            pingora::ErrorType::InternalError,
            "DoH handled in request_filter",
        ))
    }
}

#[async_trait]
impl super::Receiver for DohReceiver {
    async fn run(self: Box<Self>) {
        // pingora's run_forever blocks its thread and manages its own runtimes,
        // so it has to live on a blocking thread to stay awaitable.
        let port = self.port;
        let certs = Arc::clone(&self.certs);
        tokio::task::spawn_blocking(move || {
            let mut tls_settings = certs
                .tls_settings_with_sni()
                .expect("failed to build SNI-capturing TLS settings");
            tls_settings.enable_h2();

            let mut my_server = Server::new(None).unwrap();
            my_server.bootstrap();

            let listen_addr = format!("0.0.0.0:{port}");
            let mut proxy = http_proxy_service(&my_server.configuration, *self);
            proxy.add_tls_with_settings(&listen_addr, None, tls_settings);

            my_server.add_service(proxy);
            my_server.run_forever();
        })
        .await
        .unwrap();
    }
}

/// Pull the DNS query out of a DoH request.
///
/// GET has base64url-encoded in the `?dns=` query param
/// POST carries the raw message in the body.
async fn extract_query(
    session: &mut Session,
) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error + Send + Sync>> {
    let method = session.req_header().method.clone();
    let uri = session.req_header().uri.clone();

    if method == Method::GET {
        let Some(param) = dns_query_param(uri.query()) else {
            return Ok(None);
        };
        let decoded = URL_SAFE_NO_PAD.decode(percent_decode(&param))?;
        return Ok(Some(decoded));
    }

    if method == Method::POST {
        let Some(body) = session.downstream_session.read_request_body().await? else {
            return Ok(None);
        };
        return Ok(Some(body.to_vec()));
    }

    Ok(None)
}

/// Find the value of the `dns` query parameter.
fn dns_query_param(query: Option<&str>) -> Option<String> {
    let query = query?;
    for pair in query.split('&') {
        if let Some(value) = pair.strip_prefix("dns=") {
            return Some(value.to_string());
        }
    }
    None
}

/// Minimal percent-decoding (enough for base64url query params).
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = &input[i + 1..i + 3];
            if let Ok(v) = u8::from_str_radix(hex, 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
