//! Pooled, deduped DNS upstream transports.
//!
//! Many accounts can share the same resolver; the pool builds one transport
//! per distinct `(protocol, host, port, addr)` tuple and reuses it, so a
//! 4-friend deployment holds a handful of connections instead of one per
//! account (or per query).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::model::{Upstream, UpstreamProtocol};
use crate::transport::{DohTransport, DotTransport, Transport, UdpTransport};

type PoolKey = (UpstreamProtocol, String, u16, Option<String>);

pub struct UpstreamPool {
    /// Transport used when an account has no custom upstream configured.
    fallback: Arc<dyn Transport>,
    cache: Mutex<HashMap<PoolKey, Arc<dyn Transport>>>,
}

impl UpstreamPool {
    pub fn new(fallback: Arc<dyn Transport>) -> Self {
        Self {
            fallback,
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Transport to use when an account has no custom upstream.
    pub fn fallback(&self) -> Arc<dyn Transport> {
        Arc::clone(&self.fallback)
    }

    /// Get (or build and cache) the transport for an upstream record.
    pub fn get(&self, upstream: &Upstream) -> Arc<dyn Transport> {
        let key = (
            upstream.protocol,
            upstream.host.clone(),
            upstream.port,
            upstream.addr.clone(),
        );
        let mut cache = self.cache.lock().expect("upstream pool poisoned");
        let transport = cache
            .entry(key)
            .or_insert_with(|| build_transport(upstream))
            .clone();
        crate::observability::set_upstream_transports(cache.len() as i64);
        transport
    }

    /// Number of distinct transports currently cached.
    pub fn len(&self) -> usize {
        self.cache.lock().expect("poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Build a transport for an upstream record.
pub fn build_transport(upstream: &Upstream) -> Arc<dyn Transport> {
    match upstream.protocol {
        UpstreamProtocol::Dot => Arc::new(
            DotTransport::new(&upstream.host, Some(upstream.port))
                .expect("failed to build DoT upstream transport"),
        ),
        UpstreamProtocol::Doh => Arc::new(DohTransport::new(
            &upstream.host,
            upstream.port,
            upstream.addr.as_deref(),
        )),
        UpstreamProtocol::Udp => Arc::new(UdpTransport::new(&upstream.host, upstream.port)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn upstream(host: &str) -> Upstream {
        Upstream {
            id: 1,
            account_id: Some(1),
            name: host.into(),
            protocol: UpstreamProtocol::Dot,
            host: host.into(),
            port: 853,
            addr: None,
            is_preset: false,
        }
    }

    #[test]
    fn dedupes_identical_upstreams() {
        let fallback = Arc::new(UdpTransport::new("8.8.8.8", 53));
        let pool = UpstreamPool::new(fallback);

        let a = pool.get(&upstream("1.1.1.1"));
        let b = pool.get(&upstream("1.1.1.1"));
        assert!(
            Arc::ptr_eq(&a, &b),
            "same upstream must share one transport"
        );
        assert_eq!(pool.len(), 1);

        let c = pool.get(&upstream("9.9.9.9"));
        assert!(!Arc::ptr_eq(&a, &c));
        assert_eq!(pool.len(), 2);
    }
}
