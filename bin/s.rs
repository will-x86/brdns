use brdns::receiver::{DohReceiver, DotReceiver, Receiver};
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
    Dot,
    Doh,
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args = Args::parse();
    let settings = brdns::config::load();

    let receiver: Box<dyn Receiver> = match args.receiver_type {
        ReceiverType::Doh => Box::new(DohReceiver::from_config(
            settings.doh.listen_port,
            settings.doh,
        )),
        ReceiverType::Dot => Box::new(DotReceiver::from_config(
            settings.dot.listen_port,
            settings.dot,
            &settings.certs,
        )?),
    };
    receiver.run().await;
    Ok(())
}
