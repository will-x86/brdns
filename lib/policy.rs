//! Poll-refreshed policy cache.
//!
//! The DNS hot path must not hit Postgres per query. A background poll (see
//! `bin/s.rs`) refreshes this in-memory snapshot of every account's rules and
//! upstream; queries read only from here.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::controlplane::ControlPlane;
use crate::model::AccountPolicy;

pub struct PolicyCache {
    inner: RwLock<HashMap<String, Arc<AccountPolicy>>>,
}

impl Default for PolicyCache {
    fn default() -> Self {
        Self::new()
    }
}

impl PolicyCache {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
        }
    }

    /// Reload from the control plane. Returns the number of accounts cached.
    pub async fn refresh(
        &self,
        cp: &dyn ControlPlane,
    ) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
        let snapshot = cp.snapshot().await?;
        let n = snapshot.len();
        let mut guard = self.inner.write().expect("policy cache poisoned");
        *guard = snapshot
            .into_iter()
            .map(|(number, policy)| (number, Arc::new(policy)))
            .collect();
        Ok(n)
    }

    /// The cached policy for an account, if present.
    pub fn get(&self, account: &str) -> Option<Arc<AccountPolicy>> {
        self.inner
            .read()
            .expect("policy cache poisoned")
            .get(account)
            .cloned()
    }

    pub fn len(&self) -> usize {
        self.inner.read().expect("poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controlplane::NoopControlPlane;
    use crate::model::{Action, NewRule, TargetType};

    #[tokio::test]
    async fn refresh_and_get() {
        let cp = NoopControlPlane::default();
        cp.create_account("acct").await.unwrap();
        cp.replace_rules(
            "acct",
            &[NewRule {
                action: Action::Deny,
                target_type: TargetType::Domain,
                target_value: "x.com".into(),
                limit_count: None,
                limit_window: None,
                enabled: true,
            }],
        )
        .await
        .unwrap();

        let cache = PolicyCache::new();
        assert!(cache.get("acct").is_none());

        let n = cache.refresh(&cp).await.unwrap();
        assert_eq!(n, 1);

        let policy = cache.get("acct").unwrap();
        assert_eq!(policy.rules.len(), 1);
        assert_eq!(policy.rules[0].action, Action::Deny);
    }
}
