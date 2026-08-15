use brdns::buffer::BytePacketBuffer;
use brdns::config;
use brdns::protocol::packet::DnsPacket;
use brdns::protocol::question::DnsQuestion;
use brdns::protocol::record::QueryType;
use brdns::transport::{DohTransport, DotTransport, Transport, UdpTransport};
use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Transport mechanism
    #[arg(value_enum)]
    transport_type: TransportType,
    /// Domain to hit
    #[arg(short, help = "Pass `-d` and you'll see me!")]
    domain: String,
}
#[derive(clap::ValueEnum, Debug, Clone)]
enum TransportType {
    Udp,
    Dot,
    Doh,
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args = Args::parse();
    let settings = config::load();

    // Build query
    let mut packet = DnsPacket::new();
    packet.header.id = 6666;
    packet.header.recursion_desired = true;
    packet
        .questions
        .push(DnsQuestion::new(args.domain.to_string(), QueryType::A));

    let mut req_buffer = BytePacketBuffer::new();
    packet.write(&mut req_buffer).map_err(|e| e.to_string())?;
    let query_bytes = req_buffer.as_bytes();

    // Select transport
    let transport: Box<dyn Transport> = match args.transport_type {
        TransportType::Udp => Box::new(UdpTransport::from_config(&settings.udp)),
        TransportType::Dot => Box::new(DotTransport::from_config(&settings.dot)?),
        TransportType::Doh => Box::new(DohTransport::from_config(&settings.doh)),
    };
    println!("Querying {} via {}", args.domain, transport.name());

    // Send and receive
    let resp_bytes = transport.send_recv(query_bytes).await?;

    // Parse response
    let mut res_buffer = BytePacketBuffer::from_bytes(&resp_bytes);
    let res_packet = DnsPacket::from_buffer(&mut res_buffer).map_err(|e| e.to_string())?;

    println!("\n{:#?}", res_packet.header);

    for q in &res_packet.questions {
        println!("  Question: {:?}", q);
    }
    for rec in &res_packet.answers {
        println!("  Answer:   {:?}", rec);
    }

    Ok(())
}
