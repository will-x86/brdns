//! Postgres-backed [`ControlPlane`] via sqlx.

use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use sqlx::PgPool;

use crate::controlplane::{ControlPlane, CplResult, now, preset_upstreams_default};
use crate::model::{Account, NewRule, NewUpstream, Rule, Upstream};

pub struct PostgresControlPlane {
    pool: PgPool,
}

/// Columns shared by all upstream SELECTs.
const UPSTREAM_COLS: &str = "id, account_id, name, protocol, host, port, addr, is_preset";

fn upstream_from_row(
    row: (
        i64,
        Option<i64>,
        String,
        String,
        String,
        i32,
        Option<String>,
        bool,
    ),
) -> Result<Upstream, Box<dyn std::error::Error + Send + Sync>> {
    let (id, account_id, name, protocol, host, port, addr, is_preset) = row;
    Ok(Upstream {
        id,
        account_id,
        name,
        protocol: protocol.parse()?,
        host,
        port: u16::try_from(port).map_err(|_| "upstream port out of range")?,
        addr,
        is_preset,
    })
}

fn rule_from_row(
    row: (
        i64,
        i64,
        i32,
        String,
        String,
        String,
        Option<i64>,
        Option<String>,
        bool,
    ),
) -> Result<Rule, Box<dyn std::error::Error + Send + Sync>> {
    let (
        id,
        account_id,
        position,
        action,
        target_type,
        target_value,
        limit_count,
        limit_window,
        enabled,
    ) = row;
    Ok(Rule {
        id,
        account_id,
        position,
        action: action.parse()?,
        target_type: target_type.parse()?,
        target_value,
        limit_count,
        limit_window: limit_window.map(|w| w.parse()).transpose()?,
        enabled,
    })
}

impl PostgresControlPlane {
    /// Connect, run embedded migrations, and seed preset upstreams.
    pub async fn connect(url: &str) -> CplResult<Self> {
        let pool = PgPool::connect(url).await?;
        sqlx::migrate!("./migrations").run(&pool).await?;

        let cp = Self { pool };
        cp.seed_presets().await?;
        Ok(cp)
    }

    async fn seed_presets(&self) -> CplResult<()> {
        for preset in preset_upstreams_default() {
            sqlx::query(
                "INSERT INTO upstreams (account_id, name, protocol, host, port, addr, is_preset, created_at)
                 VALUES (NULL, $1, $2, $3, $4, $5, TRUE, $6)
                 ON CONFLICT DO NOTHING",
            )
            .bind(&preset.name)
            .bind(preset.protocol.as_str())
            .bind(&preset.host)
            .bind(preset.port as i32)
            .bind(&preset.addr)
            .bind(now())
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    async fn account_id(&self, account_number: &str) -> CplResult<Option<i64>> {
        let row: Option<(i64,)> =
            sqlx::query_as("SELECT id FROM accounts WHERE account_number = $1")
                .bind(account_number)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(|r| r.0))
    }

    async fn account_id_or_create(&self, account_number: &str) -> CplResult<i64> {
        if let Some(id) = self.account_id(account_number).await? {
            return Ok(id);
        }
        let row: (i64,) = sqlx::query_as(
            "INSERT INTO accounts (account_number, created_at, updated_at)
             VALUES ($1, $2, $2)
             ON CONFLICT (account_number) DO UPDATE SET account_number = EXCLUDED.account_number
             RETURNING id",
        )
        .bind(account_number)
        .bind(now())
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }
}

#[async_trait]
impl ControlPlane for PostgresControlPlane {
    async fn rules(&self, account_number: &str) -> CplResult<Vec<Rule>> {
        let Some(account_id) = self.account_id(account_number).await? else {
            return Ok(Vec::new());
        };
        let rows = sqlx::query_as::<
            _,
            (
                i64,
                i64,
                i32,
                String,
                String,
                String,
                Option<i64>,
                Option<String>,
                bool,
            ),
        >(
            "SELECT id, account_id, position, action, target_type, target_value,
                    limit_count, limit_window, enabled
             FROM rules WHERE account_id = $1 ORDER BY position",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(rule_from_row).collect()
    }

    async fn active_upstream(&self, account_number: &str) -> CplResult<Option<Upstream>> {
        let Some(account_id) = self.account_id(account_number).await? else {
            return Ok(None);
        };
        let sql = format!(
            "SELECT {UPSTREAM_COLS} FROM upstreams WHERE account_id = $1 ORDER BY id LIMIT 1"
        );
        let row = sqlx::query_as::<
            _,
            (
                i64,
                Option<i64>,
                String,
                String,
                String,
                i32,
                Option<String>,
                bool,
            ),
        >(&sql)
        .bind(account_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(upstream_from_row).transpose()
    }

    async fn record_quota(
        &self,
        account_number: &str,
        rule_id: i64,
        limit_count: i64,
        window: crate::model::Window,
    ) -> CplResult<bool> {
        if limit_count <= 0 {
            return Ok(false);
        }
        let Some(account_id) = self.account_id(account_number).await? else {
            return Ok(true);
        };
        let window_start = crate::quota::window_start(now(), window);

        let row: Option<(i64,)> = sqlx::query_as(
            "WITH ins AS (
                INSERT INTO quota_counters (account_id, rule_id, window_start, count)
                VALUES ($1, $2, $3, 1)
                ON CONFLICT (rule_id, window_start)
                DO UPDATE SET count = quota_counters.count + 1
                WHERE quota_counters.count < $4
                RETURNING count
            )
            SELECT count FROM ins",
        )
        .bind(account_id)
        .bind(rule_id)
        .bind(window_start)
        .bind(limit_count)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.is_some())
    }

    async fn create_account(&self, account_number: &str) -> CplResult<Account> {
        let id = self.account_id_or_create(account_number).await?;
        Ok(Account {
            id,
            account_number: account_number.to_string(),
        })
    }

    async fn get_account(&self, account_number: &str) -> CplResult<Option<Account>> {
        let row: Option<(i64, String)> =
            sqlx::query_as("SELECT id, account_number FROM accounts WHERE account_number = $1")
                .bind(account_number)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(|(id, account_number)| Account { id, account_number }))
    }

    async fn replace_rules(&self, account_number: &str, rules: &[NewRule]) -> CplResult<Vec<Rule>> {
        let account_id = self.account_id_or_create(account_number).await?;
        let mut tx = self.pool.begin().await?;

        sqlx::query("DELETE FROM rules WHERE account_id = $1")
            .bind(account_id)
            .execute(&mut *tx)
            .await?;

        let mut out = Vec::with_capacity(rules.len());
        for (position, new) in rules.iter().enumerate() {
            let row: (i64,) = sqlx::query_as(
                "INSERT INTO rules (account_id, position, action, target_type, target_value,
                                   limit_count, limit_window, enabled, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9)
                 RETURNING id",
            )
            .bind(account_id)
            .bind(position as i32)
            .bind(new.action.as_str())
            .bind(new.target_type.as_str())
            .bind(&new.target_value)
            .bind(new.limit_count)
            .bind(new.limit_window.map(|w| w.as_str().to_string()))
            .bind(new.enabled)
            .bind(now())
            .fetch_one(&mut *tx)
            .await?;

            out.push(Rule {
                id: row.0,
                account_id,
                position: position as i32,
                action: new.action,
                target_type: new.target_type,
                target_value: new.target_value.clone(),
                limit_count: new.limit_count,
                limit_window: new.limit_window,
                enabled: new.enabled,
            });
        }

        tx.commit().await?;
        Ok(out)
    }

    async fn list_upstreams(&self, account_number: &str) -> CplResult<Vec<Upstream>> {
        let Some(account_id) = self.account_id(account_number).await? else {
            return Ok(Vec::new());
        };
        let sql =
            format!("SELECT {UPSTREAM_COLS} FROM upstreams WHERE account_id = $1 ORDER BY id");
        let rows = sqlx::query_as::<
            _,
            (
                i64,
                Option<i64>,
                String,
                String,
                String,
                i32,
                Option<String>,
                bool,
            ),
        >(&sql)
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(upstream_from_row).collect()
    }

    async fn replace_upstreams(
        &self,
        account_number: &str,
        upstreams: &[NewUpstream],
    ) -> CplResult<Vec<Upstream>> {
        let account_id = self.account_id_or_create(account_number).await?;
        let mut tx = self.pool.begin().await?;

        sqlx::query("DELETE FROM upstreams WHERE account_id = $1")
            .bind(account_id)
            .execute(&mut *tx)
            .await?;

        let mut out = Vec::with_capacity(upstreams.len());
        for new in upstreams {
            let row: (i64,) = sqlx::query_as(
                "INSERT INTO upstreams (account_id, name, protocol, host, port, addr, is_preset, created_at)
                 VALUES ($1, $2, $3, $4, $5, $6, FALSE, $7)
                 RETURNING id",
            )
            .bind(account_id)
            .bind(&new.name)
            .bind(new.protocol.as_str())
            .bind(&new.host)
            .bind(new.port as i32)
            .bind(&new.addr)
            .bind(now())
            .fetch_one(&mut *tx)
            .await?;

            out.push(Upstream {
                id: row.0,
                account_id: Some(account_id),
                name: new.name.clone(),
                protocol: new.protocol,
                host: new.host.clone(),
                port: new.port,
                addr: new.addr.clone(),
                is_preset: false,
            });
        }

        tx.commit().await?;
        Ok(out)
    }

    async fn preset_upstreams(&self) -> CplResult<Vec<Upstream>> {
        let sql = format!("SELECT {UPSTREAM_COLS} FROM upstreams WHERE is_preset ORDER BY id");
        let rows = sqlx::query_as::<
            _,
            (
                i64,
                Option<i64>,
                String,
                String,
                String,
                i32,
                Option<String>,
                bool,
            ),
        >(&sql)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(upstream_from_row).collect()
    }

    async fn categories(&self) -> CplResult<HashMap<String, HashSet<String>>> {
        let rows: Vec<(String, String)> =
            sqlx::query_as("SELECT domain, category FROM domain_categories")
                .fetch_all(&self.pool)
                .await?;
        let mut map: HashMap<String, HashSet<String>> = HashMap::new();
        for (domain, category) in rows {
            map.entry(domain).or_default().insert(category);
        }
        Ok(map)
    }

    async fn replace_categories(
        &self,
        categories: &HashMap<String, HashSet<String>>,
    ) -> CplResult<()> {
        let mut tx = self.pool.begin().await?;
        let now = now();

        sqlx::query("DELETE FROM domain_categories")
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM domains").execute(&mut *tx).await?;

        for (domain, cats) in categories {
            sqlx::query("INSERT INTO domains (domain, updated_at) VALUES ($1, $2)")
                .bind(domain)
                .bind(now)
                .execute(&mut *tx)
                .await?;
            for category in cats {
                sqlx::query("INSERT INTO domain_categories (domain, category) VALUES ($1, $2)")
                    .bind(domain)
                    .bind(category)
                    .execute(&mut *tx)
                    .await?;
            }
        }

        tx.commit().await?;
        Ok(())
    }

    async fn last_ingestion(&self) -> CplResult<Option<i64>> {
        let row: Option<(i64,)> = sqlx::query_as("SELECT MAX(updated_at) FROM domains")
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| r.0))
    }

    async fn snapshot(&self) -> CplResult<HashMap<String, crate::model::AccountPolicy>> {
        use crate::model::AccountPolicy;

        let accounts: Vec<(i64, String)> =
            sqlx::query_as("SELECT id, account_number FROM accounts")
                .fetch_all(&self.pool)
                .await?;

        let rule_rows = sqlx::query_as::<
            _,
            (
                i64,
                i64,
                i32,
                String,
                String,
                String,
                Option<i64>,
                Option<String>,
                bool,
            ),
        >(
            "SELECT id, account_id, position, action, target_type, target_value,
                    limit_count, limit_window, enabled
             FROM rules ORDER BY account_id, position",
        )
        .fetch_all(&self.pool)
        .await?;

        let upstream_rows = sqlx::query_as::<_, (
            i64, Option<i64>, String, String, String, i32, Option<String>, bool,
        )>(
            "SELECT DISTINCT ON (account_id) id, account_id, name, protocol, host, port, addr, is_preset
             FROM upstreams WHERE account_id IS NOT NULL
             ORDER BY account_id, id",
        )
        .fetch_all(&self.pool)
        .await?;

        let mut rules_by_account: HashMap<i64, Vec<crate::model::Rule>> = HashMap::new();
        for row in rule_rows {
            rules_by_account
                .entry(row.1)
                .or_default()
                .push(rule_from_row(row)?);
        }
        let mut upstream_by_account: HashMap<i64, crate::model::Upstream> = HashMap::new();
        for row in upstream_rows {
            upstream_by_account.insert(row.1.unwrap_or(0), upstream_from_row(row)?);
        }

        let mut out: HashMap<String, AccountPolicy> = HashMap::new();
        for (id, number) in accounts {
            out.insert(
                number,
                AccountPolicy {
                    rules: rules_by_account.remove(&id).unwrap_or_default(),
                    upstream: upstream_by_account.remove(&id),
                },
            );
        }
        Ok(out)
    }
}
