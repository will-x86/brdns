use super::Transport;
use async_trait::async_trait;
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, RootCertStore};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

pub struct DotTransport {
    server: String,
    port: u16,
    connector: TlsConnector,
}

impl DotTransport {
    pub fn new(
        server: impl Into<String>,
        port: Option<u16>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let mut root_store = RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

        let config = ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        Ok(Self {
            server: server.into(),
            port: port.unwrap_or(853),
            connector: TlsConnector::from(Arc::new(config)),
        })
    }
}

#[async_trait]
impl Transport for DotTransport {
    async fn send_recv(
        &self,
        data: &[u8],
    ) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        let addr = format!("{}:{}", self.server, self.port);
        let tcp = TcpStream::connect(&addr).await?;

        // Use SNI with the server name - FIXED HERE
        let server_name = ServerName::try_from(self.server.clone())?;
        let tls = self.connector.connect(server_name, tcp).await?;

        let mut tls = tls;

        // DNS over TLS uses 2-byte length prefix
        let len = (data.len() as u16).to_be_bytes();
        tls.write_all(&len).await?;
        tls.write_all(data).await?;
        tls.flush().await?;

        // Read response length
        let mut len_buf = [0u8; 2];
        tls.read_exact(&mut len_buf).await?;
        let resp_len = u16::from_be_bytes(len_buf) as usize;

        // Read response
        let mut resp = vec![0u8; resp_len];
        tls.read_exact(&mut resp).await?;

        Ok(resp)
    }

    fn name(&self) -> &'static str {
        "DoT"
    }
}
