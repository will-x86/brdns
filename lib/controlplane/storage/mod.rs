//! Storage backends for the control plane.

mod inmem;
mod postgres;

pub use inmem::InMemControlPlane;
pub use postgres::PostgresControlPlane;
