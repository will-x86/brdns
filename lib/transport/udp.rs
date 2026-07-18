use super::Transport;
use async_trait::async_trait;
use std::net::UdpSocket;

pub struct UdpTransport {
    server: String,
    port: u16,
}

impl UdpTransport {
    pub fn new(server: impl Into<String>, port: u16) -> Self {
        Self {
            server: server.into(),
            port,
        }
    }
}

#[async_trait]
impl Transport for UdpTransport {
    async fn send_recv(
        &self,
        data: &[u8],
    ) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        // Run blocking UDP in a spawn_blocking task
        let server = self.server.clone();
        let port = self.port;
        let data = data.to_vec();

        tokio::task::spawn_blocking(move || {
            let socket = UdpSocket::bind(("0.0.0.0", 0))?;
            let addr = format!("{}:{}", server, port);
            socket.send_to(&data, &addr)?;

            let mut buf = vec![0u8; 4096];
            let (len, _) = socket.recv_from(&mut buf)?;
            buf.truncate(len);

            Ok(buf)
        })
        .await?
    }

    fn name(&self) -> &'static str {
        "UDP"
    }
}
