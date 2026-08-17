#![cfg(feature = "fuzz")]

use brdns::buffer::BytePacketBuffer;
use brdns::protocol::header::DnsHeader;
use brdns::protocol::packet::DnsPacket;
use brdns::protocol::question::DnsQuestion;
use brdns::protocol::record::{DnsRecord, QueryType};

use fuzztest::domains::arbitrary::Arbitrary;
use fuzztest::domains::containers::VecOf;
use fuzztest::fuzztest;

fn bytes_domain() -> VecOf<Arbitrary<u8>> {
    VecOf::new(Arbitrary::<u8>::default())
}

#[fuzztest(data = bytes_domain())]
fn buffer(data: Vec<u8>) {
    let mut b = BytePacketBuffer::from_bytes(&data);
    let _ = b.read();
    let _ = b.read_u16();
    let _ = b.read_u32();
    let mut name = String::new();
    let _ = b.read_qname(&mut name);
}

#[fuzztest(data = bytes_domain())]
fn header(data: Vec<u8>) {
    let mut b = BytePacketBuffer::from_bytes(&data);
    let mut h = DnsHeader::new();
    let _ = h.read(&mut b);
}

#[fuzztest(data = bytes_domain())]
fn question(data: Vec<u8>) {
    let mut b = BytePacketBuffer::from_bytes(&data);
    let mut q = DnsQuestion::new(String::new(), QueryType::UNKNOWN(0));
    let _ = q.read(&mut b);
}

#[fuzztest(data = bytes_domain())]
fn record(data: Vec<u8>) {
    let mut b = BytePacketBuffer::from_bytes(&data);
    let _ = DnsRecord::read(&mut b);
}

#[fuzztest(data = bytes_domain())]
fn packet(data: Vec<u8>) {
    let mut b = BytePacketBuffer::from_bytes(&data);
    let _ = DnsPacket::from_buffer(&mut b);
}
