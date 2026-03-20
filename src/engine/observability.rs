// Observability setup for axum-apcore.
//
// Configures tracing middleware, metrics collection, and structured logging
// from apcore's observability primitives, matching fastapi-apcore behavior.

use std::sync::Arc;

use apcore::observability::error_history::ErrorHistory;
use apcore::observability::exporters::StdoutExporter;
use apcore::observability::metrics::MetricsCollector;
use apcore::observability::tracing_middleware::TracingMiddleware;

use crate::config::ApcoreSettings;

/// Set up observability components based on settings.
///
/// Configures:
/// - `TracingMiddleware` with stdout span exporter (if tracing is enabled)
/// - `MetricsCollector` for call counts and latency (if metrics is enabled)
/// - `ErrorHistory` tracker (if any observability feature is enabled)
pub fn setup_observability(settings: &ApcoreSettings) -> ObservabilityState {
    let mut state = ObservabilityState::default();

    if settings.tracing {
        tracing::info!("Enabling apcore tracing middleware");
        state.tracing_enabled = true;
        let exporter = Box::new(StdoutExporter);
        state.tracing_middleware = Some(Arc::new(TracingMiddleware::new(exporter)));
    }

    if settings.metrics {
        tracing::info!("Enabling apcore metrics collection");
        state.metrics_enabled = true;
        state.metrics_collector = Some(Arc::new(std::sync::Mutex::new(MetricsCollector::new())));
    }

    if settings.observability_logging {
        tracing::info!("Enabling apcore structured logging");
        state.logging_enabled = true;
    }

    // Enable error history if any observability is active
    if state.tracing_enabled || state.metrics_enabled || state.logging_enabled {
        state.error_history = Some(Arc::new(std::sync::Mutex::new(ErrorHistory::new(100))));
    }

    state
}

/// Current observability configuration state.
#[derive(Debug, Default, Clone)]
pub struct ObservabilityState {
    pub tracing_enabled: bool,
    pub metrics_enabled: bool,
    pub logging_enabled: bool,
    pub tracing_middleware: Option<Arc<TracingMiddleware>>,
    pub metrics_collector: Option<Arc<std::sync::Mutex<MetricsCollector>>>,
    pub error_history: Option<Arc<std::sync::Mutex<ErrorHistory>>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_setup_observability_defaults() {
        let settings = ApcoreSettings::default();
        let state = setup_observability(&settings);
        assert!(!state.tracing_enabled);
        assert!(!state.metrics_enabled);
        assert!(!state.logging_enabled);
        assert!(state.tracing_middleware.is_none());
        assert!(state.metrics_collector.is_none());
        assert!(state.error_history.is_none());
    }

    #[test]
    fn test_setup_observability_all_enabled() {
        let settings = ApcoreSettings {
            tracing: true,
            metrics: true,
            observability_logging: true,
            ..ApcoreSettings::default()
        };
        let state = setup_observability(&settings);
        assert!(state.tracing_enabled);
        assert!(state.metrics_enabled);
        assert!(state.logging_enabled);
        assert!(state.tracing_middleware.is_some());
        assert!(state.metrics_collector.is_some());
        assert!(state.error_history.is_some());
    }

    #[test]
    fn test_setup_observability_tracing_only() {
        let settings = ApcoreSettings {
            tracing: true,
            ..ApcoreSettings::default()
        };
        let state = setup_observability(&settings);
        assert!(state.tracing_enabled);
        assert!(state.tracing_middleware.is_some());
        assert!(!state.metrics_enabled);
        assert!(state.metrics_collector.is_none());
        assert!(state.error_history.is_some());
    }
}
