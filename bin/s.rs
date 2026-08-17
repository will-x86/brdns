use std::sync::Arc;
use std::time::{Duration, Instant};

use brdns::receiver::{DohReceiver, DotReceiver, Receiver};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let settings = brdns::config::load();

    // Logs + otel
    if let Err(e) = brdns::observability::init(&settings.observability) {
        eprintln!("failed to init observability: {e}");
    }

    // Init control plane, either Postgres or in mem
    let control_plane =
        brdns::controlplane::init(settings.control_plane.database_url.as_deref()).await?;

    // PolicyCache
    let cache = Arc::new(brdns::policy::PolicyCache::new());
    match cache.refresh(control_plane.as_ref()).await {
        Ok(n) => {
            log::info!("policy cache loaded {n} accounts");
            brdns::observability::set_policy_accounts(n as i64);
        }
        Err(e) => log::warn!("initial policy refresh failed: {e}"),
    }

    // CategoryIndex: load saved categories first
    let categories = Arc::new(brdns::categories::CategoryIndex::new());
    match brdns::blocklist::load(control_plane.as_ref(), &categories).await {
        Ok(n) => {
            brdns::observability::set_category_domains(n as i64);
            if n > 0 {
                log::info!("loaded {n} domains into category index");
            }
        }
        Err(e) => log::warn!("failed to load categories from control plane: {e}"),
    }
    // Then ingest
    if settings.blocklist.enabled {
        let feeds: Vec<brdns::blocklist::Feed> = if settings.blocklist.feeds.is_empty() {
            brdns::blocklist::default_feeds()
        } else {
            settings.blocklist.feeds.iter().map(Into::into).collect()
        };
        let interval = settings.blocklist.refresh_interval_secs.max(1);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock before unix epoch")
            .as_secs() as i64;
        let fresh = match control_plane.last_ingestion().await {
            Ok(Some(last)) => now.saturating_sub(last) < interval as i64,
            Ok(None) => false,
            Err(e) => {
                log::warn!("failed to read last blocklist ingestion: {e}");
                false
            }
        };
        if fresh {
            log::info!("blocklist ingestion skipped: refreshed within the last {interval}s");
        } else {
            match brdns::blocklist::ingest(control_plane.as_ref(), &categories, &feeds).await {
                Ok(n) => {
                    log::info!("blocklist ingestion complete: {n} domains");
                    brdns::observability::set_category_domains(n as i64);
                }
                Err(e) => log::error!("blocklist ingestion failed: {e}"),
            }
        }
    }

    // Context for the receivers (DoT and DoH)
    let ctx = Arc::new(brdns::context::RuntimeContext::new(
        Arc::clone(&control_plane),
        Arc::clone(&categories),
        brdns::blocking::BlockPolicy::from_config(&settings.policy),
        Arc::clone(&cache),
    ));

    // Prom metrics endpoint.
    if !settings.observability.metrics_addr.is_empty() {
        let addr = settings.observability.metrics_addr.clone();
        tokio::spawn(async move {
            log::info!("metrics listening on http://{addr}");
            if let Err(e) = brdns::observability::serve_metrics(&addr).await {
                log::error!("metrics server failed: {e}");
            }
        });
    }

    // Background poll: refresh the policy snapshot and (less often) the
    // blocklists. Interval poll
    {
        let cp = Arc::clone(&control_plane);
        let cache = Arc::clone(&cache);
        let categories = Arc::clone(&categories);
        let feeds: Vec<brdns::blocklist::Feed> = if settings.blocklist.feeds.is_empty() {
            brdns::blocklist::default_feeds()
        } else {
            settings.blocklist.feeds.iter().map(Into::into).collect()
        };
        let blocklist_enabled = settings.blocklist.enabled;
        let policy_secs = settings.control_plane.policy_refresh_secs.max(1);
        let blocklist_secs = settings.blocklist.refresh_interval_secs.max(1);
        tokio::spawn(async move {
            let mut last_blocklist = Instant::now();
            loop {
                tokio::time::sleep(Duration::from_secs(policy_secs)).await;

                match cache.refresh(cp.as_ref()).await {
                    Ok(n) => {
                        log::debug!("policy refresh: {n} accounts");
                        brdns::observability::set_policy_accounts(n as i64);
                    }
                    Err(e) => log::warn!("policy refresh failed: {e}"),
                }

                if blocklist_enabled
                    && last_blocklist.elapsed() >= Duration::from_secs(blocklist_secs)
                {
                    match brdns::blocklist::ingest(cp.as_ref(), &categories, &feeds).await {
                        Ok(n) => {
                            log::info!("blocklist refresh: {n} domains");
                            brdns::observability::set_category_domains(n as i64);
                        }
                        Err(e) => log::warn!("blocklist refresh failed: {e}"),
                    }
                    last_blocklist = Instant::now();
                }
            }
        });
    }

    // Run the management API when enabled.
    if settings.control_plane.enabled {
        if settings.control_plane.api_token.is_none() {
            log::warn!(
                "control plane enabled without api_token; management API will refuse requests"
            );
        }
        let state = Arc::new(brdns::controlplane::http::ApiState {
            cp: Arc::clone(&control_plane),
            token: settings.control_plane.api_token.clone(),
        });
        let addr = settings.control_plane.listen_addr.clone();
        tokio::spawn(async move {
            log::info!("control plane listening on http://{addr}");
            if let Err(e) = brdns::controlplane::http::serve(state, &addr).await {
                log::error!("control plane failed: {e}");
            }
        });
    }

    // Build both DoT and DoH receivers.
    let dot = Box::new(DotReceiver::from_config(
        settings.dot.listen_port,
        settings.dot.clone(),
        &settings.server,
        &settings.certs,
        Arc::clone(&ctx),
    )?) as Box<dyn Receiver>;
    let doh = Box::new(DohReceiver::from_config(
        settings.doh.listen_port,
        settings.doh.clone(),
        &settings.server,
        &settings.certs,
        Arc::clone(&ctx),
    )?) as Box<dyn Receiver>;

    log::info!(
        "running both dot:{} and doh:{}",
        settings.dot.listen_port,
        settings.doh.listen_port
    );
    tokio::join!(dot.run(), doh.run());
    Ok(())
}
