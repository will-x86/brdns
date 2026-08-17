//! Block/limit response synthesis.
//!
//! `deny` rules and over-quota `limit` rules answer with a synthesized DNS
//! response. The mode is configurable:
//!
//! - `nxdomain`: rcode NXDOMAIN, no answers (default).
//! - `refused`:  rcode REFUSED.
//! - `null`:     NOERROR + `0.0.0.0` (A) / `::` (AAAA).
//! - `custom`:   NOERROR + a configurable A/AAAA address (block page).

use std::net::{Ipv4Addr, Ipv6Addr};

use crate::buffer::BytePacketBuffer;
use crate::config::{BlockResponse, PolicyConfig};
use crate::protocol::header::ResultCode;
use crate::protocol::packet::DnsPacket;
use crate::protocol::record::{DnsRecord, QueryType};

#[derive(Debug, Clone)]
pub struct BlockPolicy {
    pub response: BlockResponse,
    pub ipv4: Ipv4Addr,
    pub ipv6: Ipv6Addr,
}

impl BlockPolicy {
    pub fn from_config(c: &PolicyConfig) -> Self {
        Self {
            response: c.block_response,
            ipv4: c.custom_ipv4.parse().unwrap_or(Ipv4Addr::UNSPECIFIED),
            ipv6: c.custom_ipv6.parse().unwrap_or(Ipv6Addr::UNSPECIFIED),
        }
    }
}

impl Default for BlockPolicy {
    fn default() -> Self {
        Self::from_config(&PolicyConfig::default())
    }
}

/// Synthesize the block/limit response for a raw query.
pub fn synthesize(
    raw_query: &[u8],
    policy: &BlockPolicy,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let mut buf = BytePacketBuffer::from_bytes(raw_query);
    let mut packet = DnsPacket::from_buffer(&mut buf)
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?;

    let qname = packet
        .questions
        .first()
        .map(|q| q.name.clone())
        .unwrap_or_default();
    let qtype = packet.questions.first().map(|q| q.qtype);

    packet.header.response = true;
    packet.answers.clear();
    packet.authorities.clear();
    packet.resources.clear();

    match policy.response {
        BlockResponse::Nxdomain => packet.header.rescode = ResultCode::NXDOMAIN,
        BlockResponse::Refused => packet.header.rescode = ResultCode::REFUSED,
        BlockResponse::Null | BlockResponse::Custom => {
            packet.header.rescode = ResultCode::NOERROR;
            let custom = policy.response == BlockResponse::Custom;
            let ttl = 60u32;
            match qtype {
                Some(QueryType::A) => {
                    let addr = if custom {
                        policy.ipv4
                    } else {
                        Ipv4Addr::UNSPECIFIED
                    };
                    packet.answers.push(DnsRecord::A {
                        domain: qname,
                        addr,
                        ttl,
                    });
                }
                Some(QueryType::AAAA) => {
                    let addr = if custom {
                        policy.ipv6
                    } else {
                        Ipv6Addr::UNSPECIFIED
                    };
                    packet.answers.push(DnsRecord::AAAA {
                        domain: qname,
                        addr,
                        ttl,
                    });
                }
                // Other qtypes get a NOERROR response with no answers.
                _ => {}
            }
        }
    }

    let mut out = BytePacketBuffer::new();
    packet
        .write(&mut out)
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?;
    Ok(out.as_bytes().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_query(name: &str) -> Vec<u8> {
        let mut packet = DnsPacket::new();
        packet.header.id = 0x1234;
        packet.header.recursion_desired = true;
        packet
            .questions
            .push(crate::protocol::question::DnsQuestion::new(
                name.into(),
                QueryType::A,
            ));
        let mut buf = BytePacketBuffer::new();
        packet.write(&mut buf).unwrap();
        buf.as_bytes().to_vec()
    }

    fn parse(data: &[u8]) -> DnsPacket {
        let mut buf = BytePacketBuffer::from_bytes(data);
        DnsPacket::from_buffer(&mut buf).unwrap()
    }

    #[test]
    fn nxdomain() {
        let policy = BlockPolicy {
            response: BlockResponse::Nxdomain,
            ..BlockPolicy::default()
        };
        let resp = synthesize(&a_query("example.com"), &policy).unwrap();
        let p = parse(&resp);
        assert_eq!(p.header.rescode, ResultCode::NXDOMAIN);
        assert!(p.answers.is_empty());
    }

    #[test]
    fn null_returns_zero_addr() {
        let policy = BlockPolicy {
            response: BlockResponse::Null,
            ..BlockPolicy::default()
        };
        let resp = synthesize(&a_query("example.com"), &policy).unwrap();
        let p = parse(&resp);
        assert_eq!(p.header.rescode, ResultCode::NOERROR);
        assert_eq!(p.answers.len(), 1);
        assert_eq!(
            p.answers[0],
            DnsRecord::A {
                domain: "example.com".into(),
                addr: Ipv4Addr::UNSPECIFIED,
                ttl: 60,
            }
        );
    }

    #[test]
    fn custom_returns_configured_addr() {
        let policy = BlockPolicy {
            response: BlockResponse::Custom,
            ipv4: "1.2.3.4".parse().unwrap(),
            ..BlockPolicy::default()
        };
        let resp = synthesize(&a_query("example.com"), &policy).unwrap();
        let p = parse(&resp);
        assert_eq!(p.header.rescode, ResultCode::NOERROR);
        assert_eq!(
            p.answers[0],
            DnsRecord::A {
                domain: "example.com".into(),
                addr: "1.2.3.4".parse().unwrap(),
                ttl: 60,
            }
        );
    }
}
