use config::{Config, Environment, File};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct Settings {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub dot: DotConfig,
    #[serde(default)]
    pub doh: DohConfig,
    #[serde(default)]
    pub control_plane: ControlPlaneConfig,
    #[serde(default)]
    pub blocklist: BlocklistConfig,
    #[serde(default)]
    pub policy: PolicyConfig,
    #[serde(default)]
    pub observability: ObservabilityConfig,
    #[serde(default)]
    pub udp: UdpConfig,
    #[serde(default)]
    pub certs: CertsConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ServerConfig {
    /// Base domain used for SNI-based identity.
    /// An account is the leftmost DNS label
    pub domain: String,
    /// Account to use when a query has no (or unrecognized) SNI
    pub fallback_account: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            domain: "dns.example.com".into(),
            fallback_account: "default".into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct DotConfig {
    pub listen_port: u16,
    pub upstream_host: String,
    pub upstream_port: u16,
}

impl Default for DotConfig {
    fn default() -> Self {
        Self {
            listen_port: 8853,
            upstream_host: "1.1.1.1".into(),
            upstream_port: 853,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct DohConfig {
    pub listen_port: u16,
    pub upstream_host: String,
    pub upstream_addr: String,
}

impl Default for DohConfig {
    fn default() -> Self {
        Self {
            listen_port: 6188,
            upstream_host: "cloudflare-dns.com".into(),
            upstream_addr: "1.1.1.1:443".into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ControlPlaneConfig {
    /// Run the management API
    pub enabled: bool,
    /// Management Api addr
    pub listen_addr: String,
    /// Postgres URL -- if absent: use in mem storage
    pub database_url: Option<String>,
    /// Seconds between rules + upstreams refreshes.
    pub policy_refresh_secs: u64,
    /// Bearer token required by the management API.
    /// If None: all req's are refused.
    pub api_token: Option<String>,
}

impl Default for ControlPlaneConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            listen_addr: "127.0.0.1:8080".into(),
            database_url: None,
            policy_refresh_secs: 30,
            api_token: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct BlocklistConfig {
    /// Fetch and apply community feeds at startup.
    pub enabled: bool,
    /// Seconds between feed refreshes (used by the poll loop).
    pub refresh_interval_secs: u64,
    /// Custom feeds; empty = built-in defaults.
    pub feeds: Vec<FeedConfig>,
}

impl Default for BlocklistConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            refresh_interval_secs: 86400,
            feeds: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FeedConfig {
    pub name: String,
    pub url: String,
    /// "plain" | "hosts" | "adblock".
    pub format: String,
    pub categories: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BlockResponse {
    /// rcode NXDOMAIN, no answers.
    Nxdomain,
    /// rcode REFUSED.
    Refused,
    /// NOERROR + 0.0.0.0 / :: answer.
    Null,
    /// NOERROR + custom A/AAAA answer.
    Custom,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct PolicyConfig {
    /// How `deny` rules (and over-quota `limit` rules) answer.
    pub block_response: BlockResponse,
    /// IPv4 returned for [`BlockResponse::Custom`].
    pub custom_ipv4: String,
    /// IPv6 returned for [`BlockResponse::Custom`].
    pub custom_ipv6: String,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            block_response: BlockResponse::Nxdomain,
            custom_ipv4: "0.0.0.0".into(),
            custom_ipv6: "::".into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ObservabilityConfig {
    /// Address for the Prom `/metrics` endpoint; empty disables it.
    pub metrics_addr: String,
    /// OTLP HTTP endpoint for traces (defaults to http://127.0.0.1:4318/v1/traces);
    /// `None` disables OpenTelemetry export.
    pub otel_endpoint: Option<String>,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            metrics_addr: "0.0.0.0:9091".into(),
            otel_endpoint: Some("http://127.0.0.1:4318/v1/traces".into()),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct UdpConfig {
    pub server: String,
    pub port: u16,
}

impl Default for UdpConfig {
    fn default() -> Self {
        Self {
            server: "8.8.8.8".into(),
            port: 53,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct CertsConfig {
    /// Paths to existing cert/key files. If both provided, load from disk.
    /// If None, auto-generate using `gen` parameters.
    pub cert_path: Option<PathBuf>,
    pub key_path: Option<PathBuf>,

    /// When true, generated certs are not written to disk (zero IO).
    /// Default: false.
    pub in_mem: bool,

    /// Generation parameters; used only when cert_path/key_path are None.
    #[serde(default)]
    pub generate: CertGenConfig,
}

/// Parameters for auto-generating a self-signed certificate.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct CertGenConfig {
    /// Subject common name. Default: "localhost".
    pub subject_cn: String,
    /// Subject alternative names (DNS:..., IP:...). Default: DNS:localhost, IP:127.0.0.1.
    pub subject_alt_names: Vec<String>,
    /// Validity in days. Default: 365.
    pub days: u32,
    /// RSA key size in bits. Default: 2048.
    pub key_bits: u32,
}

impl Default for CertGenConfig {
    fn default() -> Self {
        Self {
            subject_cn: "localhost".into(),
            subject_alt_names: vec!["DNS:localhost".into(), "IP:127.0.0.1".into()],
            days: 365,
            key_bits: 2048,
        }
    }
}

/// Load configuration from: compiled-in defaults -> optional `brdns.toml` -> env vars (`BRDNS_*`).
pub fn load() -> Settings {
    Config::builder()
        .add_source(Config::try_from(&Settings::default()).unwrap())
        .add_source(File::with_name("brdns").required(false))
        .add_source(Environment::with_prefix("BRDNS").separator("__"))
        .build()
        .unwrap()
        .try_deserialize()
        .unwrap()
}
