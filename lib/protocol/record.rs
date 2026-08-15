use crate::buffer::{BytePacketBuffer, Result};
use std::net::Ipv4Addr;

#[derive(PartialEq, Eq, Debug, Clone, Hash, Copy)]
pub enum QueryType {
    UNKNOWN(u16),
    A,     // 1
    AAAA,  // 28
    CNAME, // 5
    MX,    // 15
    NS,    // 2
    TXT,   // 16
}

impl QueryType {
    pub fn to_num(&self) -> u16 {
        match self {
            Self::UNKNOWN(x) => *x,
            Self::A => 1,
            Self::AAAA => 28,
            Self::CNAME => 5,
            Self::MX => 15,
            Self::NS => 2,
            Self::TXT => 16,
        }
    }

    pub fn from_num(num: u16) -> Self {
        match num {
            1 => Self::A,
            28 => Self::AAAA,
            5 => Self::CNAME,
            15 => Self::MX,
            2 => Self::NS,
            16 => Self::TXT,
            _ => Self::UNKNOWN(num),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DnsRecord {
    UNKNOWN {
        domain: String,
        qtype: u16,
        data_len: u16,
        ttl: u32,
    },
    A {
        domain: String,
        addr: Ipv4Addr,
        ttl: u32,
    },
    AAAA {
        domain: String,
        addr: std::net::Ipv6Addr,
        ttl: u32,
    },
    CNAME {
        domain: String,
        host: String,
        ttl: u32,
    },
    MX {
        domain: String,
        priority: u16,
        host: String,
        ttl: u32,
    },
    NS {
        domain: String,
        host: String,
        ttl: u32,
    },
    TXT {
        domain: String,
        data: String,
        ttl: u32,
    },
}

impl DnsRecord {
    pub fn read(buffer: &mut BytePacketBuffer) -> Result<Self> {
        let mut domain = String::new();
        buffer.read_qname(&mut domain)?;

        let qtype_num = buffer.read_u16()?;
        let qtype = QueryType::from_num(qtype_num);
        let _class = buffer.read_u16()?;
        let ttl = buffer.read_u32()?;
        let data_len = buffer.read_u16()?;

        match qtype {
            QueryType::A => {
                let raw_addr = buffer.read_u32()?;
                let addr = Ipv4Addr::new(
                    ((raw_addr >> 24) & 0xFF) as u8,
                    ((raw_addr >> 16) & 0xFF) as u8,
                    ((raw_addr >> 8) & 0xFF) as u8,
                    (raw_addr & 0xFF) as u8,
                );
                Ok(Self::A { domain, addr, ttl })
            }
            QueryType::AAAA => {
                let mut octets = [0u8; 16];
                for octet in &mut octets {
                    *octet = buffer.read()?;
                }
                Ok(Self::AAAA {
                    domain,
                    addr: std::net::Ipv6Addr::from(octets),
                    ttl,
                })
            }
            QueryType::CNAME | QueryType::NS => {
                let mut host = String::new();
                buffer.read_qname(&mut host)?;
                match qtype {
                    QueryType::CNAME => Ok(Self::CNAME { domain, host, ttl }),
                    QueryType::NS => Ok(Self::NS { domain, host, ttl }),
                    _ => unreachable!(),
                }
            }
            QueryType::MX => {
                let priority = buffer.read_u16()?;
                let mut host = String::new();
                buffer.read_qname(&mut host)?;
                Ok(Self::MX {
                    domain,
                    priority,
                    host,
                    ttl,
                })
            }
            QueryType::TXT => {
                // TXT records have length-prefixed strings
                let mut data = String::new();
                let end = buffer.pos() + data_len as usize;
                while buffer.pos() < end {
                    let len = buffer.read()? as usize;
                    for _ in 0..len {
                        data.push(buffer.read()? as char);
                    }
                }
                Ok(Self::TXT { domain, data, ttl })
            }
            QueryType::UNKNOWN(_) => {
                buffer.step(data_len as usize)?;
                Ok(Self::UNKNOWN {
                    domain,
                    qtype: qtype_num,
                    data_len,
                    ttl,
                })
            }
        }
    }

    pub fn write(&self, buffer: &mut BytePacketBuffer) -> Result<usize> {
        let start_pos = buffer.pos();

        match self {
            Self::A { domain, addr, ttl } => {
                buffer.write_qname(domain)?;
                buffer.write_u16(QueryType::A.to_num())?;
                buffer.write_u16(1)?;
                buffer.write_u32(*ttl)?;
                buffer.write_u16(4)?;
                for octet in addr.octets() {
                    buffer.write_u8(octet)?;
                }
            }
            Self::CNAME { domain, host, ttl } => {
                buffer.write_qname(domain)?;
                buffer.write_u16(QueryType::CNAME.to_num())?;
                buffer.write_u16(1)?;
                buffer.write_u32(*ttl)?;
                let pos_before = buffer.pos();
                buffer.write_u16(0)?; // placeholder for length
                buffer.write_qname(host)?;
                let len = (buffer.pos() - pos_before - 2) as u16;
                // Rewrite length
                buffer.buf[pos_before] = (len >> 8) as u8;
                buffer.buf[pos_before + 1] = (len & 0xFF) as u8;
            }
            Self::UNKNOWN { .. } => {
                // Skip unknown records
            }
            _ => {
                // TODO: Implement other record types
            }
        }

        Ok(buffer.pos() - start_pos)
    }
}
