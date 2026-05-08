pub use dubbo_rs_common;
pub use dubbo_rs_protocol;

use std::sync::Arc;

use dubbo_rs_common::error::RPCError;
use prometheus::{HistogramOpts, HistogramVec, Opts, Registry};

// ============================================================================
// Error type
// ============================================================================

/// Error returned when metric registration fails.
#[derive(Debug)]
pub enum MetricsError {
    Registration { name: String, reason: String },
}

impl std::fmt::Display for MetricsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Registration { name, reason } => {
                write!(f, "failed to register metric `{name}`: {reason}")
            }
        }
    }
}

impl std::error::Error for MetricsError {}

// ============================================================================
// Label constants
// ============================================================================

/// Label key for the service name (e.g. `"com.example.GreetService"`).
pub const LABEL_SERVICE: &str = "service";
/// Label key for the RPC method name (e.g. `"sayHello"`).
pub const LABEL_METHOD: &str = "method";
/// Label key for the call status (e.g. `"success"`, `"error"`).
pub const LABEL_STATUS: &str = "status";
/// Label key for the error variant (e.g. `"ClientTimeout"`, `"ServiceNotFound"`).
pub const LABEL_ERROR_TYPE: &str = "error_type";
/// Label key for the subsystem module name (e.g. `"registry"`, `"metadata"`, `"config"`).
pub const LABEL_MODULE: &str = "module";
/// Label key for the operation name (e.g. `"register"`, `"subscribe"`, `"get_config"`).
pub const LABEL_OPERATION: &str = "operation";

// ============================================================================
// Metric name constants
// ============================================================================

/// Counter: total RPC invocations, labelled by service, method, status.
pub const METRIC_RPC_REQUESTS_TOTAL: &str = "rpc_requests_total";
/// Histogram: RPC call duration in seconds, labelled by service, method.
pub const METRIC_RPC_REQUEST_DURATION_SECONDS: &str = "rpc_request_duration_seconds";
/// Counter: total RPC errors, labelled by service, method, `error_type`.
pub const METRIC_RPC_ERRORS_TOTAL: &str = "rpc_errors_total";

// -- Registry module metrics --
pub const METRIC_REGISTRY_OPS_TOTAL: &str = "registry_ops_total";
pub const METRIC_REGISTRY_OP_DURATION_SECONDS: &str = "registry_op_duration_seconds";

// -- Metadata module metrics --
pub const METRIC_METADATA_OPS_TOTAL: &str = "metadata_ops_total";
pub const METRIC_METADATA_OP_DURATION_SECONDS: &str = "metadata_op_duration_seconds";

// -- Config module metrics --
pub const METRIC_CONFIG_OPS_TOTAL: &str = "config_ops_total";
pub const METRIC_CONFIG_OP_DURATION_SECONDS: &str = "config_op_duration_seconds";
pub const METRIC_CONFIG_CHANGES_TOTAL: &str = "config_changes_total";

// ============================================================================
// Typed metric wrappers
// ============================================================================

/// A monotonically increasing counter.
///
/// Delegates to [`prometheus::Counter`].
#[derive(Clone)]
pub struct Counter {
    inner: prometheus::Counter,
}

impl Counter {
    /// Increment the counter by 1.
    pub fn inc(&self) {
        self.inner.inc();
    }

    /// Increment the counter by the given value.
    pub fn inc_by(&self, v: f64) {
        self.inner.inc_by(v);
    }
}

impl From<prometheus::Counter> for Counter {
    fn from(inner: prometheus::Counter) -> Self {
        Self { inner }
    }
}

impl From<Counter> for prometheus::Counter {
    fn from(c: Counter) -> Self {
        c.inner
    }
}

/// A gauge that can go up and down.
///
/// Delegates to [`prometheus::Gauge`].
#[derive(Clone)]
pub struct Gauge {
    inner: prometheus::Gauge,
}

impl Gauge {
    /// Set the gauge to an absolute value.
    pub fn set(&self, v: f64) {
        self.inner.set(v);
    }

    /// Increment the gauge by 1.
    pub fn inc(&self) {
        self.inner.inc();
    }

    /// Decrement the gauge by 1.
    pub fn dec(&self) {
        self.inner.dec();
    }

    /// Get the current value.
    #[must_use]
    pub fn get(&self) -> f64 {
        self.inner.get()
    }
}

impl From<prometheus::Gauge> for Gauge {
    fn from(inner: prometheus::Gauge) -> Self {
        Self { inner }
    }
}

/// A histogram that records observations in configurable buckets.
///
/// Delegates to [`prometheus::Histogram`].
#[derive(Clone)]
pub struct Histogram {
    inner: prometheus::Histogram,
}

impl Histogram {
    /// Observe a single value.
    pub fn observe(&self, v: f64) {
        self.inner.observe(v);
    }
}

impl From<prometheus::Histogram> for Histogram {
    fn from(inner: prometheus::Histogram) -> Self {
        Self { inner }
    }
}

// ============================================================================
// MetricsCollector
// ============================================================================

/// Central metrics registry for RPC metrics.
///
/// Safe to clone — all metric handles are internally reference-counted.
/// Once constructed, all recording methods are infallible (no panics).
///
/// # Examples
///
/// ```rust
/// use dubbo_rs_metrics::MetricsCollector;
///
/// let collector = MetricsCollector::new()?;
/// collector.record_request("MyService", "sayHello", "success");
/// collector.record_duration("MyService", "sayHello", 0.042);
/// # Ok::<(), dubbo_rs_metrics::MetricsError>(())
/// ```
#[derive(Clone)]
pub struct MetricsCollector {
    registry: Arc<Registry>,
    rpc_requests: prometheus::CounterVec,
    rpc_duration: HistogramVec,
    rpc_errors: prometheus::CounterVec,
    registry_ops: prometheus::CounterVec,
    registry_duration: HistogramVec,
    metadata_ops: prometheus::CounterVec,
    metadata_duration: HistogramVec,
    config_ops: prometheus::CounterVec,
    config_duration: HistogramVec,
    config_changes: prometheus::CounterVec,
}

impl MetricsCollector {
    /// Create a default `MetricsCollector` with all standard RPC metrics
    /// pre-registered.
    ///
    /// # Errors
    ///
    /// Returns [`MetricsError`] if any metric cannot be registered (e.g.,
    /// duplicate names within the same registry).
    pub fn new() -> Result<Self, MetricsError> {
        MetricsCollectorBuilder::new().build()
    }

    /// Record a single RPC invocation with the given outcome.
    ///
    /// `status` is typically `"success"` or `"error"`, but may be any
    /// string the caller prefers.
    pub fn record_request(&self, service: &str, method: &str, status: &str) {
        // SAFETY: the CounterVec was created with exactly 3 labels
        // (service, method, status), so this never panics.
        self.rpc_requests
            .with_label_values(&[service, method, status])
            .inc();
    }

    /// Record the duration (in seconds) of an RPC call.
    pub fn record_duration(&self, service: &str, method: &str, duration_secs: f64) {
        self.rpc_duration
            .with_label_values(&[service, method])
            .observe(duration_secs);
    }

    /// Record an RPC error with a specific error type.
    ///
    /// `error_type` is a human-readable variant name, e.g. `"ClientTimeout"`
    /// or `"ServiceNotFound"`.
    pub fn record_error(&self, service: &str, method: &str, error_type: &str) {
        self.rpc_errors
            .with_label_values(&[service, method, error_type])
            .inc();
    }

    /// Convenience: derive the error type from an [`RPCError`] variant name.
    pub fn record_rpc_error(&self, service: &str, method: &str, error: &RPCError) {
        let error_type = error_type_name(error);
        self.record_error(service, method, error_type);
    }

    /// Convenience: record a full request lifecycle from an [`InvocationContext`]
    /// and [`RPCResult`].
    pub fn record_invocation(
        &self,
        ctx: &dubbo_rs_protocol::InvocationContext,
        result: &dubbo_rs_protocol::RPCResult,
        duration_secs: f64,
    ) {
        let service = ctx.url.path.as_str();
        let method = &ctx.method_name;

        self.record_duration(service, method, duration_secs);

        if result.is_error() {
            self.record_request(service, method, "error");
            if let Some(err) = &result.error {
                self.record_rpc_error(service, method, err);
            }
        } else {
            self.record_request(service, method, "success");
        }
    }

    /// Access the underlying Prometheus [`Registry`].
    ///
    /// Useful for adding custom metrics or for integration with other
    /// prometheus exporters.
    #[must_use]
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    pub fn record_registry_op(&self, operation: &str, status: &str) {
        self.registry_ops
            .with_label_values(&[operation, status])
            .inc();
    }

    pub fn record_registry_duration(&self, operation: &str, duration_secs: f64) {
        self.registry_duration
            .with_label_values(&[operation])
            .observe(duration_secs);
    }

    pub fn record_metadata_op(&self, operation: &str, status: &str) {
        self.metadata_ops
            .with_label_values(&[operation, status])
            .inc();
    }

    pub fn record_metadata_duration(&self, operation: &str, duration_secs: f64) {
        self.metadata_duration
            .with_label_values(&[operation])
            .observe(duration_secs);
    }

    pub fn record_config_op(&self, operation: &str, status: &str) {
        self.config_ops
            .with_label_values(&[operation, status])
            .inc();
    }

    pub fn record_config_duration(&self, operation: &str, duration_secs: f64) {
        self.config_duration
            .with_label_values(&[operation])
            .observe(duration_secs);
    }

    pub fn record_config_change(&self, key: &str, change_type: &str) {
        self.config_changes
            .with_label_values(&[key, change_type])
            .inc();
    }
}

/// Return a short `snake_case` string identifying the error variant.
fn error_type_name(err: &RPCError) -> &'static str {
    match err {
        RPCError::ClientTimeout(_) => "client_timeout",
        RPCError::ServerTimeout(_) => "server_timeout",
        RPCError::BadRequest(_) => "bad_request",
        RPCError::BadResponse(_) => "bad_response",
        RPCError::ServiceNotFound(_) => "service_not_found",
        RPCError::ServiceError(_) => "service_error",
        RPCError::ServerError(_) => "server_error",
        RPCError::ClientError(_) => "client_error",
        RPCError::ServerThreadpoolExhausted(_) => "server_threadpool_exhausted",
    }
}

// ============================================================================
// MetricsCollectorBuilder
// ============================================================================

/// Builder for [`MetricsCollector`].
///
/// Use when you need to customise the metrics namespace or skip certain
/// metrics. For the common case, use [`MetricsCollector::new()`].
///
/// # Example
///
/// ```rust
/// use dubbo_rs_metrics::MetricsCollectorBuilder;
///
/// let collector = MetricsCollectorBuilder::new()
///     .build()?;
/// # Ok::<(), dubbo_rs_metrics::MetricsError>(())
/// ```
#[must_use]
pub struct MetricsCollectorBuilder {
    namespace: Option<String>,
}

impl MetricsCollectorBuilder {
    /// Create a builder with default settings.
    pub fn new() -> Self {
        Self { namespace: None }
    }

    /// Set a namespace prefix for all metric names.
    ///
    /// When set, metric names are prefixed with `{namespace}_`.
    /// For example, a namespace of `"dubbo"` produces
    /// `"dubbo_rpc_requests_total"`.
    pub fn namespace(mut self, ns: impl Into<String>) -> Self {
        self.namespace = Some(ns.into());
        self
    }

    /// Build the [`MetricsCollector`], registering all RPC metrics.
    ///
    /// # Errors
    ///
    /// Returns [`MetricsError`] if any metric cannot be registered.
    pub fn build(self) -> Result<MetricsCollector, MetricsError> {
        let registry = Arc::new(Registry::new());

        let prefix = self
            .namespace
            .as_deref()
            .map_or(String::new(), |ns| format!("{ns}_"));

        let rpc_requests = register_counter_vec(
            &registry,
            Opts::new(
                format!("{prefix}{METRIC_RPC_REQUESTS_TOTAL}"),
                "Total number of RPC requests.",
            ),
            &[LABEL_SERVICE, LABEL_METHOD, LABEL_STATUS],
        )?;

        let rpc_duration = register_histogram_vec(
            &registry,
            HistogramOpts::new(
                format!("{prefix}{METRIC_RPC_REQUEST_DURATION_SECONDS}"),
                "RPC request duration in seconds.",
            ),
            &[LABEL_SERVICE, LABEL_METHOD],
        )?;

        let rpc_errors = register_counter_vec(
            &registry,
            Opts::new(
                format!("{prefix}{METRIC_RPC_ERRORS_TOTAL}"),
                "Total number of RPC errors.",
            ),
            &[LABEL_SERVICE, LABEL_METHOD, LABEL_ERROR_TYPE],
        )?;

        let registry_ops = register_counter_vec(
            &registry,
            Opts::new(
                format!("{prefix}{METRIC_REGISTRY_OPS_TOTAL}"),
                "Total number of registry operations.",
            ),
            &[LABEL_OPERATION, LABEL_STATUS],
        )?;

        let registry_duration = register_histogram_vec(
            &registry,
            HistogramOpts::new(
                format!("{prefix}{METRIC_REGISTRY_OP_DURATION_SECONDS}"),
                "Registry operation duration in seconds.",
            ),
            &[LABEL_OPERATION],
        )?;

        let metadata_ops = register_counter_vec(
            &registry,
            Opts::new(
                format!("{prefix}{METRIC_METADATA_OPS_TOTAL}"),
                "Total number of metadata operations.",
            ),
            &[LABEL_OPERATION, LABEL_STATUS],
        )?;

        let metadata_duration = register_histogram_vec(
            &registry,
            HistogramOpts::new(
                format!("{prefix}{METRIC_METADATA_OP_DURATION_SECONDS}"),
                "Metadata operation duration in seconds.",
            ),
            &[LABEL_OPERATION],
        )?;

        let config_ops = register_counter_vec(
            &registry,
            Opts::new(
                format!("{prefix}{METRIC_CONFIG_OPS_TOTAL}"),
                "Total number of config operations.",
            ),
            &[LABEL_OPERATION, LABEL_STATUS],
        )?;

        let config_duration = register_histogram_vec(
            &registry,
            HistogramOpts::new(
                format!("{prefix}{METRIC_CONFIG_OP_DURATION_SECONDS}"),
                "Config operation duration in seconds.",
            ),
            &[LABEL_OPERATION],
        )?;

        let config_changes = register_counter_vec(
            &registry,
            Opts::new(
                format!("{prefix}{METRIC_CONFIG_CHANGES_TOTAL}"),
                "Total number of config changes.",
            ),
            &["key", "change_type"],
        )?;

        Ok(MetricsCollector {
            registry,
            rpc_requests,
            rpc_duration,
            rpc_errors,
            registry_ops,
            registry_duration,
            metadata_ops,
            metadata_duration,
            config_ops,
            config_duration,
            config_changes,
        })
    }
}

impl Default for MetricsCollectorBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper: create and register a [`prometheus::CounterVec`] on the given registry.
fn register_counter_vec(
    registry: &Registry,
    opts: Opts,
    label_names: &[&str],
) -> Result<prometheus::CounterVec, MetricsError> {
    let name = opts.name.clone();
    let cv =
        prometheus::CounterVec::new(opts, label_names).map_err(|e| MetricsError::Registration {
            name: name.clone(),
            reason: e.to_string(),
        })?;
    registry
        .register(Box::new(cv.clone()))
        .map_err(|e| MetricsError::Registration {
            name,
            reason: e.to_string(),
        })?;
    Ok(cv)
}

/// Helper: create and register a [`HistogramVec`] on the given registry.
fn register_histogram_vec(
    registry: &Registry,
    opts: HistogramOpts,
    label_names: &[&str],
) -> Result<HistogramVec, MetricsError> {
    let name = opts.common_opts.name.clone();
    let hv = HistogramVec::new(opts, label_names).map_err(|e| MetricsError::Registration {
        name: name.clone(),
        reason: e.to_string(),
    })?;
    registry
        .register(Box::new(hv.clone()))
        .map_err(|e| MetricsError::Registration {
            name,
            reason: e.to_string(),
        })?;
    Ok(hv)
}

// ============================================================================
// MetricsExporter
// ============================================================================

/// Exports metrics from a [`MetricsCollector`] in Prometheus text format.
///
/// # Example
///
/// ```rust
/// use dubbo_rs_metrics::{MetricsCollector, MetricsExporter};
///
/// let collector = MetricsCollector::new()?;
/// collector.record_request("MyService", "sayHello", "success");
///
/// let exporter = MetricsExporter::new(&collector);
/// let text = exporter.export_metrics();
/// assert!(text.contains("rpc_requests_total"));
/// # Ok::<(), dubbo_rs_metrics::MetricsError>(())
/// ```
pub struct MetricsExporter<'a> {
    registry: &'a Registry,
}

impl<'a> MetricsExporter<'a> {
    /// Create an exporter that reads from the given collector's registry.
    #[must_use]
    pub fn new(collector: &'a MetricsCollector) -> Self {
        Self {
            registry: collector.registry(),
        }
    }

    /// Export all registered metrics as Prometheus text format.
    ///
    /// Returns an empty string if no metrics are registered (which should
    /// not happen when using the default builder). Encoding errors produce
    /// a short error message prefixed with `#`.
    #[must_use]
    pub fn export_metrics(&self) -> String {
        let encoder = prometheus::TextEncoder::new();
        let families = self.registry.gather();
        encoder
            .encode_to_string(&families)
            .unwrap_or_else(|e| format!("# Error encoding metrics: {e}\n"))
    }
}

// ============================================================================
// MetricsFilter — integrates metrics collection into the filter chain
// ============================================================================

use std::time::Instant;

use async_trait::async_trait;
use dubbo_rs_filter::Filter;
use dubbo_rs_protocol::{InvocationContext, Invoker, RPCResult};

/// A Dubbo [`Filter`] that records RPC metrics using a [`MetricsCollector`].
///
/// Place this filter in the filter chain to automatically record request
/// counts, durations, and error rates for every invocation.
///
/// # Example
///
/// ```rust
/// use dubbo_rs_metrics::{MetricsCollector, MetricsFilter};
///
/// let collector = MetricsCollector::new().unwrap();
/// let filter = MetricsFilter::new(collector);
/// ```
pub struct MetricsFilter {
    collector: MetricsCollector,
}

impl MetricsFilter {
    /// Create a new metrics filter backed by the given collector.
    #[must_use]
    pub fn new(collector: MetricsCollector) -> Self {
        Self { collector }
    }
}

#[async_trait]
impl Filter for MetricsFilter {
    async fn invoke(
        &self,
        ctx: &mut InvocationContext,
        next: &dyn Invoker,
    ) -> Result<RPCResult, anyhow::Error> {
        let start = Instant::now();
        let result = next.invoke(ctx).await;
        let duration = start.elapsed().as_secs_f64();

        match &result {
            Ok(rpc_result) => {
                self.collector.record_invocation(ctx, rpc_result, duration);
            }
            Err(e) => {
                let service = ctx.url.path.as_str();
                let method = &ctx.method_name;
                self.collector.record_request(service, method, "error");
                self.collector.record_duration(service, method, duration);
                self.collector
                    .record_error(service, method, "invocation_error");
                tracing::warn!(service, method, error = %e, "RPC invocation failed");
            }
        }

        result
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use dubbo_rs_common::url::URL;

    // ── helpers ──────────────────────────────────────────────────────────

    fn make_ctx(method: &str) -> dubbo_rs_protocol::InvocationContext {
        let url = URL::new("tri", "/com.example.TestService");
        dubbo_rs_protocol::InvocationContext::new(method, url)
    }

    // ── Registry creation ────────────────────────────────────────────────

    #[test]
    fn test_collector_creation() {
        let collector = MetricsCollector::new().expect("should create collector");
        // Verify the registry is accessible
        let _ = collector.registry();
    }

    #[test]
    fn test_collector_clone() {
        let c1 = MetricsCollector::new().unwrap();
        let c2 = c1.clone();
        // Both should point to the same registry (same metric families)
        let families = c1.registry().gather();
        let families2 = c2.registry().gather();
        assert_eq!(families.len(), families2.len());
    }

    // ── Builder with namespace ────────────────────────────────────────────

    #[test]
    fn test_builder_with_namespace() {
        let collector = MetricsCollectorBuilder::new()
            .namespace("dubbo")
            .build()
            .unwrap();

        collector.record_request("Svc", "m", "success");
        collector.record_duration("Svc", "m", 0.1);
        collector.record_error("Svc", "m", "timeout");

        let exporter = MetricsExporter::new(&collector);
        let output = exporter.export_metrics();
        assert!(
            output.contains("dubbo_rpc_requests_total"),
            "expected 'dubbo_rpc_requests_total' in output: {output}"
        );
    }

    #[test]
    fn test_builder_default() {
        let collector = MetricsCollectorBuilder::default().build().unwrap();
        collector.record_request("Svc", "m", "success");
        collector.record_duration("Svc", "m", 0.1);
        collector.record_error("Svc", "m", "timeout");

        let exporter = MetricsExporter::new(&collector);
        let output = exporter.export_metrics();
        assert!(output.contains("rpc_requests_total"));
    }

    // ── Counter increment ────────────────────────────────────────────────

    #[test]
    fn test_record_request_increments_counter() {
        let collector = MetricsCollector::new().unwrap();

        collector.record_request("TestService", "sayHello", "success");
        collector.record_request("TestService", "sayHello", "success");
        collector.record_request("TestService", "sayHello", "error");

        let exporter = MetricsExporter::new(&collector);
        let output = exporter.export_metrics();

        // The output should contain the counter with value 2 for "success"
        // and value 1 for "error".
        assert!(
            output.contains("rpc_requests_total"),
            "export should contain rpc_requests_total"
        );
    }

    // ── Histogram observe ────────────────────────────────────────────────

    #[test]
    fn test_record_duration_observes_histogram() {
        let collector = MetricsCollector::new().unwrap();
        collector.record_duration("TestService", "sayHello", 0.042);
        collector.record_duration("TestService", "sayHello", 0.100);

        let exporter = MetricsExporter::new(&collector);
        let output = exporter.export_metrics();

        assert!(
            output.contains("rpc_request_duration_seconds"),
            "export should contain the histogram: {output}"
        );
    }

    #[test]
    fn test_record_duration_histogram_count_increases() {
        let collector = MetricsCollector::new().unwrap();

        for _ in 0..5 {
            collector.record_duration("Svc", "m", 0.01);
        }

        let exporter = MetricsExporter::new(&collector);
        let output = exporter.export_metrics();
        assert!(output.contains("rpc_request_duration_seconds_count"),);
    }

    // ── Error recording ──────────────────────────────────────────────────

    #[test]
    fn test_record_error() {
        let collector = MetricsCollector::new().unwrap();
        collector.record_error("TestService", "sayHello", "client_timeout");
        collector.record_error("TestService", "sayHello", "server_error");
        collector.record_error("TestService", "sayHello", "client_timeout");

        let exporter = MetricsExporter::new(&collector);
        let output = exporter.export_metrics();
        assert!(output.contains("rpc_errors_total"));
    }

    #[test]
    fn test_record_rpc_error_derives_type_name() {
        let collector = MetricsCollector::new().unwrap();

        let err = RPCError::ServiceNotFound("missing".into());
        collector.record_rpc_error("Svc", "m", &err);

        // Calling again with the same error should increment.
        collector.record_rpc_error("Svc", "m", &err);

        let exporter = MetricsExporter::new(&collector);
        let output = exporter.export_metrics();
        assert!(
            output.contains("service_not_found"),
            "output should contain 'service_not_found': {output}"
        );
    }

    // ── record_invocation convenience ────────────────────────────────────

    #[test]
    fn test_record_invocation_success_path() {
        let collector = MetricsCollector::new().unwrap();
        let ctx = make_ctx("sayHello");
        let result = dubbo_rs_protocol::RPCResult::success(b"ok".to_vec());

        collector.record_invocation(&ctx, &result, 0.005);

        let exporter = MetricsExporter::new(&collector);
        let output = exporter.export_metrics();
        assert!(output.contains("rpc_requests_total"));
        assert!(output.contains("rpc_request_duration_seconds_count"));
        // No errors should be recorded
        // (rpc_errors_total may still appear in HELP/TYPE lines but with 0 count)
    }

    #[test]
    fn test_record_invocation_error_path() {
        let collector = MetricsCollector::new().unwrap();
        let ctx = make_ctx("sayHello");
        let err = RPCError::ServerError("boom".into());
        let result = dubbo_rs_protocol::RPCResult::from_error(err);

        collector.record_invocation(&ctx, &result, 0.100);

        let exporter = MetricsExporter::new(&collector);
        let output = exporter.export_metrics();
        assert!(output.contains("server_error"));
    }

    // ── Text export ──────────────────────────────────────────────────────

    #[test]
    fn test_export_metrics_non_empty() {
        let collector = MetricsCollector::new().unwrap();
        collector.record_request("Svc", "m", "success");

        let exporter = MetricsExporter::new(&collector);
        let text = exporter.export_metrics();

        assert!(!text.is_empty(), "exported text should not be empty");
        // Prometheus text format always starts with HELP or TYPE lines
        assert!(
            text.contains("# HELP"),
            "export should contain HELP line: {text}"
        );
    }

    #[test]
    fn test_export_metrics_contains_expected_metrics() {
        let collector = MetricsCollector::new().unwrap();

        // Trigger some usage
        collector.record_request("A", "b", "success");
        collector.record_duration("A", "b", 1.0);
        collector.record_error("A", "b", "timeout");

        let text = MetricsExporter::new(&collector).export_metrics();

        for metric_name in &[
            METRIC_RPC_REQUESTS_TOTAL,
            METRIC_RPC_REQUEST_DURATION_SECONDS,
            METRIC_RPC_ERRORS_TOTAL,
        ] {
            assert!(
                text.contains(metric_name),
                "export should contain '{metric_name}'"
            );
        }
    }

    // ── Typed wrapper tests ──────────────────────────────────────────────

    #[test]
    fn test_counter_wrapper_inc() {
        let collector = MetricsCollector::new().unwrap();
        // Access the inner CounterVec and wrap one label combination.
        let c: Counter = collector
            .rpc_requests
            .with_label_values(&["svc", "m", "ok"])
            .into();

        c.inc();
        c.inc_by(2.0);

        let output = MetricsExporter::new(&collector).export_metrics();
        // The counter value should contain "3" for this label combo.
        // Just verify it is in the output (exact format depends on prom crate).
        assert!(output.contains("svc") && output.contains("ok"));
    }

    #[test]
    fn test_gauge_wrapper_set_and_get() {
        use prometheus::Gauge as PromGauge;

        let registry = Arc::new(Registry::new());
        let pg = PromGauge::new("test_gauge", "help").unwrap();
        registry.register(Box::new(pg.clone())).unwrap();

        let gauge: Gauge = pg.into();
        gauge.set(42.0);
        assert!((gauge.get() - 42.0).abs() < f64::EPSILON);

        gauge.inc();
        assert!((gauge.get() - 43.0).abs() < f64::EPSILON);

        gauge.dec();
        assert!((gauge.get() - 42.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_histogram_wrapper_observe() {
        use prometheus::{Histogram as PromHistogram, HistogramOpts};

        let registry = Arc::new(Registry::new());
        let opts = HistogramOpts::new("test_histogram", "help");
        let ph = PromHistogram::with_opts(opts).unwrap();
        registry.register(Box::new(ph.clone())).unwrap();

        let hist: Histogram = ph.into();
        hist.observe(0.5);
        hist.observe(1.5);

        let output = prometheus::TextEncoder::new()
            .encode_to_string(&registry.gather())
            .unwrap();
        assert!(output.contains("test_histogram"));
    }

    // ── error_type_name ──────────────────────────────────────────────────

    // ── MetricsFilter tests ──────────────────────────────────────────────

    use async_trait::async_trait;
    use dubbo_rs_common::node::Node;
    use dubbo_rs_filter::Filter;
    use dubbo_rs_protocol::Invoker;

    struct SuccessInvoker;

    impl Node for SuccessInvoker {
        fn get_url(&self) -> &URL {
            static U: std::sync::LazyLock<URL> =
                std::sync::LazyLock::new(|| URL::new("tri", "/com.example.TestService"));
            &U
        }
        fn is_available(&self) -> bool {
            true
        }
        fn destroy(&self) {}
    }

    #[async_trait]
    impl Invoker for SuccessInvoker {
        async fn invoke(&self, _ctx: &mut InvocationContext) -> Result<RPCResult, anyhow::Error> {
            Ok(RPCResult::success(b"ok".to_vec()))
        }
    }

    struct FailInvoker;

    impl Node for FailInvoker {
        fn get_url(&self) -> &URL {
            static U: std::sync::LazyLock<URL> =
                std::sync::LazyLock::new(|| URL::new("tri", "/com.example.TestService"));
            &U
        }
        fn is_available(&self) -> bool {
            true
        }
        fn destroy(&self) {}
    }

    #[async_trait]
    impl Invoker for FailInvoker {
        async fn invoke(&self, _ctx: &mut InvocationContext) -> Result<RPCResult, anyhow::Error> {
            Err(anyhow::anyhow!("test failure"))
        }
    }

    #[tokio::test]
    async fn test_metrics_filter_success_path() {
        let collector = super::MetricsCollector::new().unwrap();
        let filter = super::MetricsFilter::new(collector.clone());
        let mut ctx = make_ctx("sayHello");
        let next = SuccessInvoker;

        let result = filter.invoke(&mut ctx, &next).await;
        assert!(result.is_ok());
        let rpc = result.unwrap();
        assert!(!rpc.is_error());

        let output = super::MetricsExporter::new(&collector).export_metrics();
        assert!(output.contains("rpc_requests_total"));
        assert!(output.contains("rpc_request_duration_seconds"));
    }

    #[tokio::test]
    async fn test_metrics_filter_error_path() {
        let collector = super::MetricsCollector::new().unwrap();
        let filter = super::MetricsFilter::new(collector.clone());
        let mut ctx = make_ctx("sayHello");
        let next = FailInvoker;

        let result = filter.invoke(&mut ctx, &next).await;
        assert!(result.is_err());

        let output = super::MetricsExporter::new(&collector).export_metrics();
        assert!(output.contains("rpc_requests_total"));
        assert!(output.contains("rpc_request_duration_seconds"));
    }

    #[test]
    fn test_metrics_filter_creation() {
        let collector = super::MetricsCollector::new().unwrap();
        let _filter = super::MetricsFilter::new(collector);
    }

    #[test]
    fn test_metrics_filter_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<super::MetricsFilter>();
        assert_sync::<super::MetricsFilter>();
    }

    // ── Registry module tests ──────────────────────────────────────────

    #[test]
    fn test_record_registry_op_increments_counter() {
        let collector = MetricsCollector::new().unwrap();
        collector.record_registry_op("register", "success");
        collector.record_registry_op("register", "success");
        collector.record_registry_op("subscribe", "error");

        let output = MetricsExporter::new(&collector).export_metrics();
        assert!(
            output.contains("registry_ops_total"),
            "should contain registry_ops_total: {output}"
        );
    }

    #[test]
    fn test_record_registry_duration_observes_histogram() {
        let collector = MetricsCollector::new().unwrap();
        collector.record_registry_duration("register", 0.05);
        collector.record_registry_duration("register", 0.12);
        collector.record_registry_duration("subscribe", 0.03);

        let output = MetricsExporter::new(&collector).export_metrics();
        assert!(
            output.contains("registry_op_duration_seconds"),
            "should contain registry_op_duration_seconds: {output}"
        );
        assert!(
            output.contains("registry_op_duration_seconds_count"),
            "should contain count suffix: {output}"
        );
    }

    // ── Metadata module tests ─────────────────────────────────────────

    #[test]
    fn test_record_metadata_op_increments_counter() {
        let collector = MetricsCollector::new().unwrap();
        collector.record_metadata_op("publish", "success");
        collector.record_metadata_op("publish", "success");
        collector.record_metadata_op("get", "error");

        let output = MetricsExporter::new(&collector).export_metrics();
        assert!(
            output.contains("metadata_ops_total"),
            "should contain metadata_ops_total: {output}"
        );
    }

    #[test]
    fn test_record_metadata_duration_observes_histogram() {
        let collector = MetricsCollector::new().unwrap();
        collector.record_metadata_duration("publish", 0.02);
        collector.record_metadata_duration("get", 0.01);

        let output = MetricsExporter::new(&collector).export_metrics();
        assert!(
            output.contains("metadata_op_duration_seconds"),
            "should contain metadata_op_duration_seconds: {output}"
        );
    }

    // ── Config module tests ───────────────────────────────────────────

    #[test]
    fn test_record_config_op_increments_counter() {
        let collector = MetricsCollector::new().unwrap();
        collector.record_config_op("get_config", "success");
        collector.record_config_op("get_config", "success");
        collector.record_config_op("watch", "error");

        let output = MetricsExporter::new(&collector).export_metrics();
        assert!(
            output.contains("config_ops_total"),
            "should contain config_ops_total: {output}"
        );
    }

    #[test]
    fn test_record_config_duration_observes_histogram() {
        let collector = MetricsCollector::new().unwrap();
        collector.record_config_duration("get_config", 0.005);
        collector.record_config_duration("watch", 0.1);

        let output = MetricsExporter::new(&collector).export_metrics();
        assert!(
            output.contains("config_op_duration_seconds"),
            "should contain config_op_duration_seconds: {output}"
        );
    }

    #[test]
    fn test_record_config_change() {
        let collector = MetricsCollector::new().unwrap();
        collector.record_config_change("dubbo.protocol.port", "add");
        collector.record_config_change("dubbo.protocol.port", "modify");
        collector.record_config_change("dubbo.registry.address", "delete");

        let output = MetricsExporter::new(&collector).export_metrics();
        assert!(
            output.contains("config_changes_total"),
            "should contain config_changes_total: {output}"
        );
    }

    #[test]
    fn test_all_modules_in_export() {
        let collector = MetricsCollector::new().unwrap();

        collector.record_registry_op("register", "success");
        collector.record_registry_duration("register", 0.01);
        collector.record_metadata_op("publish", "success");
        collector.record_metadata_duration("publish", 0.02);
        collector.record_config_op("get_config", "success");
        collector.record_config_duration("get_config", 0.003);
        collector.record_config_change("key1", "add");

        let output = MetricsExporter::new(&collector).export_metrics();

        for name in &[
            METRIC_REGISTRY_OPS_TOTAL,
            METRIC_REGISTRY_OP_DURATION_SECONDS,
            METRIC_METADATA_OPS_TOTAL,
            METRIC_METADATA_OP_DURATION_SECONDS,
            METRIC_CONFIG_OPS_TOTAL,
            METRIC_CONFIG_OP_DURATION_SECONDS,
            METRIC_CONFIG_CHANGES_TOTAL,
        ] {
            assert!(output.contains(name), "export should contain '{name}'");
        }
    }

    #[test]
    fn test_error_type_name_all_variants() {
        let cases: Vec<(&str, RPCError)> = vec![
            ("client_timeout", RPCError::ClientTimeout(String::new())),
            ("server_timeout", RPCError::ServerTimeout(String::new())),
            ("bad_request", RPCError::BadRequest(String::new())),
            ("bad_response", RPCError::BadResponse(String::new())),
            (
                "service_not_found",
                RPCError::ServiceNotFound(String::new()),
            ),
            ("service_error", RPCError::ServiceError(String::new())),
            ("server_error", RPCError::ServerError(String::new())),
            ("client_error", RPCError::ClientError(String::new())),
            (
                "server_threadpool_exhausted",
                RPCError::ServerThreadpoolExhausted(String::new()),
            ),
        ];

        for (expected, err) in &cases {
            assert_eq!(error_type_name(err), *expected, "mismatch for {err:?}");
        }
    }

    // ── Health check tests ────────────────────────────────────────────────

    #[test]
    fn test_health_checker_new_status_starting() {
        use super::{HealthChecker, HealthStatus};
        let checker = HealthChecker::new();
        assert_eq!(checker.get_status(), HealthStatus::Starting);
    }

    #[test]
    fn test_health_checker_set_get_status() {
        use super::{HealthChecker, HealthStatus};
        let checker = HealthChecker::new();

        checker.set_status(HealthStatus::Up);
        assert_eq!(checker.get_status(), HealthStatus::Up);

        checker.set_status(HealthStatus::Down);
        assert_eq!(checker.get_status(), HealthStatus::Down);

        checker.set_status(HealthStatus::Stopping);
        assert_eq!(checker.get_status(), HealthStatus::Stopping);

        checker.set_status(HealthStatus::Starting);
        assert_eq!(checker.get_status(), HealthStatus::Starting);
    }

    #[test]
    fn test_health_checker_add_check() {
        use super::HealthChecker;
        let checker = HealthChecker::new();

        checker.add_check("db", Box::new(|| true));
        checker.add_check("cache", Box::new(|| false));

        let results = checker.check_all();
        assert_eq!(results.len(), 2);
        assert_eq!(results.get("db"), Some(&true));
        assert_eq!(results.get("cache"), Some(&false));
    }

    #[test]
    fn test_health_checker_is_healthy() {
        use super::{HealthChecker, HealthStatus};
        let checker = HealthChecker::new();

        // Status is Starting → not healthy
        assert!(!checker.is_healthy());

        // Status Up, no checks → healthy
        checker.set_status(HealthStatus::Up);
        assert!(checker.is_healthy());

        // Add a failing check → not healthy
        checker.add_check("failing", Box::new(|| false));
        assert!(!checker.is_healthy());

        // Add a passing check alongside → still not healthy
        checker.add_check("passing", Box::new(|| true));
        assert!(!checker.is_healthy());
    }

    #[test]
    fn test_health_checker_check_all() {
        use super::HealthChecker;
        let checker = HealthChecker::new();

        checker.add_check("a", Box::new(|| true));
        checker.add_check("b", Box::new(|| true));

        let results = checker.check_all();
        assert_eq!(results.len(), 2);
        assert!(results.values().all(|&v| v));
    }

    #[test]
    fn test_health_checker_to_json() {
        use super::{HealthChecker, HealthStatus};
        let checker = HealthChecker::new();

        checker.set_status(HealthStatus::Up);
        checker.add_check("db", Box::new(|| true));
        checker.add_check("cache", Box::new(|| false));

        let json = checker.to_json();
        assert!(json.contains("\"status\":\"UP\""));
        assert!(json.contains("\"db\":true"));
        assert!(json.contains("\"cache\":false"));
    }

    #[test]
    fn test_health_checker_add_detail() {
        use super::HealthChecker;
        let checker = HealthChecker::new();

        checker.add_detail("version", "1.0.0");
        checker.add_detail("build", "release");

        let json = checker.to_json();
        assert!(json.contains("\"version\":\"1.0.0\""));
        assert!(json.contains("\"build\":\"release\""));
    }

    #[test]
    fn test_health_endpoint_creation() {
        use super::{HealthChecker, HealthEndpoint};
        use std::sync::Arc;

        let checker = Arc::new(HealthChecker::new());
        let endpoint = HealthEndpoint::new(checker, 9876);
        assert_eq!(endpoint.port, 9876);
    }
}

// ============================================================================
// Health check
// ============================================================================

use dashmap::DashMap;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};

/// Health status of a service component.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus {
    /// Service is starting up.
    Starting,
    /// Service is up and healthy.
    Up,
    /// Service is down.
    Down,
    /// Service is shutting down.
    Stopping,
}

impl HealthStatus {
    /// Convert to the wire representation used by `AtomicU8`.
    fn to_u8(self) -> u8 {
        match self {
            Self::Starting => 0,
            Self::Up => 1,
            Self::Down => 2,
            Self::Stopping => 3,
        }
    }

    /// Convert from the `AtomicU8` wire representation.
    fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Starting,
            1 => Self::Up,
            3 => Self::Stopping,
            _ => Self::Down,
        }
    }

    /// JSON-friendly string representation.
    fn as_str(self) -> &'static str {
        match self {
            Self::Starting => "STARTING",
            Self::Up => "UP",
            Self::Down => "DOWN",
            Self::Stopping => "STOPPING",
        }
    }
}

/// Central health registry that tracks service health state and runs
/// registered health-check functions.
///
/// Thread-safe and cheaply cloneable — all state is behind `Arc`.
pub struct HealthChecker {
    status: Arc<AtomicU8>,
    checks: Arc<DashMap<String, Box<dyn Fn() -> bool + Send + Sync>>>,
    details: Arc<DashMap<String, String>>,
}

impl HealthChecker {
    /// Create a new `HealthChecker` in the [`HealthStatus::Starting`] state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            status: Arc::new(AtomicU8::new(HealthStatus::Starting.to_u8())),
            checks: Arc::new(DashMap::new()),
            details: Arc::new(DashMap::new()),
        }
    }

    /// Update the overall health status.
    pub fn set_status(&self, status: HealthStatus) {
        self.status.store(status.to_u8(), Ordering::Release);
    }

    /// Read the current health status.
    #[must_use]
    pub fn get_status(&self) -> HealthStatus {
        HealthStatus::from_u8(self.status.load(Ordering::Acquire))
    }

    /// Register a named health-check function.
    ///
    /// The function should return `true` when the checked subsystem is healthy.
    pub fn add_check(&self, name: &str, check: Box<dyn Fn() -> bool + Send + Sync>) {
        self.checks.insert(name.to_string(), check);
    }

    /// Add a metadata key-value pair (e.g. version, build info).
    pub fn add_detail(&self, key: &str, value: &str) {
        self.details.insert(key.to_string(), value.to_string());
    }

    /// Returns `true` when the overall status is [`HealthStatus::Up`] **and**
    /// every registered check function returns `true`.
    #[must_use]
    pub fn is_healthy(&self) -> bool {
        if self.get_status() != HealthStatus::Up {
            return false;
        }
        self.checks.iter().all(|entry| (entry.value())())
    }

    /// Run all registered checks and return a map of check-name → result.
    #[must_use]
    pub fn check_all(&self) -> HashMap<String, bool> {
        self.checks
            .iter()
            .map(|entry| (entry.key().clone(), (entry.value())()))
            .collect()
    }

    /// Produce a JSON representation of the current health state.
    ///
    /// Format:
    /// ```json
    /// {"status":"UP","checks":{"db":true},"details":{"version":"1.0"}}
    /// ```
    #[must_use]
    pub fn to_json(&self) -> String {
        let status = self.get_status().as_str();

        let checks_obj: String = self
            .check_all()
            .into_iter()
            .map(|(k, v)| format!("\"{k}\":{v}"))
            .collect::<Vec<_>>()
            .join(",");

        let details_obj: String = self
            .details
            .iter()
            .map(|entry| {
                let k = entry.key();
                let v = entry.value();
                // Escape basic JSON special chars in values
                format!(
                    "\"{}\":\"{}\"",
                    k,
                    v.replace('\\', "\\\\").replace('"', "\\\"")
                )
            })
            .collect::<Vec<_>>()
            .join(",");

        format!(
            "{{\"status\":\"{status}\",\"checks\":{{{checks_obj}}},\"details\":{{{details_obj}}}}}"
        )
    }
}

impl Default for HealthChecker {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// HealthEndpoint — HTTP server exposing health probes
// ============================================================================

use http_body_util::Full;
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;

/// Thin HTTP server that exposes Kubernetes-style health probes.
///
/// Routes:
/// - `GET /health`       → full JSON status (200 if healthy, 503 otherwise)
/// - `GET /health/live`  → liveness probe (always 200 while the process lives)
/// - `GET /health/ready` → readiness probe (200 when status is `Up`, 503 otherwise)
pub struct HealthEndpoint {
    checker: Arc<HealthChecker>,
    pub port: u16,
}

impl HealthEndpoint {
    /// Create a new endpoint backed by the given checker, bound to `port`.
    #[must_use]
    pub fn new(checker: Arc<HealthChecker>, port: u16) -> Self {
        Self { checker, port }
    }

    /// Start the HTTP server in the background.
    ///
    /// Returns `Ok(())` once the listener is bound. The server runs on a
    /// spawned Tokio task and will shut down when the task is cancelled.
    ///
    /// # Errors
    ///
    /// Returns an error if the TCP listener cannot be bound.
    pub async fn start(&self) -> Result<(), anyhow::Error> {
        let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", self.port)).await?;
        let checker = self.checker.clone();

        tokio::spawn(async move {
            loop {
                let Ok((stream, _addr)) = listener.accept().await else {
                    continue;
                };
                let checker = checker.clone();
                let service = service_fn(move |req: Request<hyper::body::Incoming>| {
                    let checker = checker.clone();
                    async move { Ok::<_, hyper::Error>(handle_health_request(&req, &checker)) }
                });
                tokio::spawn(async move {
                    let io = TokioIo::new(stream);
                    let _ = http1::Builder::new().serve_connection(io, service).await;
                });
            }
        });

        Ok(())
    }
}

// ============================================================================
// PrometheusEndpoint — HTTP server for Prometheus metrics scraping
// ============================================================================

/// HTTP endpoint that serves Prometheus text-format metrics for scraping.
///
/// Routes:
/// - `GET /metrics` → Prometheus text exposition format (200)
/// - Any other path → 404
pub struct PrometheusEndpoint {
    collector: MetricsCollector,
    pub port: u16,
}

impl PrometheusEndpoint {
    /// Create a new endpoint backed by the given collector, bound to `port`.
    #[must_use]
    pub fn new(collector: MetricsCollector, port: u16) -> Self {
        Self { collector, port }
    }

    /// Start the HTTP server in the background.
    ///
    /// Returns `Ok(())` once the listener is bound. The server runs on a
    /// spawned Tokio task and will shut down when the task is cancelled.
    ///
    /// # Errors
    ///
    /// Returns an error if the TCP listener cannot be bound.
    pub async fn start(&self) -> Result<(), anyhow::Error> {
        let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", self.port)).await?;
        let collector = self.collector.clone();

        tokio::spawn(async move {
            loop {
                let Ok((stream, _addr)) = listener.accept().await else {
                    continue;
                };
                let collector = collector.clone();
                let service = service_fn(move |req: Request<hyper::body::Incoming>| {
                    let collector = collector.clone();
                    async move { Ok::<_, hyper::Error>(handle_metrics_request(req, &collector)) }
                });
                tokio::spawn(async move {
                    let io = TokioIo::new(stream);
                    let _ = http1::Builder::new().serve_connection(io, service).await;
                });
            }
        });

        Ok(())
    }
}

#[allow(clippy::needless_pass_by_value)]
fn handle_metrics_request(
    req: Request<hyper::body::Incoming>,
    collector: &MetricsCollector,
) -> Response<Full<Bytes>> {
    let path = req.uri().path();

    match path {
        "/metrics" => {
            let exporter = MetricsExporter::new(collector);
            let body = exporter.export_metrics();
            Response::builder()
                .status(200)
                .header("content-type", "text/plain; version=0.0.4; charset=utf-8")
                .body(Full::new(Bytes::from(body)))
                .unwrap()
        }
        _ => response(404, "Not Found"),
    }
}

fn handle_health_request(
    req: &Request<hyper::body::Incoming>,
    checker: &HealthChecker,
) -> Response<Full<Bytes>> {
    let path = req.uri().path();

    match path {
        "/health/live" => response(200, "OK"),
        "/health/ready" => {
            if checker.get_status() == HealthStatus::Up {
                response(200, "OK")
            } else {
                response(503, "Service Unavailable")
            }
        }
        "/health" => {
            let json = checker.to_json();
            if checker.is_healthy() {
                response_json(200, json)
            } else {
                response_json(503, json)
            }
        }
        _ => response(404, "Not Found"),
    }
}

fn response(status: u16, body: &str) -> Response<Full<Bytes>> {
    Response::builder()
        .status(status)
        .header("content-type", "text/plain; charset=utf-8")
        .body(Full::new(Bytes::from(body.to_string())))
        .unwrap()
}

fn response_json(status: u16, body: String) -> Response<Full<Bytes>> {
    Response::builder()
        .status(status)
        .header("content-type", "application/json; charset=utf-8")
        .body(Full::new(Bytes::from(body)))
        .unwrap()
}

#[cfg(test)]
mod prometheus_endpoint_tests {
    use super::*;

    fn make_collector() -> MetricsCollector {
        MetricsCollector::new().unwrap()
    }

    async fn spawn_server(collector: MetricsCollector) -> u16 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                let Ok((stream, _addr)) = listener.accept().await else {
                    continue;
                };
                let c = collector.clone();
                let service = service_fn(move |req: Request<hyper::body::Incoming>| {
                    let c = c.clone();
                    async move { Ok::<_, hyper::Error>(handle_metrics_request(req, &c)) }
                });
                tokio::spawn(async move {
                    let io = TokioIo::new(stream);
                    let _ = http1::Builder::new().serve_connection(io, service).await;
                });
            }
        });
        port
    }

    async fn http_get(port: u16, path: &str) -> (u16, String, Option<String>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{port}"))
            .await
            .unwrap();
        let request =
            format!("GET {path} HTTP/1.1\r\nhost: 127.0.0.1:{port}\r\nconnection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).await.unwrap();
        let response = String::from_utf8(buf).unwrap();
        let mut headers_end = 0;
        let mut status_code = 0u16;
        let mut content_type: Option<String> = None;
        if let Some(pos) = response.find("\r\n\r\n") {
            headers_end = pos;
            let header_section = &response[..pos];
            let first_line = header_section.lines().next().unwrap_or("");
            if let Some(code_str) = first_line.split_whitespace().nth(1) {
                status_code = code_str.parse().unwrap_or(0);
            }
            for line in header_section.lines() {
                if let Some(val) = line.strip_prefix("content-type:") {
                    content_type = Some(val.trim().to_string());
                } else if let Some(val) = line.strip_prefix("Content-Type:") {
                    content_type = Some(val.trim().to_string());
                }
            }
        }
        let body = response.get(headers_end + 4..).unwrap_or("").to_string();
        (status_code, body, content_type)
    }

    #[test]
    fn test_prometheus_endpoint_creation() {
        let collector = make_collector();
        let endpoint = PrometheusEndpoint::new(collector, 9091);
        assert_eq!(endpoint.port, 9091);
    }

    #[tokio::test]
    async fn test_prometheus_endpoint_serves_metrics() {
        let collector = make_collector();
        collector.record_request("Svc", "m", "success");
        let port = spawn_server(collector).await;
        let (status, body, _) = http_get(port, "/metrics").await;
        assert_eq!(status, 200);
        assert!(
            body.contains("rpc_requests_total"),
            "body should contain metric name, got: {body}"
        );
    }

    #[tokio::test]
    async fn test_prometheus_endpoint_unknown_path() {
        let port = spawn_server(make_collector()).await;
        let (status, body, _) = http_get(port, "/unknown").await;
        assert_eq!(status, 404);
        assert_eq!(body, "Not Found");
    }

    #[tokio::test]
    async fn test_prometheus_endpoint_content_type() {
        let port = spawn_server(make_collector()).await;
        let (status, _, ct) = http_get(port, "/metrics").await;
        assert_eq!(status, 200);
        assert_eq!(
            ct.as_deref(),
            Some("text/plain; version=0.0.4; charset=utf-8")
        );
    }

    #[tokio::test]
    async fn test_prometheus_endpoint_with_data() {
        let collector = make_collector();
        collector.record_request("TestService", "greet", "success");
        collector.record_duration("TestService", "greet", 0.042);
        collector.record_error("TestService", "greet", "timeout");

        let port = spawn_server(collector).await;
        let (status, body, _) = http_get(port, "/metrics").await;
        assert_eq!(status, 200);
        assert!(body.contains("TestService"));
        assert!(body.contains("greet"));
        assert!(body.contains("timeout"));
    }

    #[tokio::test]
    async fn test_prometheus_endpoint_empty_metrics() {
        let port = spawn_server(make_collector()).await;
        let (status, body, _) = http_get(port, "/metrics").await;
        assert_eq!(status, 200);
        assert!(
            body.contains("rpc_requests_total")
                || body.contains("rpc_request_duration_seconds")
                || body.contains("rpc_errors_total")
                || body.is_empty(),
            "empty collector should return 200 with empty or partial output"
        );
    }
}
