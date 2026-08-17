//! In-memory [`ControlPlane`] implementation

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use async_trait::async_trait;

use crate::controlplane::{ControlPlane, CplResult, now, preset_upstreams_default};
use crate::model::{Account, AccountPolicy, NewRule, NewUpstream, Rule, Upstream, Window};

pub struct InMemControlPlane {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    accounts: HashMap<String, AccountRecord>,
    presets: Vec<Upstream>,
    /// (rule_id, window_start) to query count.
    quota: HashMap<(i64, i64), i64>,
    /// domain to categories.
    categories: HashMap<String, HashSet<String>>,
    last_ingestion: Option<i64>,
    next_account_id: i64,
    next_rule_id: i64,
    next_upstream_id: i64,
}

struct AccountRecord {
    account: Account,
    rules: Vec<Rule>,
    upstreams: Vec<Upstream>,
}

impl Default for InMemControlPlane {
    fn default() -> Self {
        let mut inner = Inner::default();
        for (i, preset) in preset_upstreams_default().into_iter().enumerate() {
            inner.next_upstream_id = i as i64 + 1;
            inner.presets.push(Upstream {
                id: inner.next_upstream_id,
                account_id: None,
                name: preset.name,
                protocol: preset.protocol,
                host: preset.host,
                port: preset.port,
                addr: preset.addr,
                is_preset: true,
            });
        }
        Self {
            inner: Mutex::new(inner),
        }
    }
}

impl InMemControlPlane {
    fn get_or_create(&self, account_number: &str) -> Account {
        let mut inner = self.inner.lock().expect("poisoned");
        if let Some(rec) = inner.accounts.get(account_number) {
            return rec.account.clone();
        }
        inner.next_account_id += 1;
        let account = Account {
            id: inner.next_account_id,
            account_number: account_number.to_string(),
        };
        inner.accounts.insert(
            account_number.to_string(),
            AccountRecord {
                account: account.clone(),
                rules: Vec::new(),
                upstreams: Vec::new(),
            },
        );
        account
    }
}

#[async_trait]
impl ControlPlane for InMemControlPlane {
    async fn rules(&self, account_number: &str) -> CplResult<Vec<Rule>> {
        let inner = self.inner.lock().expect("poisoned");
        Ok(inner
            .accounts
            .get(account_number)
            .map(|r| r.rules.clone())
            .unwrap_or_default())
    }

    async fn active_upstream(&self, account_number: &str) -> CplResult<Option<Upstream>> {
        let inner = self.inner.lock().expect("poisoned");
        Ok(inner
            .accounts
            .get(account_number)
            .and_then(|r| r.upstreams.first())
            .cloned())
    }

    async fn record_quota(
        &self,
        _account_number: &str,
        rule_id: i64,
        limit_count: i64,
        window: Window,
    ) -> CplResult<bool> {
        if limit_count <= 0 {
            return Ok(false);
        }
        let window_start = crate::quota::window_start(now(), window);
        let mut inner = self.inner.lock().expect("poisoned");
        let count = inner.quota.entry((rule_id, window_start)).or_insert(0);
        if *count >= limit_count {
            return Ok(false);
        }
        *count += 1;
        Ok(true)
    }

    async fn create_account(&self, account_number: &str) -> CplResult<Account> {
        Ok(self.get_or_create(account_number))
    }

    async fn get_account(&self, account_number: &str) -> CplResult<Option<Account>> {
        let inner = self.inner.lock().expect("poisoned");
        Ok(inner
            .accounts
            .get(account_number)
            .map(|r| r.account.clone()))
    }

    async fn replace_rules(&self, account_number: &str, rules: &[NewRule]) -> CplResult<Vec<Rule>> {
        let account = self.get_or_create(account_number);
        let mut inner = self.inner.lock().expect("poisoned");
        let mut out = Vec::with_capacity(rules.len());
        for (position, new) in rules.iter().enumerate() {
            inner.next_rule_id += 1;
            let rule = Rule {
                id: inner.next_rule_id,
                account_id: account.id,
                position: position as i32,
                action: new.action,
                target_type: new.target_type,
                target_value: new.target_value.clone(),
                limit_count: new.limit_count,
                limit_window: new.limit_window,
                enabled: new.enabled,
            };
            out.push(rule.clone());
        }
        inner
            .accounts
            .get_mut(account_number)
            .expect("just created")
            .rules = out.clone();
        Ok(out)
    }

    async fn list_upstreams(&self, account_number: &str) -> CplResult<Vec<Upstream>> {
        let inner = self.inner.lock().expect("poisoned");
        Ok(inner
            .accounts
            .get(account_number)
            .map(|r| r.upstreams.clone())
            .unwrap_or_default())
    }

    async fn replace_upstreams(
        &self,
        account_number: &str,
        upstreams: &[NewUpstream],
    ) -> CplResult<Vec<Upstream>> {
        let account = self.get_or_create(account_number);
        let mut inner = self.inner.lock().expect("poisoned");
        let mut out = Vec::with_capacity(upstreams.len());
        for new in upstreams {
            inner.next_upstream_id += 1;
            let up = Upstream {
                id: inner.next_upstream_id,
                account_id: Some(account.id),
                name: new.name.clone(),
                protocol: new.protocol,
                host: new.host.clone(),
                port: new.port,
                addr: new.addr.clone(),
                is_preset: false,
            };
            out.push(up.clone());
        }
        inner
            .accounts
            .get_mut(account_number)
            .expect("just created")
            .upstreams = out.clone();
        Ok(out)
    }

    async fn preset_upstreams(&self) -> CplResult<Vec<Upstream>> {
        let inner = self.inner.lock().expect("poisoned");
        Ok(inner.presets.clone())
    }

    async fn categories(&self) -> CplResult<HashMap<String, HashSet<String>>> {
        Ok(self.inner.lock().expect("poisoned").categories.clone())
    }

    async fn replace_categories(
        &self,
        categories: &HashMap<String, HashSet<String>>,
    ) -> CplResult<()> {
        let mut inner = self.inner.lock().expect("poisoned");
        inner.categories = categories.clone();
        inner.last_ingestion = Some(now());
        Ok(())
    }

    async fn last_ingestion(&self) -> CplResult<Option<i64>> {
        Ok(self.inner.lock().expect("poisoned").last_ingestion)
    }

    async fn snapshot(&self) -> CplResult<HashMap<String, AccountPolicy>> {
        let inner = self.inner.lock().expect("poisoned");
        Ok(inner
            .accounts
            .iter()
            .map(|(number, rec)| {
                (
                    number.clone(),
                    AccountPolicy {
                        rules: rec.rules.clone(),
                        upstream: rec.upstreams.first().cloned(),
                    },
                )
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Action, TargetType, UpstreamProtocol, Window};

    #[test]
    fn presets_have_unique_names() {
        let presets = crate::controlplane::preset_upstreams_default();
        let mut names: Vec<_> = presets.iter().map(|p| p.name.clone()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), presets.len());
    }

    #[tokio::test]
    async fn inmem_rules_and_upstreams() {
        let cp = InMemControlPlane::default();

        let acct = cp.create_account("1234567890").await.unwrap();
        assert_eq!(acct.account_number, "1234567890");

        let rules = vec![NewRule {
            action: Action::Deny,
            target_type: TargetType::Category,
            target_value: "ads".into(),
            limit_count: None,
            limit_window: None,
            enabled: true,
        }];
        let stored = cp.replace_rules("1234567890", &rules).await.unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].position, 0);
        assert_eq!(stored[0].action, Action::Deny);

        assert_eq!(cp.rules("1234567890").await.unwrap().len(), 1);
        assert_eq!(cp.rules("nobody").await.unwrap().len(), 0);

        let ups = vec![NewUpstream {
            name: "mine".into(),
            protocol: UpstreamProtocol::Dot,
            host: "1.2.3.4".into(),
            port: 853,
            addr: None,
        }];
        cp.replace_upstreams("1234567890", &ups).await.unwrap();
        let active = cp.active_upstream("1234567890").await.unwrap().unwrap();
        assert_eq!(active.host, "1.2.3.4");
        assert!(!active.is_preset);

        assert!(cp.active_upstream("nobody").await.unwrap().is_none());

        let presets = cp.preset_upstreams().await.unwrap();
        assert!(presets.iter().all(|p| p.is_preset));
    }

    #[tokio::test]
    async fn inmem_quota_enforced() {
        let cp = InMemControlPlane::default();
        cp.create_account("q").await.unwrap();

        let w = Window::Month;
        assert!(cp.record_quota("q", 42, 2, w).await.unwrap());
        assert!(cp.record_quota("q", 42, 2, w).await.unwrap());
        assert!(!cp.record_quota("q", 42, 2, w).await.unwrap());

        assert!(cp.record_quota("q", 43, 2, w).await.unwrap());
        assert!(!cp.record_quota("q", 44, 0, w).await.unwrap());
        assert!(!cp.record_quota("q", 45, -1, w).await.unwrap());
    }
}
