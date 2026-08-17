//! The query pipeline
//!
//! Both the DoT and DoH entry points call [`Pipeline::handle_query`] with the
//! caller's account and the raw DNS query bytes. The pipeline:
//!
//! 1. Parses the query name.
//! 2. Loads the account's rules from the control plane and evaluates them.
//! 3. Blocks, limits (per-window quotas), or
//!    forwards to chosen upstream.

use std::sync::Arc;

use crate::blocking::BlockPolicy;
use crate::buffer::BytePacketBuffer;
use crate::categories::CategoryIndex;
use crate::controlplane::ControlPlane;
use crate::model::{AccountPolicy, Rule};
use crate::observability::Outcome;
use crate::policy::PolicyCache;
use crate::protocol::packet::DnsPacket;
use crate::ruleengine::{Decision, evaluate};
use crate::transport::Transport;
use crate::upstream::UpstreamPool;

/// Bundles the policy inputs shared by every query.
pub struct Pipeline {
    /// Control plane, used for quota writes only - policy reads come from `cache`.
    cp: Arc<dyn ControlPlane>,
    categories: Arc<CategoryIndex>,
    upstreams: Arc<UpstreamPool>,
    policy: BlockPolicy,
    cache: Arc<PolicyCache>,
    /// "dot" or "doh" - for metric/span labels.
    protocol: &'static str,
}

impl Pipeline {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cp: Arc<dyn ControlPlane>,
        categories: Arc<CategoryIndex>,
        upstreams: Arc<UpstreamPool>,
        policy: BlockPolicy,
        cache: Arc<PolicyCache>,
        protocol: &'static str,
    ) -> Self {
        Self {
            cp,
            categories,
            upstreams,
            policy,
            cache,
            protocol,
        }
    }

    /// The transport an account should use:
    /// Cached custom upstream if set,
    /// else the default.
    fn transport_for(&self, policy: Option<&AccountPolicy>) -> Arc<dyn Transport> {
        match policy.and_then(|p| p.upstream.as_ref()) {
            Some(upstream) => self.upstreams.get(upstream),
            None => self.upstreams.fallback(),
        }
    }

    /// Resolve a single DNS query on behalf of `account`.
    ///
    /// `raw_query` is the wire-format DNS message from the client; the returned
    /// bytes are the wire-format DNS response to hand back.
    pub async fn handle_query(
        &self,
        account: &str,
        raw_query: &[u8],
    ) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        let start = std::time::Instant::now();
        let span = tracing::debug_span!("query", account = %account, protocol = %self.protocol);
        let _enter = span.enter();

        let (outcome, result) = self.handle_inner(account, raw_query).await;

        crate::observability::record_query(account, self.protocol, outcome, start.elapsed());
        if matches!(outcome, Outcome::Deny | Outcome::LimitExceeded) {
            let reason = match outcome {
                Outcome::Deny => "deny",
                _ => "limit",
            };
            crate::observability::record_blocked(account, reason);
        }
        result
    }

    async fn handle_inner(
        &self,
        account: &str,
        raw_query: &[u8],
    ) -> (
        Outcome,
        Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>>,
    ) {
        let qname = match query_name(raw_query) {
            Ok(q) => q,
            Err(e) => return (Outcome::Error, Err(e)),
        };
        let cached = self.cache.get(account);

        let rules: &[Rule] = cached.as_ref().map(|p| p.rules.as_slice()).unwrap_or(&[]);
        let decision = evaluate(rules, &qname, &|cat| self.categories.contains(&qname, cat));

        log::debug!("account={account} -> {decision:?}");

        match decision {
            Some(Decision::Deny) => (
                Outcome::Deny,
                crate::blocking::synthesize(raw_query, &self.policy),
            ),
            Some(Decision::Limit {
                rule_id,
                limit_count,
                window,
            }) => match self
                .cp
                .record_quota(account, rule_id, limit_count, window)
                .await
            {
                Ok(true) => {
                    let transport = self.transport_for(cached.as_deref());
                    (Outcome::LimitOk, transport.send_recv(raw_query).await)
                }
                Ok(false) => (
                    Outcome::LimitExceeded,
                    crate::blocking::synthesize(raw_query, &self.policy),
                ),
                Err(e) => (Outcome::Error, Err(e)),
            },
            // Allow (explicit) or no matching rule: forward.
            _ => {
                let transport = self.transport_for(cached.as_deref());
                (Outcome::Allow, transport.send_recv(raw_query).await)
            }
        }
    }
}

/// Extract the first question name from a raw DNS query.
pub fn query_name(raw_query: &[u8]) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let mut buf = BytePacketBuffer::from_bytes(raw_query);
    let packet = DnsPacket::from_buffer(&mut buf)
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?;
    Ok(packet
        .questions
        .first()
        .map(|q| q.name.clone())
        .unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controlplane::InMemControlPlane;
    use crate::model::{Action, NewRule, NewUpstream, TargetType, UpstreamProtocol, Window};
    use crate::protocol::header::ResultCode;
    use crate::transport::UdpTransport;

    fn query() -> Vec<u8> {
        vec![
            0x12, 0x34, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07, b'e',
            b'x', b'a', b'm', b'p', b'l', b'e', 0x03, b'c', b'o', b'm', 0x00, 0x00, 0x01, 0x00,
            0x01,
        ]
    }

    async fn pipeline_with(cp: Arc<dyn ControlPlane>) -> Pipeline {
        let categories = Arc::new(CategoryIndex::new());
        let upstreams = Arc::new(UpstreamPool::new(Arc::new(UdpTransport::new(
            "8.8.8.8", 53,
        ))));
        let cache = Arc::new(PolicyCache::new());
        cache.refresh(cp.as_ref()).await.unwrap();
        Pipeline::new(
            cp,
            categories,
            upstreams,
            BlockPolicy::default(),
            cache,
            "test",
        )
    }

    #[test]
    fn query_name_parses() {
        assert_eq!(query_name(&query()).unwrap(), "example.com");
    }

    #[tokio::test]
    async fn deny_synthesizes_nxdomain() {
        let cp = Arc::new(InMemControlPlane::default());
        cp.create_account("1234567890").await.unwrap();
        cp.replace_rules(
            "1234567890",
            &[NewRule {
                action: Action::Deny,
                target_type: TargetType::Domain,
                target_value: "example.com".into(),
                limit_count: None,
                limit_window: None,
                enabled: true,
            }],
        )
        .await
        .unwrap();

        let pipeline = pipeline_with(cp).await;
        let resp = pipeline.handle_query("1234567890", &query()).await.unwrap();

        let mut buf = BytePacketBuffer::from_bytes(&resp);
        let packet = DnsPacket::from_buffer(&mut buf).unwrap();
        assert!(packet.header.response);
        assert_eq!(packet.header.rescode, ResultCode::NXDOMAIN);
        assert_eq!(packet.questions.len(), 1);
        assert_eq!(packet.answers.len(), 0);
    }

    #[tokio::test]
    async fn limit_over_quota_blocks() {
        let cp = Arc::new(InMemControlPlane::default());
        cp.create_account("1234567890").await.unwrap();
        cp.replace_rules(
            "1234567890",
            &[NewRule {
                action: Action::Limit,
                target_type: TargetType::Domain,
                target_value: "example.com".into(),
                limit_count: Some(1),
                limit_window: Some(Window::Month),
                enabled: true,
            }],
        )
        .await
        .unwrap();

        // Exhaust the single-query budget without touching the network.
        let rule = cp.rules("1234567890").await.unwrap().remove(0);
        assert!(
            cp.record_quota("1234567890", rule.id, 1, Window::Month)
                .await
                .unwrap()
        );

        let pipeline = pipeline_with(cp).await;
        let resp = pipeline.handle_query("1234567890", &query()).await.unwrap();

        let mut buf = BytePacketBuffer::from_bytes(&resp);
        let packet = DnsPacket::from_buffer(&mut buf).unwrap();
        assert_eq!(packet.header.rescode, ResultCode::NXDOMAIN);
        assert!(packet.answers.is_empty());
    }

    #[tokio::test]
    async fn transport_for_uses_custom_upstream() {
        let cp = Arc::new(InMemControlPlane::default());
        cp.create_account("acct").await.unwrap();
        cp.replace_upstreams(
            "acct",
            &[NewUpstream {
                name: "custom-dot".into(),
                protocol: UpstreamProtocol::Dot,
                host: "1.2.3.4".into(),
                port: 853,
                addr: None,
            }],
        )
        .await
        .unwrap();

        let pipeline = pipeline_with(cp).await;
        let cached = pipeline.cache.get("acct");

        // Custom upstream wins.
        let t = pipeline.transport_for(cached.as_deref());
        assert_eq!(t.name(), "DoT");
        // No cached policy -> deployment default (UDP fallback here).
        let t = pipeline.transport_for(None);
        assert_eq!(t.name(), "UDP");
    }
}
