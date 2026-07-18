pub mod doh;
pub mod dot;
pub mod udp;

use async_trait::async_trait;

/// Trait for sending/receiving raw DNS packets over different transports
#[async_trait]
pub trait Transport: Send + Sync {
    /// Send a DNS query and return the raw response bytes
    async fn send_recv(
        &self,
        data: &[u8],
    ) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>>;

    /// Human-readable name for this transport
    fn name(&self) -> &'static str;
}

pub use doh::DohTransport;
pub use dot::DotTransport;
pub use udp::UdpTransport;
