//! Shared runtime context handed to the DNS receivers.
//!
//! Bundles the policy inputs every query needs (control plane, category index,
//! block policy, poll-refreshed cache) so receiver constructors don't grow an
//! ever-larger parameter list.

use std::sync::Arc;

use crate::blocking::BlockPolicy;
use crate::categories::CategoryIndex;
use crate::controlplane::ControlPlane;
use crate::pipeline::Pipeline;
use crate::policy::PolicyCache;
use crate::transport::Transport;
use crate::upstream::UpstreamPool;

pub struct RuntimeContext {
    pub cp: Arc<dyn ControlPlane>,
    pub categories: Arc<CategoryIndex>,
    pub policy: BlockPolicy,
    pub cache: Arc<PolicyCache>,
}

impl RuntimeContext {
    pub fn new(
        cp: Arc<dyn ControlPlane>,
        categories: Arc<CategoryIndex>,
        policy: BlockPolicy,
        cache: Arc<PolicyCache>,
    ) -> Self {
        Self {
            cp,
            categories,
            policy,
            cache,
        }
    }

    /// Build a query pipeline whose fallback (default) upstream is `fallback`.
    pub fn pipeline(&self, fallback: Arc<dyn Transport>, protocol: &'static str) -> Arc<Pipeline> {
        let upstreams = Arc::new(UpstreamPool::new(fallback));
        Arc::new(Pipeline::new(
            Arc::clone(&self.cp),
            Arc::clone(&self.categories),
            upstreams,
            self.policy.clone(),
            Arc::clone(&self.cache),
            protocol,
        ))
    }
}
