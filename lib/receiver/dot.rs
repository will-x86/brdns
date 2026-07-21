use std::sync::Arc;

use async_trait::async_trait;
use pingora::apps::ServerApp;
use pingora::protocols::Stream;
use pingora::server::{Server, ShutdownWatch};
use pingora::services::listening::Service;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::transport::{DotTransport, Transport};

// We terminate TLS with our own cert (pingora does the handshake via add_tls),
// so the DNS payload is plaintext in `process_new` and can be inspected before
// being forwarded upstream over our own DoT connection to Cloudflare.
const CERT_PATH: &str = "certs/cert.pem";
const KEY_PATH: &str = "certs/key.pem";
const LISTEN_ADDR: &str = "0.0.0.0:8853";

// Upstream resolver we forward decrypted queries to.
const UPSTREAM_HOST: &str = "cloudflare-dns.com";

/// pingora app that speaks the DoT wire protocol on an already-TLS-terminated
/// stream: a 2-byte big-endian length prefix followed by the DNS message
/// (RFC 7858 §3.3).
struct DotApp {
    upstream: Arc<DotTransport>,
}

#[async_trait]
impl ServerApp for DotApp {
    async fn process_new(
        self: &Arc<Self>,
        mut session: Stream,
        _shutdown: &ShutdownWatch,
    ) -> Option<Stream> {
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

            // `query` is the decrypted DNS message — inspect or rewrite it here.
            let response = match self.upstream.send_recv(&query).await {
                Ok(r) => r,
                Err(e) => {
                    log::warn!("upstream query failed: {e}");
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

pub struct DotReceiver;

impl DotReceiver {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl super::Receiver for DotReceiver {
    async fn run(self: Box<Self>) {
        // pingora's run_forever blocks its thread and manages its own runtimes,
        // so it has to live on a blocking thread to stay awaitable.
        tokio::task::spawn_blocking(move || {
            let upstream = Arc::new(
                DotTransport::new(UPSTREAM_HOST, None)
                    .expect("failed to build upstream DoT transport"),
            );

            let mut my_server = Server::new(None).unwrap();
            my_server.bootstrap();

            let mut service = Service::new("dot".to_owned(), DotApp { upstream });
            service
                .add_tls(LISTEN_ADDR, CERT_PATH, KEY_PATH)
                .expect("failed to load DoT server certificate/key");

            my_server.add_service(service);
            my_server.run_forever();
        })
        .await
        .unwrap();
    }
}
