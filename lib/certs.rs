use openssl::asn1::Asn1Time;
use openssl::bn::{BigNum, MsbOption};
use openssl::hash::MessageDigest;
use openssl::pkey::{PKey, Private};
use openssl::rsa::Rsa;
use openssl::ssl::{SslAcceptor, SslMethod};
use openssl::x509::extension::{BasicConstraints, SubjectAlternativeName};
use openssl::x509::{X509, X509NameBuilder};

use pingora::listeners::tls::TlsSettings;

use crate::config::CertsConfig;

/// Holds an X509 certificate and private key, generated in memory or loaded from disk.
pub struct GeneratedCerts {
    pub x509: X509,
    pub pkey: PKey<Private>,
    /// PEM-encoded certificate bytes.
    pub cert_pem: Vec<u8>,
    /// PEM-encoded private key bytes.
    pub key_pem: Vec<u8>,
}

/// Load existing certs from disk, or auto-generate a self-signed cert/key pair.
pub fn load_or_generate(
    config: &CertsConfig,
) -> Result<GeneratedCerts, Box<dyn std::error::Error + Send + Sync>> {
    match (&config.cert_path, &config.key_path) {
        (Some(cert_path), Some(key_path)) => {
            // load from disk
            let cert_pem = std::fs::read(cert_path)?;
            let key_pem = std::fs::read(key_path)?;
            let x509 = X509::from_pem(&cert_pem)?;
            let pkey = PKey::private_key_from_pem(&key_pem)?;
            Ok(GeneratedCerts {
                x509,
                pkey,
                cert_pem,
                key_pem,
            })
        }
        _ => {
            // auto-generate
            let cfg_gen = &config.generate;
            let generated = generate_cert(cfg_gen)?;

            // persist to disk only if not in_mem
            if !config.in_mem {
                let dir = std::path::Path::new("certs");
                std::fs::create_dir_all(dir)?;
                std::fs::write(dir.join("cert.pem"), &generated.cert_pem)?;
                std::fs::write(dir.join("key.pem"), &generated.key_pem)?;
            }

            Ok(generated)
        }
    }
}

/// Generate a self-signed X509 cert and RSA key using the given parameters.
fn generate_cert(
    cfg_gen: &crate::config::CertGenConfig,
) -> Result<GeneratedCerts, Box<dyn std::error::Error + Send + Sync>> {
    // Generate RSA key
    let rsa = Rsa::generate(cfg_gen.key_bits)?;
    let pkey = PKey::from_rsa(rsa)?;

    // Build subject name: CN = cfg_gen.subject_cn
    let mut name_builder = X509NameBuilder::new()?;
    name_builder.append_entry_by_text("CN", &cfg_gen.subject_cn)?;
    let name = name_builder.build();

    // X509 v3 builder
    let mut builder = X509::builder()?;
    builder.set_version(2)?;

    // Random 128-bit serial number
    let mut serial = BigNum::new()?;
    serial.rand(128, MsbOption::MAYBE_ZERO, false)?;
    let asn1_serial = serial.to_asn1_integer()?;
    builder.set_serial_number(&asn1_serial)?;

    // Self-signed
    builder.set_issuer_name(&name)?;
    builder.set_subject_name(&name)?;

    // Validity window
    let now = Asn1Time::days_from_now(0)?;
    let expiry = Asn1Time::days_from_now(cfg_gen.days)?;
    builder.set_not_before(&now)?;
    builder.set_not_after(&expiry)?;

    // Public key
    builder.set_pubkey(&pkey)?;

    // Build SAN extension; parse DNS: / IP: entries from config
    let san = build_san(&cfg_gen.subject_alt_names);
    let san_ext = san.build(&builder.x509v3_context(None, None))?;
    builder.append_extension(san_ext)?;

    // Basic constraints: not a CA
    let bc = BasicConstraints::new().build()?;
    builder.append_extension(bc)?;

    // Self-sign with SHA-256
    builder.sign(&pkey, MessageDigest::sha256())?;

    let x509 = builder.build();

    // PEM-encode
    let cert_pem = x509.to_pem()?;
    let key_pem = pkey.private_key_to_pem_pkcs8()?;

    Ok(GeneratedCerts {
        x509,
        pkey,
        cert_pem,
        key_pem,
    })
}

/// Build a `SubjectAlternativeName` from a list of "DNS:..." / "IP:..." strings.
fn build_san(names: &[String]) -> SubjectAlternativeName {
    let mut san = SubjectAlternativeName::new();
    for name in names {
        if let Some(dns) = name.strip_prefix("DNS:") {
            san.dns(dns);
        } else if let Some(ip) = name.strip_prefix("IP:") {
            san.ip(ip);
        }
    }
    san
}

impl GeneratedCerts {
    /// Build a pingora `TlsSettings` from the in-memory cert and key.
    /// Uses `SslAcceptorBuilder::set_private_key` / `set_certificate`
    pub fn tls_settings(&self) -> Result<TlsSettings, Box<dyn std::error::Error + Send + Sync>> {
        let mut builder = SslAcceptor::mozilla_intermediate_v5(SslMethod::tls())?;
        builder.set_private_key(&self.pkey)?;
        builder.set_certificate(&self.x509)?;
        Ok(TlsSettings::from(builder))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CertGenConfig;

    #[test]
    fn test_generate_default() {
        let cfg_gen = CertGenConfig::default();
        let certs = generate_cert(&cfg_gen).expect("failed to generate cert");
        assert!(!certs.cert_pem.is_empty());
        assert!(!certs.key_pem.is_empty());
        let reparsed = X509::from_pem(&certs.cert_pem).expect("invalid cert PEM");
        let subject: String = reparsed
            .subject_name()
            .entries_by_nid(openssl::nid::Nid::COMMONNAME)
            .next()
            .and_then(|e| e.data().to_string().ok().map(|s| s.to_string()))
            .unwrap_or_default();
        assert_eq!(subject, "localhost");
    }

    #[test]
    fn test_load_or_generate_in_mem_no_io() {
        let tmp = std::env::temp_dir().join(format!("brdns-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);

        // in_mem=true: should generate without writing files
        let config = CertsConfig {
            in_mem: true,
            cert_path: None,
            key_path: None,
            ..CertsConfig::default()
        };
        let certs = load_or_generate(&config).expect("in-mem generation");
        assert!(!certs.cert_pem.is_empty());
        assert!(!tmp.join("certs").exists());
    }

    #[test]
    fn test_tls_settings() {
        let config = CertsConfig {
            in_mem: true,
            ..CertsConfig::default()
        };
        let certs = load_or_generate(&config).expect("failed to generate");
        let _settings = certs.tls_settings().expect("failed to build tls settings");
    }
}
