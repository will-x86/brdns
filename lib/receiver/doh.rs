use async_trait::async_trait;
use pingora::{
    Result,
    http::RequestHeader,
    proxy::{ProxyHttp, Session, http_proxy_service},
    server::Server,
    upstreams::peer::HttpPeer,
};
/*
 * Do we implement the dns over https spec here ?
 * Aka /dns-query?dns=... on applcations/dns-message
 * + post req's
 * + json spec ...
 */
/// Cloudflare DoH endpoint.
const UPSTREAM_HOST: &str = "cloudflare-dns.com";
const UPSTREAM_ADDR: &str = "1.1.1.1:443";

/// Default port for DoH
pub const DEFAULT_DOH_PORT: u16 = 6188;

pub struct DohReceiver {
    addr: std::net::SocketAddr,
    port: u16,
}

#[async_trait]
impl ProxyHttp for DohReceiver {
    type CTX = ();
    fn new_ctx(&self) -> () {
        ()
    }
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
        // UPSTREAM_HOST is the SNI here
        let peer = Box::new(HttpPeer::new(self.addr, true, UPSTREAM_HOST.to_owned()));
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
            .insert_header("Host", UPSTREAM_HOST)
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
    pub fn new(port: u16) -> Self {
        Self {
            addr: UPSTREAM_ADDR.parse().unwrap(),
            port,
        }
    }

    /// Port this receiver is configured to listen on.
    pub fn port(&self) -> u16 {
        self.port
    }
}
