pub mod doh;
pub mod dot;

use async_trait::async_trait;
#[async_trait]
pub trait Receiver {
    async fn run(self: Box<Self>);
}
pub use doh::DohReceiver;
pub use dot::DotReceiver;
