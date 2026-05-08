pub use dubbo_rs_common;
pub use dubbo_rs_filter;
pub use dubbo_rs_protocol;

use async_trait::async_trait;
use dubbo_rs_filter::Filter;
use dubbo_rs_protocol::{InvocationContext, Invoker, RPCResult};
use opentelemetry::trace::{
    Span as _, SpanContext, SpanId, TraceContextExt, TraceFlags, TraceId, TraceState, Tracer,
    TracerProvider as _,
};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::IdGenerator;
use std::sync::OnceLock;

pub trait TraceContextPropagator: Send + Sync {
    fn extract(&self, ctx: &InvocationContext) -> Option<String>;
    fn inject(&self, ctx: &mut InvocationContext, traceparent: &str);
}

#[derive(Debug, Clone, Default)]
pub struct W3CPropagator;

impl TraceContextPropagator for W3CPropagator {
    fn extract(&self, ctx: &InvocationContext) -> Option<String> {
        ctx.attachments.get("traceparent").cloned()
    }

    fn inject(&self, ctx: &mut InvocationContext, traceparent: &str) {
        ctx.attachments
            .insert("traceparent".into(), traceparent.into());
    }
}

#[derive(Debug, Clone, Default)]
pub enum ExporterType {
    #[default]
    Otlp,
    Stdout,
}

#[derive(Debug, Clone)]
pub struct TracingConfig {
    pub endpoint: Option<String>,
    pub sample_rate: f64,
    pub enable_trace_id_log: bool,
    pub exporter: ExporterType,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            endpoint: None,
            sample_rate: 1.0,
            enable_trace_id_log: true,
            exporter: ExporterType::default(),
        }
    }
}

impl TracingConfig {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = Some(endpoint.into());
        self
    }

    #[must_use]
    pub fn with_sample_rate(mut self, rate: f64) -> Self {
        self.sample_rate = rate.clamp(0.0, 1.0);
        self
    }

    #[must_use]
    pub fn with_trace_id_log(mut self, enable: bool) -> Self {
        self.enable_trace_id_log = enable;
        self
    }

    #[must_use]
    pub fn with_exporter(mut self, exporter: ExporterType) -> Self {
        self.exporter = exporter;
        self
    }
}

pub struct TracingFilter {
    config: TracingConfig,
    propagator: Box<dyn TraceContextPropagator>,
    id_generator: opentelemetry_sdk::trace::RandomIdGenerator,
    tracer_provider: OnceLock<opentelemetry_sdk::trace::SdkTracerProvider>,
}

impl TracingFilter {
    #[must_use]
    pub fn new(config: TracingConfig) -> Self {
        Self::new_with_propagator(config, W3CPropagator)
    }

    #[must_use]
    pub fn new_with_propagator(
        config: TracingConfig,
        propagator: impl TraceContextPropagator + 'static,
    ) -> Self {
        Self {
            config,
            propagator: Box::new(propagator),
            id_generator: opentelemetry_sdk::trace::RandomIdGenerator::default(),
            tracer_provider: OnceLock::new(),
        }
    }

    fn is_sampled(&self) -> bool {
        self.config.sample_rate > 0.0
    }

    fn has_exporter(&self) -> bool {
        match &self.config.exporter {
            ExporterType::Otlp => self.config.endpoint.is_some(),
            ExporterType::Stdout => true,
        }
    }

    fn ensure_provider(&self) {
        let should_init = match &self.config.exporter {
            ExporterType::Otlp => self.config.endpoint.is_some(),
            ExporterType::Stdout => true,
        };
        if should_init {
            self.tracer_provider
                .get_or_init(|| init_provider(&self.config));
        }
    }

    fn should_sample(&self) -> bool {
        if self.config.sample_rate >= 1.0 {
            return true;
        }
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        (f64::from(nanos) / 1_000_000_000.0) < self.config.sample_rate
    }
}

fn init_provider(config: &TracingConfig) -> opentelemetry_sdk::trace::SdkTracerProvider {
    match &config.exporter {
        ExporterType::Otlp => {
            let endpoint = config
                .endpoint
                .as_deref()
                .expect("OTLP exporter requires an endpoint");
            let exporter = opentelemetry_otlp::SpanExporter::builder()
                .with_tonic()
                .with_endpoint(endpoint)
                .build()
                .expect("failed to build OTLP span exporter");

            opentelemetry_sdk::trace::SdkTracerProvider::builder()
                .with_batch_exporter(exporter)
                .build()
        }
        ExporterType::Stdout => {
            let exporter = opentelemetry_stdout::SpanExporter::default();
            opentelemetry_sdk::trace::SdkTracerProvider::builder()
                .with_batch_exporter(exporter)
                .build()
        }
    }
}

const TRACEPARENT_VERSION: &str = "00";
const SAMPLED_FLAG: &str = "01";
const NOT_SAMPLED_FLAG: &str = "00";

fn parse_traceparent(tp: &str) -> Option<(TraceId, SpanId)> {
    let mut parts = tp.splitn(4, '-');
    if parts.next() != Some(TRACEPARENT_VERSION) {
        return None;
    }
    let trace_hex = parts.next()?;
    let span_hex = parts.next()?;

    if trace_hex.len() != 32 || span_hex.len() != 16 {
        return None;
    }

    let tid = hex_to_trace_id(trace_hex)?;
    let sid = hex_to_span_id(span_hex)?;
    Some((tid, sid))
}

fn hex_to_trace_id(hex: &str) -> Option<TraceId> {
    let mut bytes = [0u8; 16];
    hex_to_bytes(hex, &mut bytes)?;
    Some(TraceId::from_bytes(bytes))
}

fn hex_to_span_id(hex: &str) -> Option<SpanId> {
    let mut bytes = [0u8; 8];
    hex_to_bytes(hex, &mut bytes)?;
    Some(SpanId::from_bytes(bytes))
}

fn hex_to_bytes(hex: &str, dst: &mut [u8]) -> Option<()> {
    if hex.len() != dst.len() * 2 {
        return None;
    }
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        let hi = hex_val(chunk[0])?;
        let lo = hex_val(chunk[1])?;
        dst[i] = hi << 4 | lo;
    }
    Some(())
}

const fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    }
}

fn trace_id_to_hex(tid: &TraceId) -> String {
    bytes_to_hex(&tid.to_bytes())
}

fn span_id_to_hex(sid: SpanId) -> String {
    bytes_to_hex(&sid.to_bytes())
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
    }
    s
}

fn build_traceparent(tid: &TraceId, sid: SpanId, sampled: bool) -> String {
    let flag = if sampled {
        SAMPLED_FLAG
    } else {
        NOT_SAMPLED_FLAG
    };
    format!(
        "{}-{}-{}-{flag}",
        TRACEPARENT_VERSION,
        trace_id_to_hex(tid),
        span_id_to_hex(sid)
    )
}

#[async_trait]
impl Filter for TracingFilter {
    async fn invoke(
        &self,
        ctx: &mut InvocationContext,
        next: &dyn Invoker,
    ) -> Result<RPCResult, anyhow::Error> {
        let incoming_tp = self.propagator.extract(ctx);
        let (trace_id, parent_span_id) = match incoming_tp.as_deref().and_then(parse_traceparent) {
            Some((tid, psid)) => (tid, Some(psid)),
            None => (self.id_generator.new_trace_id(), None),
        };

        let span_id = self.id_generator.new_span_id();
        let sampled = self.should_sample();

        let outgoing_tp = build_traceparent(&trace_id, span_id, sampled);
        self.propagator.inject(ctx, &outgoing_tp);

        if !self.is_sampled() && !self.has_exporter() {
            return next.invoke(ctx).await;
        }

        let service = ctx.url.path.clone();
        let method = ctx.method_name.clone();
        let tid_hex = trace_id_to_hex(&trace_id);

        let span = if self.config.enable_trace_id_log {
            tracing::info_span!(
                "rpc_call",
                service = %service,
                method = %method,
                trace_id = %tid_hex,
            )
        } else {
            tracing::info_span!("rpc_call", service = %service, method = %method)
        };
        let _enter = span.enter();

        let mut otel_span = if self.has_exporter() && sampled {
            self.ensure_provider();
            Some(self.make_otel_span(
                &trace_id,
                parent_span_id.as_ref(),
                span_id,
                &service,
                &method,
            ))
        } else {
            None
        };

        let result = next.invoke(ctx).await;

        match &result {
            Ok(rpc_result) if rpc_result.is_error() => {
                tracing::error!(method = %method, "RPC call returned error");
                if let Some(ref mut s) = otel_span {
                    s.set_status(opentelemetry::trace::Status::error("RPC error"));
                }
            }
            Err(e) => {
                tracing::error!(method = %method, error = %e, "RPC call failed");
                if let Some(ref mut s) = otel_span {
                    let msg = format!("{e:#}");
                    s.set_status(opentelemetry::trace::Status::error(msg));
                }
            }
            Ok(_) => {}
        }

        if let Some(mut s) = otel_span {
            s.end();
        }

        result
    }
}

impl TracingFilter {
    fn make_otel_span(
        &self,
        trace_id: &TraceId,
        parent_span_id: Option<&SpanId>,
        span_id: SpanId,
        service: &str,
        method: &str,
    ) -> opentelemetry_sdk::trace::Span {
        let provider = self
            .tracer_provider
            .get()
            .expect("OTLP provider not initialised");
        let tracer: opentelemetry_sdk::trace::Tracer = provider.tracer("dubbo-rs-tracing");

        let mut builder = opentelemetry::trace::SpanBuilder::from_name("rpc_call");
        builder.span_id = Some(span_id);
        builder.trace_id = Some(*trace_id);
        builder.attributes = Some(vec![
            opentelemetry::KeyValue::new("service", service.to_string()),
            opentelemetry::KeyValue::new("method", method.to_string()),
        ]);

        match parent_span_id {
            Some(psid) => {
                let parent_ctx =
                    opentelemetry::Context::new().with_remote_span_context(SpanContext::new(
                        *trace_id,
                        *psid,
                        TraceFlags::SAMPLED,
                        true,
                        TraceState::default(),
                    ));
                tracer.build_with_context(builder, &parent_ctx)
            }
            None => tracer.build(builder),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dubbo_rs_common::node::Node;
    use dubbo_rs_common::url::URL;

    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;

    struct TestInvoker {
        url: URL,
        call_count: Arc<AtomicUsize>,
        should_fail: Arc<AtomicBool>,
        captured: Arc<std::sync::Mutex<Vec<std::collections::HashMap<String, String>>>>,
    }

    impl TestInvoker {
        fn new() -> Self {
            Self {
                url: URL::new("tri", "/com.example.TestService"),
                call_count: Arc::new(AtomicUsize::new(0)),
                should_fail: Arc::new(AtomicBool::new(false)),
                captured: Arc::new(std::sync::Mutex::new(Vec::new())),
            }
        }

        fn count(&self) -> usize {
            self.call_count.load(Ordering::SeqCst)
        }

        fn captured_attachments(&self) -> Vec<std::collections::HashMap<String, String>> {
            self.captured.lock().unwrap().clone()
        }
    }

    impl Node for TestInvoker {
        fn get_url(&self) -> &URL {
            &self.url
        }

        fn is_available(&self) -> bool {
            true
        }

        fn destroy(&self) {}
    }

    #[async_trait]
    impl Invoker for TestInvoker {
        async fn invoke(&self, ctx: &mut InvocationContext) -> Result<RPCResult, anyhow::Error> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            self.captured.lock().unwrap().push(ctx.attachments.clone());
            if self.should_fail.load(Ordering::SeqCst) {
                Ok(RPCResult::from_error(
                    dubbo_rs_common::error::RPCError::ServerError("test failure".into()),
                ))
            } else {
                Ok(RPCResult::success(b"ok".to_vec()))
            }
        }
    }

    fn make_ctx(method: &str) -> InvocationContext {
        let url = URL::new("tri", "/com.example.TestService");
        InvocationContext::new(method, url)
    }

    #[tokio::test]
    async fn test_span_context_generated_and_propagated() {
        let config = TracingConfig::new();
        let filter = TracingFilter::new(config);
        let next = TestInvoker::new();
        let mut ctx = make_ctx("sayHello");

        let result = filter.invoke(&mut ctx, &next).await.unwrap();
        assert!(!result.is_error());

        let tp = ctx
            .attachments
            .get("traceparent")
            .expect("traceparent should be injected");
        assert!(tp.starts_with("00-"));
        assert_eq!(tp.len(), 55, "traceparent should be 55 chars: 00-32-16-2");

        let captured = next.captured_attachments();
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].get("traceparent"), Some(tp));
    }

    #[tokio::test]
    async fn test_context_propagation_roundtrip() {
        let config = TracingConfig::new();
        let filter = TracingFilter::new(config);
        let next = TestInvoker::new();
        let mut ctx = make_ctx("sayHello");

        ctx.attachments.insert(
            "traceparent".into(),
            "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01".into(),
        );

        let result = filter.invoke(&mut ctx, &next).await.unwrap();
        assert!(!result.is_error());

        let outgoing = ctx.attachments.get("traceparent").unwrap();
        assert!(
            outgoing.starts_with("00-0af7651916cd43dd8448eb211c80319c-"),
            "trace_id should be preserved, got: {outgoing}"
        );
    }

    #[tokio::test]
    async fn test_tracing_disabled_when_sample_rate_zero() {
        let config = TracingConfig::new().with_sample_rate(0.0);
        let filter = TracingFilter::new(config);
        let next = TestInvoker::new();
        let mut ctx = make_ctx("sayHello");

        let result = filter.invoke(&mut ctx, &next).await.unwrap();
        assert!(!result.is_error());
        assert_eq!(next.count(), 1);
    }

    #[tokio::test]
    async fn test_attachments_preserved() {
        let config = TracingConfig::new();
        let filter = TracingFilter::new(config);
        let next = TestInvoker::new();
        let mut ctx = make_ctx("sayHello");

        ctx.attachments
            .insert("custom-key".into(), "custom-value".into());

        let result = filter.invoke(&mut ctx, &next).await.unwrap();
        assert!(!result.is_error());

        assert!(ctx.attachments.contains_key("traceparent"));
        assert_eq!(
            ctx.attachments.get("custom-key"),
            Some(&"custom-value".into())
        );
    }

    #[test]
    fn test_tracing_config_defaults() {
        let config = TracingConfig::default();
        assert!(config.endpoint.is_none());
        assert!((config.sample_rate - 1.0).abs() < f64::EPSILON);
        assert!(config.enable_trace_id_log);
    }

    #[test]
    fn test_tracing_config_builder() {
        let config = TracingConfig::new()
            .with_endpoint("http://localhost:4317")
            .with_sample_rate(0.5)
            .with_trace_id_log(false);

        assert_eq!(config.endpoint.as_deref(), Some("http://localhost:4317"));
        assert!((config.sample_rate - 0.5).abs() < f64::EPSILON);
        assert!(!config.enable_trace_id_log);
    }

    #[test]
    fn test_tracing_config_sample_rate_clamped() {
        let config = TracingConfig::new()
            .with_sample_rate(1.5)
            .with_sample_rate(-0.2);
        assert!((config.sample_rate - 0.0).abs() < f64::EPSILON);
    }

    struct CustomHeaderPropagator;

    impl TraceContextPropagator for CustomHeaderPropagator {
        fn extract(&self, ctx: &InvocationContext) -> Option<String> {
            ctx.attachments.get("x-trace").cloned()
        }

        fn inject(&self, ctx: &mut InvocationContext, traceparent: &str) {
            ctx.attachments.insert("x-trace".into(), traceparent.into());
        }
    }

    #[tokio::test]
    async fn test_custom_propagator() {
        let config = TracingConfig::new();
        let filter = TracingFilter::new_with_propagator(config, CustomHeaderPropagator);
        let next = TestInvoker::new();
        let mut ctx = make_ctx("sayHello");

        let trace_id_hex = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let span_id_hex = "bbbbbbbbbbbbbbbb";
        ctx.attachments.insert(
            "x-trace".into(),
            format!("00-{trace_id_hex}-{span_id_hex}-01"),
        );

        let result = filter.invoke(&mut ctx, &next).await.unwrap();
        assert!(!result.is_error());

        assert!(
            !ctx.attachments.contains_key("traceparent"),
            "default traceparent key should not be set with custom propagator"
        );

        let injected = ctx.attachments.get("x-trace").unwrap();
        let expected_prefix = format!("00-{trace_id_hex}-");
        assert!(
            injected.starts_with(&expected_prefix),
            "custom propagator should preserve trace_id={trace_id_hex}, got: {injected}"
        );
    }

    #[tokio::test]
    async fn test_filter_handles_downstream_error() {
        let config = TracingConfig::new();
        let filter = TracingFilter::new(config);
        let next = TestInvoker::new();
        next.should_fail.store(true, Ordering::SeqCst);
        let mut ctx = make_ctx("sayHello");

        let result = filter.invoke(&mut ctx, &next).await.unwrap();
        assert!(result.is_error());
        assert_eq!(next.count(), 1);
        assert!(ctx.attachments.contains_key("traceparent"));
    }

    #[test]
    fn test_parse_valid_traceparent() {
        let tp = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";
        let (tid, sid) = parse_traceparent(tp).unwrap();
        assert_eq!(trace_id_to_hex(&tid), "0af7651916cd43dd8448eb211c80319c");
        assert_eq!(span_id_to_hex(sid), "b7ad6b7169203331");
    }

    #[test]
    fn test_parse_invalid_traceparent() {
        assert!(parse_traceparent("").is_none());
        assert!(parse_traceparent("00-short-16-01").is_none());
        assert!(
            parse_traceparent("01-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01").is_none()
        );
    }

    #[tokio::test]
    async fn test_span_generates_valid_w3c_format() {
        let config = TracingConfig::new();
        let filter = TracingFilter::new(config);
        let next = TestInvoker::new();
        let mut ctx = make_ctx("sayHello");

        let _result = filter.invoke(&mut ctx, &next).await.unwrap();

        let tp = ctx.attachments.get("traceparent").unwrap();
        let parts: Vec<&str> = tp.split('-').collect();
        assert_eq!(parts.len(), 4);
        assert_eq!(parts[0], "00");
        assert_eq!(parts[1].len(), 32);
        assert_eq!(parts[2].len(), 16);
        assert_eq!(parts[3].len(), 2);
    }

    #[tokio::test]
    async fn test_empty_attachments_no_crash() {
        let config = TracingConfig::new();
        let filter = TracingFilter::new(config);
        let next = TestInvoker::new();
        let mut ctx = make_ctx("sayHello");
        ctx.attachments.clear();

        let result = filter.invoke(&mut ctx, &next).await.unwrap();
        assert!(!result.is_error());
        assert!(ctx.attachments.contains_key("traceparent"));
    }

    #[tokio::test]
    async fn test_trace_id_log_disabled() {
        let config = TracingConfig::new().with_trace_id_log(false);
        let filter = TracingFilter::new(config);
        let next = TestInvoker::new();
        let mut ctx = make_ctx("sayHello");

        let result = filter.invoke(&mut ctx, &next).await.unwrap();
        assert!(!result.is_error());
        assert!(ctx.attachments.contains_key("traceparent"));
    }

    #[test]
    fn test_exporter_type_default_is_otlp() {
        let default = ExporterType::default();
        assert!(matches!(default, ExporterType::Otlp));
    }

    #[test]
    fn test_tracing_config_with_stdout_exporter() {
        let config = TracingConfig::new().with_exporter(ExporterType::Stdout);
        assert!(matches!(config.exporter, ExporterType::Stdout));
    }

    #[test]
    fn test_tracing_config_with_otlp_exporter() {
        let config = TracingConfig::new()
            .with_endpoint("http://localhost:4317")
            .with_exporter(ExporterType::Otlp);
        assert!(matches!(config.exporter, ExporterType::Otlp));
        assert_eq!(config.endpoint.as_deref(), Some("http://localhost:4317"));
    }

    #[tokio::test]
    async fn test_stdout_exporter_creates_spans() {
        let config = TracingConfig::new().with_exporter(ExporterType::Stdout);
        let filter = TracingFilter::new(config);
        let next = TestInvoker::new();
        let mut ctx = make_ctx("sayHello");

        let result = filter.invoke(&mut ctx, &next).await.unwrap();
        assert!(!result.is_error());

        let tp = ctx
            .attachments
            .get("traceparent")
            .expect("traceparent should be injected");
        assert!(tp.starts_with("00-"));
        assert_eq!(tp.len(), 55);
    }

    #[tokio::test]
    async fn test_filter_with_stdout_backend() {
        let config = TracingConfig::new()
            .with_exporter(ExporterType::Stdout)
            .with_sample_rate(1.0);
        let filter = TracingFilter::new(config);
        let next = TestInvoker::new();
        let mut ctx = make_ctx("sayHello");

        ctx.attachments.insert(
            "traceparent".into(),
            "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01".into(),
        );

        let result = filter.invoke(&mut ctx, &next).await.unwrap();
        assert!(!result.is_error());
        assert_eq!(next.count(), 1);

        let outgoing = ctx.attachments.get("traceparent").unwrap();
        assert!(
            outgoing.starts_with("00-0af7651916cd43dd8448eb211c80319c-"),
            "trace_id should be preserved with stdout backend, got: {outgoing}"
        );
    }

    #[test]
    fn test_exporter_type_debug_format() {
        assert_eq!(format!("{:?}", ExporterType::Otlp), "Otlp");
        assert_eq!(format!("{:?}", ExporterType::Stdout), "Stdout");
    }
}
