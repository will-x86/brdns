use super::Transport;
use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

pub struct DohTransport {
    url: String,
    client: reqwest::Client,
}

impl DohTransport {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            client: reqwest::Client::new(),
        }
    }

    /// Create with a well-known DoH server
    pub fn cloudflare() -> Self {
        Self::new("https://cloudflare-dns.com/dns-query")
    }

    pub fn google() -> Self {
        Self::new("https://dns.google/dns-query")
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
