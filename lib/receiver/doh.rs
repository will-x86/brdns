use async_trait::async_trait;
use pingora::{
    Result,
    http::RequestHeader,
    proxy::{ProxyHttp, Session, http_proxy_service},
    server::Server,
    upstreams::peer::HttpPeer,
};

pub struct DohReceiver {
    port: u16,
    doh: crate::config::DohConfig,
}

#[async_trait]
impl ProxyHttp for DohReceiver {
    type CTX = ();
    fn new_ctx(&self) {}
    async fn request_filter(&self, _session: &mut Session, _ctx: &mut Self::CTX) -> Result<bool> {
        /*
        if session.req_header().uri.path().starts_with("/login")
            && !check_login(session.req_header())
        {
            let _ = session
                .respond_error_with_body(403, Bytes::from_static(b"no way!"))
                .await;
            // true: early return as the response is already written
            return Ok(true);
        }*/
        Ok(false)
    }

    async fn upstream_peer(&self, _session: &mut Session, _ctx: &mut ()) -> Result<Box<HttpPeer>> {
        let addr: std::net::SocketAddr = self
            .doh
            .upstream_addr
            .parse()
            .expect("invalid upstream_addr");
        let peer = Box::new(HttpPeer::new(addr, true, self.doh.upstream_host.clone()));
        Ok(peer)
    }

    async fn upstream_request_filter(
        &self,
        _session: &mut Session,
        upstream_request: &mut RequestHeader,
        _ctx: &mut Self::CTX,
    ) -> Result<()> {
        // The upstream expects its own hostname in the Host header, not ours.
        upstream_request
            .insert_header("Host", &self.doh.upstream_host)
            .unwrap();
        Ok(())
    }
}

#[async_trait]
impl super::Receiver for DohReceiver {
    async fn run(self: Box<Self>) {
        // pingora's run_forever blocks its thread and manages its own runtimes,
        // so it has to live on a blocking thread to stay awaitable.
        tokio::task::spawn_blocking(move || {
            let mut my_server = Server::new(None).unwrap();
            my_server.bootstrap();

            let listen_addr = format!("0.0.0.0:{}", self.port);
            let mut proxy = http_proxy_service(&my_server.configuration, *self);
            proxy.add_tcp(&listen_addr);

            my_server.add_service(proxy);
            my_server.run_forever();
        })
        .await
        .unwrap();
    }
}

impl DohReceiver {
    /// Convenience: defaults for everything except the listen port.
    pub fn new(port: u16) -> Self {
        let defaults = crate::config::Settings::default();
        Self::from_config(port, defaults.doh)
    }

    /// Full control via config struct.
    pub fn from_config(port: u16, doh: crate::config::DohConfig) -> Self {
        Self { port, doh }
    }

    /// Port this receiver is configured to listen on.
    pub fn port(&self) -> u16 {
        self.port
    }
}
