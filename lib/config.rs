use config::{Config, Environment, File};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct Settings {
    #[serde(default)]
    pub dot: DotConfig,
    #[serde(default)]
    pub doh: DohConfig,
    #[serde(default)]
    pub udp: UdpConfig,
    #[serde(default)]
    pub certs: CertsConfig,
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

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct CertsConfig {
    pub cert_path: PathBuf,
    pub key_path: PathBuf,
}

impl Default for CertsConfig {
    fn default() -> Self {
        Self {
            cert_path: "certs/cert.pem".into(),
            key_path: "certs/key.pem".into(),
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
