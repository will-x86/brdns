use std::sync::Arc;

use brdns::blocking::BlockPolicy;
use brdns::buffer::BytePacketBuffer;
use brdns::categories::CategoryIndex;
use brdns::config::Settings;
use brdns::context::RuntimeContext;
use brdns::controlplane::{ControlPlane, NoopControlPlane};
use brdns::policy::PolicyCache;
use brdns::protocol::packet::DnsPacket;
use brdns::protocol::question::DnsQuestion;
use brdns::protocol::record::QueryType;
use brdns::receiver::{DohReceiver, DotReceiver, Receiver};
use rustls::client::danger::ServerCertVerifier;
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
use tokio::net::TcpListener;
use tokio_rustls::TlsConnector;

#[path = "integration/doh/mod.rs"]
mod doh;
#[path = "integration/dot/mod.rs"]
mod dot;

async fn pick_port() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    drop(l);
    port
}

async fn wait_for_port(port: u16) {
    for _ in 0..20 {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        if tokio::net::TcpStream::connect(format!("127.0.0.1:{port}"))
            .await
            .is_ok()
        {
            return;
        }
    }
    panic!("server never became ready on port {port}");
}

async fn spawn_server(make: impl FnOnce(u16) -> Box<dyn Receiver> + Send + 'static) -> u16 {
    let port = pick_port().await;
    let ready = Arc::new(tokio::sync::Notify::new());
    let ready2 = ready.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let rx = make(port);
            ready2.notify_one();
            rx.run().await;
        });
    });
    ready.notified().await;
    port
}

fn build_query(domain: &str, qtype: QueryType) -> Vec<u8> {
    let mut packet = DnsPacket::new();
    packet.header.id = 0x4242;
    packet.header.recursion_desired = true;
    packet
        .questions
        .push(DnsQuestion::new(domain.to_string(), qtype));
    let mut buf = BytePacketBuffer::new();
    packet.write(&mut buf).unwrap();
    buf.as_bytes().to_vec()
}

fn parse_response(data: &[u8]) -> Result<DnsPacket, String> {
    let mut buf = BytePacketBuffer::from_bytes(data);
    DnsPacket::from_buffer(&mut buf).map_err(|e| e.to_string())
}

fn no_verify_tls() -> TlsConnector {
    let config = ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoVerify))
        .with_no_client_auth();
    TlsConnector::from(Arc::new(config))
}

#[derive(Debug)]
struct NoVerify;

impl ServerCertVerifier for NoVerify {
    fn verify_server_cert(
        &self,
        _: &rustls::pki_types::CertificateDer<'_>,
        _: &[rustls::pki_types::CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _: &[u8],
        _: &rustls::pki_types::CertificateDer<'_>,
        _: &DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _: &[u8],
        _: &rustls::pki_types::CertificateDer<'_>,
        _: &DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP521_SHA512,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::ED25519,
        ]
    }
}

pub struct DotServer {
    port: u16,
}

impl DotServer {
    pub async fn start(settings: Option<&Settings>) -> Self {
        Self::start_with(settings, Arc::new(NoopControlPlane::default())).await
    }

    pub async fn start_with(settings: Option<&Settings>, cp: Arc<dyn ControlPlane>) -> Self {
        Self::start_full(settings, cp, Arc::new(CategoryIndex::new())).await
    }

    pub async fn start_full(
        settings: Option<&Settings>,
        cp: Arc<dyn ControlPlane>,
        categories: Arc<CategoryIndex>,
    ) -> Self {
        let mut settings = settings.cloned().unwrap_or_default();
        settings.certs.in_mem = true; // tests use in-memory certs
        let cache = Arc::new(PolicyCache::new());
        cache.refresh(cp.as_ref()).await.unwrap();
        let ctx = Arc::new(RuntimeContext::new(
            cp,
            categories,
            BlockPolicy::from_config(&settings.policy),
            cache,
        ));
        let port = spawn_server(move |p| {
            Box::new(
                DotReceiver::from_config(p, settings.dot, &settings.server, &settings.certs, ctx)
                    .unwrap(),
            )
        })
        .await;
        wait_for_port(port).await;
        Self { port }
    }

    pub async fn query(
        &self,
        domain: &str,
        qtype: QueryType,
    ) -> Result<DnsPacket, Box<dyn std::error::Error + Send + Sync>> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let q = build_query(domain, qtype);

        let tcp = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", self.port)).await?;
        let mut tls = no_verify_tls()
            .connect(ServerName::try_from("localhost")?, tcp)
            .await?;

        let len = (q.len() as u16).to_be_bytes();
        tls.write_all(&len).await?;
        tls.write_all(&q).await?;
        tls.flush().await?;

        let mut len_buf = [0u8; 2];
        tls.read_exact(&mut len_buf).await?;
        let mut resp = vec![0u8; u16::from_be_bytes(len_buf) as usize];
        tls.read_exact(&mut resp).await?;

        Ok(parse_response(&resp)?)
    }
}

pub struct DohServer {
    port: u16,
}

impl DohServer {
    pub async fn start(settings: Option<&Settings>) -> Self {
        Self::start_with(settings, Arc::new(NoopControlPlane::default())).await
    }

    pub async fn start_with(settings: Option<&Settings>, cp: Arc<dyn ControlPlane>) -> Self {
        let mut settings = settings.cloned().unwrap_or_default();
        settings.certs.in_mem = true; // tests use in-memory certs
        let categories = Arc::new(CategoryIndex::new());
        let cache = Arc::new(PolicyCache::new());
        cache.refresh(cp.as_ref()).await.unwrap();
        let ctx = Arc::new(RuntimeContext::new(
            cp,
            categories,
            BlockPolicy::from_config(&settings.policy),
            cache,
        ));
        let port = spawn_server(move |p| {
            Box::new(
                DohReceiver::from_config(p, settings.doh, &settings.server, &settings.certs, ctx)
                    .unwrap(),
            )
        })
        .await;
        wait_for_port(port).await;
        Self { port }
    }

    pub async fn query(
        &self,
        domain: &str,
        qtype: QueryType,
    ) -> Result<DnsPacket, Box<dyn std::error::Error + Send + Sync>> {
        let q = build_query(domain, qtype);
        // DoH now serves over HTTPS; accept the self-signed test cert.
        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()?;
        let encoded = base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, &q);
        let url = format!("https://localhost:{}/dns-query", self.port);

        let bytes = client
            .get(&url)
            .query(&[("dns", &encoded)])
            .header("Accept", "application/dns-message")
            .send()
            .await?
            .bytes()
            .await?;

        Ok(parse_response(&bytes)?)
    }
}
