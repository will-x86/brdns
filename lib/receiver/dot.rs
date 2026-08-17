use std::sync::Arc;

use async_trait::async_trait;
use openssl::ssl::NameType;
use pingora::apps::ServerApp;
use pingora::protocols::Stream;
use pingora::server::{Server, ShutdownWatch};
use pingora::services::listening::Service;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::blocking::BlockPolicy;
use crate::categories::CategoryIndex;
use crate::certs::GeneratedCerts;
use crate::config::{ServerConfig, Settings};
use crate::context::RuntimeContext;
use crate::controlplane::NoopControlPlane;
use crate::identity::account_from_sni;
use crate::pipeline::Pipeline;
use crate::policy::PolicyCache;
use crate::transport::DotTransport;

/// pingora app that speaks the DoT wire protocol on an already-TLS-terminated
/// stream: a 2-byte big-endian length prefix followed by the DNS message.
///
/// Identity comes from the TLS SNI: the leftmost label of the hostname the
/// client dialed is the account number.
struct DotApp {
    pipeline: Arc<Pipeline>,
    domain: String,
    fallback_account: String,
}

impl DotApp {
    /// Resolve the account for this connection from its SNI, falling back when
    /// the client supplied no usable name.
    fn account_for(&self, session: &Stream) -> String {
        let sni = session
            .get_ssl()
            .and_then(|ssl| ssl.servername(NameType::HOST_NAME));

        match sni.and_then(|name| account_from_sni(name, &self.domain)) {
            Some(account) => account,
            None => {
                // Never echo the raw SNI: it may carry arbitrary hostnames.
                log::warn!(
                    "no usable account in SNI; using fallback={}",
                    self.fallback_account
                );
                self.fallback_account.clone()
            }
        }
    }
}

#[async_trait]
impl ServerApp for DotApp {
    async fn process_new(
        self: &Arc<Self>,
        mut session: Stream,
        _shutdown: &ShutdownWatch,
    ) -> Option<Stream> {
        // SNI is fixed per TLS connection, so resolve the account once.
        let account = self.account_for(&session);

        // DoT reuses one connection for many queries, so loop until the client
        // hangs up or an error occurs.
        loop {
            let mut len_buf = [0u8; 2];
            match session.read_exact(&mut len_buf).await {
                Ok(_) => {}
                // Clean shutdown between messages: client closed the connection.
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return None,
                Err(e) => {
                    log::warn!("DoT read error: {e}");
                    return None;
                }
            }

            let query_len = u16::from_be_bytes(len_buf) as usize;
            let mut query = vec![0u8; query_len];
            if let Err(e) = session.read_exact(&mut query).await {
                log::warn!("DoT read body error: {e}");
                return None;
            }

            // `query` is the decrypted DNS message.
            let response = match self.pipeline.handle_query(&account, &query).await {
                Ok(r) => r,
                Err(e) => {
                    log::warn!("query pipeline failed: {e}");
                    return None;
                }
            };

            let resp_len = (response.len() as u16).to_be_bytes();
            if session.write_all(&resp_len).await.is_err()
                || session.write_all(&response).await.is_err()
                || session.flush().await.is_err()
            {
                return None;
            }
        }
    }
}

pub struct DotReceiver {
    port: u16,
    server: ServerConfig,
    certs: Arc<GeneratedCerts>,
    pipeline: Arc<Pipeline>,
}

impl DotReceiver {
    /// Convenience: defaults for everything except the listen port.
    pub fn new(port: u16) -> Self {
        let defaults = Settings::default();
        let ctx = Arc::new(RuntimeContext::new(
            Arc::new(NoopControlPlane::default()),
            Arc::new(CategoryIndex::new()),
            BlockPolicy::default(),
            Arc::new(PolicyCache::new()),
        ));
        Self::from_config(port, defaults.dot, &defaults.server, &defaults.certs, ctx)
            .expect("failed to load/generate certs")
    }

    /// DotReceiver from config plus the shared runtime context.
    pub fn from_config(
        port: u16,
        dot: crate::config::DotConfig,
        server: &ServerConfig,
        certs: &crate::config::CertsConfig,
        ctx: Arc<RuntimeContext>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let generated = crate::certs::load_or_generate(certs)?;
        let fallback = Arc::new(DotTransport::new(
            &dot.upstream_host,
            Some(dot.upstream_port),
        )?);
        let pipeline = ctx.pipeline(fallback, "dot");
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
}

#[async_trait]
impl super::Receiver for DotReceiver {
    async fn run(self: Box<Self>) {
        // pingora's run_forever blocks its thread and manages its own runtimes,
        // so it has to live on a blocking thread to stay awaitable.
        let port = self.port;
        let certs = Arc::clone(&self.certs);
        let domain = self.server.domain.clone();
        let fallback_account = self.server.fallback_account.clone();
        let pipeline = Arc::clone(&self.pipeline);
        tokio::task::spawn_blocking(move || {
            let mut my_server = Server::new(None).unwrap();
            my_server.bootstrap();

            let listen_addr = format!("0.0.0.0:{port}");
            let mut service = Service::new(
                "dot".to_owned(),
                DotApp {
                    pipeline,
                    domain,
                    fallback_account,
                },
            );
            let tls_settings = certs
                .tls_settings()
                .expect("failed to build TLS settings from cert/key");
            service.add_tls_with_settings(&listen_addr, None, tls_settings);

            my_server.add_service(service);
            my_server.run_forever();
        })
        .await
        .unwrap();
    }
}
