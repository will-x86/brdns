//! Logs and OTEL tracing.

use crate::config::ObservabilityConfig;

/// Init logs (+ optional OTLP trace export). Also routes the
/// existing `log`-crate macros into `tracing`.
pub fn init(cfg: &ObservabilityConfig) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use tracing_subscriber::EnvFilter;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let fmt_layer = tracing_subscriber::fmt::layer().json();

    let registry = tracing_subscriber::registry().with(filter).with(fmt_layer);

    if let Some(endpoint) = &cfg.otel_endpoint {
        use opentelemetry::trace::TracerProvider as _;
        use opentelemetry_otlp::{SpanExporter, WithExportConfig};
        use opentelemetry_sdk::Resource;
        use opentelemetry_sdk::trace::SdkTracerProvider;

        let exporter = SpanExporter::builder()
            .with_http()
            .with_endpoint(endpoint)
            .build()?;
        let provider = SdkTracerProvider::builder()
            .with_batch_exporter(exporter)
            .with_resource(Resource::builder().with_service_name("brdns").build())
            .build();
        let tracer = provider.tracer("brdns");
        opentelemetry::global::set_tracer_provider(provider);
        let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);

        registry.with(telemetry).try_init()?;
    } else {
        registry.try_init()?;
    }

    Ok(())
}
