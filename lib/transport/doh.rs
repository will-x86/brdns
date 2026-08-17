use super::Transport;
use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

pub struct DohTransport {
    url: String,
    client: reqwest::Client,
}

impl DohTransport {
    /// Build a DoH transport to `host` (used for SNI/URL), optionally pinning
    /// the connection to `addr` (`IP:port`) so upstream presets can bypass
    /// DNS and connect to a known resolver IP.
    pub fn new(host: &str, port: u16, addr: Option<&str>) -> Self {
        let url = format!("https://{host}:{port}/dns-query");
        let mut builder = reqwest::Client::builder();
        if let Some(addr) = addr.and_then(|a| a.parse::<std::net::SocketAddr>().ok()) {
            builder = builder.resolve(host, addr);
        }
        Self {
            url,
            client: builder.build().expect("failed to build reqwest client"),
        }
    }

    pub fn from_config(c: &crate::config::DohConfig) -> Self {
        Self::new(&c.upstream_host, 443, Some(&c.upstream_addr))
    }
}

#[async_trait]
impl Transport for DohTransport {
    async fn send_recv(
        &self,
        data: &[u8],
    ) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        let encoded = URL_SAFE_NO_PAD.encode(data);

        let resp = self
            .client
            .get(&self.url)
            .query(&[("dns", &encoded)])
            .header("Accept", "application/dns-message")
            .send()
            .await?;

        let bytes = resp.bytes().await?;
        Ok(bytes.to_vec())
    }

    fn name(&self) -> &'static str {
        "DoH"
    }
}
