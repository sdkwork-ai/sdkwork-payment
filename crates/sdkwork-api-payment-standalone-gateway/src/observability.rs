//! Observability export for the standalone payment gateway.
//!
//! `OBSERVABILITY_SPEC.md` §traces: services SHOULD use OpenTelemetry-compatible
//! trace concepts. The export is opt-in and self-describing: when the standard
//! OpenTelemetry environment is present (`OTEL_EXPORTER_OTLP_ENDPOINT`), spans
//! are exported over OTLP/HTTP (batched; timeout via
//! `OTEL_EXPORTER_OTLP_TIMEOUT`); without it the gateway keeps pure
//! in-process tracing with zero network behavior change. Nothing here reads
//! secrets or embeds env-baked configuration into the binary.

use opentelemetry::trace::TracerProvider as _;
use tracing_subscriber::prelude::*;
use opentelemetry::KeyValue;
use opentelemetry_sdk::Resource;

const DEFAULT_SERVICE_NAME: &str = "sdkwork-payment-standalone";

/// True when the standard OTLP endpoint environment is present.
#[must_use]
pub fn otlp_export_requested() -> bool {
    std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .ok()
        .is_some_and(|value| !value.trim().is_empty())
}

fn service_name() -> String {
    std::env::var("OTEL_SERVICE_NAME")
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_SERVICE_NAME.to_owned())
}

fn deployment_environment() -> String {
    std::env::var("SDKWORK_ENVIRONMENT")
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "development".to_owned())
}

fn resource() -> Resource {
    Resource::builder()
        .with_attributes([
            // OpenTelemetry semantic-conventions resource attribute keys.
            KeyValue::new("service.name", service_name()),
            KeyValue::new("deployment.environment", deployment_environment()),
        ])
        .build()
}

/// Tracer provider handle; `shutdown` flushes the batch exporter. Call it
/// during graceful drain after the HTTP server has stopped.
pub struct OtelGuard {
    provider: opentelemetry_sdk::trace::SdkTracerProvider,
}

impl OtelGuard {
    pub fn shutdown(self) {
        if let Err(error) = self.provider.shutdown() {
            tracing::warn!(?error, "otel tracer provider shutdown reported errors");
        }
    }
}

/// Builds the OTLP/HTTP span tracer. Returns `Ok(None)` when export is not
/// requested; `Err` carries operator-actionable guidance for a present-but
/// unusable configuration.
type OtelTracerParts = (opentelemetry_sdk::trace::SdkTracer, opentelemetry_sdk::trace::SdkTracerProvider);
fn build_otlp_tracer() -> Result<Option<OtelTracerParts>, String> {
    if !otlp_export_requested() {
        return Ok(None);
    }
    let protocol = std::env::var("OTEL_EXPORTER_OTLP_PROTOCOL")
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .unwrap_or_else(|| "http-proto".to_owned());
    if protocol != "http-proto" {
        return Err(format!(
            "unsupported OTEL_EXPORTER_OTLP_PROTOCOL '{protocol}': this gateway exports OTLP over HTTP with protobuf encoding; drop the variable for the default or set http-proto"
        ));
    }
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .build()
        .map_err(|error| format!("OTLP span exporter init failed: {error}"))?;
    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(resource())
        .build();
    let tracer = provider.tracer("sdkwork-payment");
    Ok(Some((tracer, provider)))
}

/// Installs the global tracing subscriber: a fmt layer plus, when the OTLP
/// environment is present, an OpenTelemetry span layer. Returns the provider
/// guard for shutdown during drain.
///
/// # Panics
/// Only when another global default subscriber was already set, which for the
/// standalone gateway binary is a programming error at the single call site.
pub fn init_tracing() -> OtelGuard {
    match build_otlp_tracer() {
        Ok(Some((tracer, provider))) => {
            let guard = OtelGuard { provider };
            tracing_subscriber::registry()
                .with(tracing_subscriber::fmt::layer())
                .with(tracing_opentelemetry::layer().with_tracer(tracer))
                .init();
            tracing::info!(
                endpoint = %std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").unwrap_or_default(),
                service = %service_name(),
                "OTLP span export enabled"
            );
            guard
        }
        outcome => {
            if let Err(ref message) = outcome {
                tracing::warn!(%message, "OTLP export disabled: falling back to stdout tracing only");
            }
            tracing_subscriber::registry()
                .with(tracing_subscriber::fmt::layer())
                .init();
            let provider = match outcome {
                // Keep the real provider alive so the guard's shutdown flushes
                // in-flight spans even though the layer wiring failed.
                Ok(Some((_, provider))) => provider,
                _ => opentelemetry_sdk::trace::SdkTracerProvider::builder().build(),
            };
            OtelGuard { provider }
        }
    }
}
