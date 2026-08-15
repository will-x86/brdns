use brdns::receiver::{DohReceiver, DotReceiver, Receiver, DEFAULT_DOH_PORT, DEFAULT_DOT_PORT};
use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Transport mechanism
    #[arg(value_enum)]
    receiver_type: ReceiverType,
}
#[derive(clap::ValueEnum, Debug, Clone)]
enum ReceiverType {
    //UDP,
    DOT,
    DOH,
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args = Args::parse();

    let receiver: Box<dyn Receiver> = match args.receiver_type {
        ReceiverType::DOH => Box::new(DohReceiver::new(DEFAULT_DOH_PORT)),
        ReceiverType::DOT => Box::new(DotReceiver::new(DEFAULT_DOT_PORT)),
    };
    receiver.run().await;
    todo!();
}
