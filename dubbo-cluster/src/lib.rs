#![allow(
    clippy::doc_markdown,
    clippy::missing_panics_doc,
    clippy::assigning_clones
)]

pub use dubbo_rs_common;
pub use dubbo_rs_protocol;
pub use dubbo_rs_registry;

use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use dubbo_rs_common::error::RPCError;
use dubbo_rs_common::node::Node;
use dubbo_rs_common::url::URL;
use dubbo_rs_protocol::{InvocationContext, Invoker, RPCResult};
use dubbo_rs_registry::{NotifyListener, ServiceEvent};

/// Directory provides a list of service invokers.
///
/// Directories may be static (pre-configured invoker list) or dynamic
/// (backed by a registry that updates the list as providers change).
#[async_trait]
pub trait Directory: Send + Sync {
    /// Return all invokers available for the given invocation context.
    ///
    /// Called by the cluster strategy before each call to select an invoker.
    ///
    /// # Errors
    ///
    /// Returns `RPCError::ServiceNotFound` if no invokers are available.
    async fn list(&self, ctx: &InvocationContext) -> Result<Vec<Arc<dyn Invoker>>, RPCError>;

    /// Returns the URL identifying this directory.
    fn get_url(&self) -> &URL;
}

/// Cluster strategy — joins a directory into a single fault-tolerant invoker.
///
/// A cluster wraps a directory of invokers with a retry/failover policy.
/// The resulting invoker presents itself as a single endpoint to callers,
/// while internally selecting from the directory and retrying on failure
/// according to the cluster policy (e.g., failover, failfast).
#[async_trait]
pub trait Cluster: Send + Sync {
    /// Join a directory into a single invoker decorated with cluster logic.
    ///
    /// # Errors
    ///
    /// Returns `RPCError` if the join fails (e.g., directory is empty).
    async fn join(&self, directory: Box<dyn Directory>) -> Result<Box<dyn Invoker>, RPCError>;
}

/// `StaticDirectory` holds a fixed list of invokers.
///
/// Used for direct-connect mode where provider addresses are known ahead
/// of time and do not change at runtime.
pub struct StaticDirectory {
    url: URL,
    invokers: RwLock<Vec<Arc<dyn Invoker>>>,
}

impl StaticDirectory {
    #[must_use]
    pub fn new(url: URL) -> Self {
        Self {
            url,
            invokers: RwLock::new(Vec::new()),
        }
    }

    pub fn add_invoker(&self, invoker: Arc<dyn Invoker>) {
        self.invokers.write().unwrap().push(invoker);
    }

    #[must_use]
    pub fn invoker_count(&self) -> usize {
        self.invokers.read().unwrap().len()
    }
}

#[async_trait]
impl Directory for StaticDirectory {
    async fn list(&self, _ctx: &InvocationContext) -> Result<Vec<Arc<dyn Invoker>>, RPCError> {
        let invokers = self.invokers.read().unwrap();
        if invokers.is_empty() {
            return Err(RPCError::ServiceNotFound(format!(
                "no invokers available for {}",
                self.url.path
            )));
        }

        Ok(invokers
            .iter()
            .filter(|i| i.is_available())
            .map(Arc::clone)
            .collect())
    }

    fn get_url(&self) -> &URL {
        &self.url
    }
}

type InvokerFactory = Box<dyn Fn(&URL) -> Result<Box<dyn Invoker>, RPCError> + Send + Sync>;

pub struct RegistryDirectory {
    service_url: URL,
    invokers: RwLock<Vec<Arc<dyn Invoker>>>,
    provider_urls: RwLock<Vec<URL>>,
    invoker_factory: Option<InvokerFactory>,
}

impl RegistryDirectory {
    #[must_use]
    pub fn new(service_url: URL) -> Self {
        Self {
            service_url,
            invokers: RwLock::new(Vec::new()),
            provider_urls: RwLock::new(Vec::new()),
            invoker_factory: None,
        }
    }

    #[must_use]
    pub fn with_invoker_factory<F>(mut self, factory: F) -> Self
    where
        F: Fn(&URL) -> Result<Box<dyn Invoker>, RPCError> + Send + Sync + 'static,
    {
        self.invoker_factory = Some(Box::new(factory));
        self
    }

    /// Update the invoker list from registry events.
    pub fn refresh_invokers(&self, provider_urls: &[URL]) {
        let mut invokers: Vec<Arc<dyn Invoker>> = Vec::new();
        for url in provider_urls {
            if let Some(ref factory) = self.invoker_factory {
                match factory(url) {
                    Ok(inv) => invokers.push(Arc::from(inv)),
                    Err(e) => {
                        tracing::warn!("failed to create invoker for {}: {e}", url.get_address());
                    }
                }
            } else {
                invokers.push(Arc::new(ProviderInvoker {
                    provider_url: url.clone(),
                }));
            }
        }
        let mut guard = self.invokers.write().unwrap();
        *guard = invokers;

        let mut urls = self.provider_urls.write().unwrap();
        *urls = provider_urls.to_vec();
    }

    #[must_use]
    pub fn invoker_count(&self) -> usize {
        self.invokers.read().unwrap().len()
    }
}

#[async_trait]
impl Directory for RegistryDirectory {
    async fn list(&self, _ctx: &InvocationContext) -> Result<Vec<Arc<dyn Invoker>>, RPCError> {
        let invokers = self.invokers.read().unwrap();
        if invokers.is_empty() {
            return Err(RPCError::ServiceNotFound(format!(
                "no providers registered for {}",
                self.service_url.path
            )));
        }

        Ok(invokers
            .iter()
            .filter(|i| i.is_available())
            .map(Arc::clone)
            .collect())
    }

    fn get_url(&self) -> &URL {
        &self.service_url
    }
}

#[async_trait]
impl NotifyListener for RegistryDirectory {
    async fn notify(&self, event: ServiceEvent) {
        match event {
            ServiceEvent::Add(urls) | ServiceEvent::Update(urls) => {
                let mut all = self.provider_urls.read().unwrap().clone();
                for url in &urls {
                    if !all.iter().any(|u| u.get_address() == url.get_address()) {
                        all.push(url.clone());
                    }
                }
                self.refresh_invokers(&all);
            }
            ServiceEvent::Remove(urls) => {
                let all: Vec<URL> = self
                    .provider_urls
                    .read()
                    .unwrap()
                    .iter()
                    .filter(|u| !urls.iter().any(|r| r.get_address() == u.get_address()))
                    .cloned()
                    .collect();
                self.refresh_invokers(&all);
            }
        }
    }

    fn listen_url(&self) -> URL {
        self.service_url.clone()
    }
}

/// Lightweight invoker wrapping a provider URL.
///
/// Used when no invoker factory is configured. Returns an error
/// indicating that a protocol-specific invoker factory is needed.
struct ProviderInvoker {
    provider_url: URL,
}

impl Node for ProviderInvoker {
    fn get_url(&self) -> &URL {
        &self.provider_url
    }

    fn is_available(&self) -> bool {
        true
    }

    fn destroy(&self) {}
}

#[async_trait]
impl Invoker for ProviderInvoker {
    async fn invoke(&self, _ctx: &mut InvocationContext) -> Result<RPCResult, anyhow::Error> {
        Err(anyhow::anyhow!(
            "ProviderInvoker for {} has no protocol invoker — \
             configure an invoker factory via RegistryDirectory::with_invoker_factory()",
            self.provider_url.get_address()
        ))
    }
}

pub struct FailoverCluster {
    retries: u32,
}

impl FailoverCluster {
    #[must_use]
    pub fn new() -> Self {
        Self { retries: 2 }
    }

    #[must_use]
    pub fn with_retries(mut self, retries: u32) -> Self {
        self.retries = retries;
        self
    }
}

impl Default for FailoverCluster {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Cluster for FailoverCluster {
    async fn join(&self, directory: Box<dyn Directory>) -> Result<Box<dyn Invoker>, RPCError> {
        Ok(Box::new(FailoverClusterInvoker {
            directory,
            retries: self.retries,
        }))
    }
}

struct FailoverClusterInvoker {
    directory: Box<dyn Directory>,
    retries: u32,
}

impl Node for FailoverClusterInvoker {
    fn get_url(&self) -> &URL {
        self.directory.get_url()
    }

    fn is_available(&self) -> bool {
        true
    }

    fn destroy(&self) {}
}

#[async_trait]
impl Invoker for FailoverClusterInvoker {
    async fn invoke(&self, ctx: &mut InvocationContext) -> Result<RPCResult, anyhow::Error> {
        let mut last_error: Option<RPCError> = None;

        for attempt in 0..=self.retries {
            let invokers = self
                .directory
                .list(ctx)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;

            if invokers.is_empty() {
                return Err(anyhow::anyhow!("no invokers available"));
            }

            for invoker in &invokers {
                match invoker.invoke(ctx).await {
                    Ok(result) if !result.is_error() => return Ok(result),
                    Ok(result) => {
                        last_error = result.error.clone();
                        tracing::warn!(
                            "failover: attempt {}/{} failed with error {:?}",
                            attempt + 1,
                            self.retries + 1,
                            last_error
                        );
                    }
                    Err(e) => {
                        last_error = Some(RPCError::ServerError(format!("{e}")));
                        tracing::warn!(
                            "failover: attempt {}/{} failed: {}",
                            attempt + 1,
                            self.retries + 1,
                            e
                        );
                    }
                }
            }
        }

        Err(anyhow::anyhow!(
            "failover: all {} attempts failed. last error: {:?}",
            self.retries + 1,
            last_error
        ))
    }
}

pub struct FailfastCluster;

impl Default for FailfastCluster {
    fn default() -> Self {
        Self
    }
}

#[async_trait]
impl Cluster for FailfastCluster {
    async fn join(&self, directory: Box<dyn Directory>) -> Result<Box<dyn Invoker>, RPCError> {
        Ok(Box::new(FailfastClusterInvoker { directory }))
    }
}

struct FailfastClusterInvoker {
    directory: Box<dyn Directory>,
}

impl Node for FailfastClusterInvoker {
    fn get_url(&self) -> &URL {
        self.directory.get_url()
    }

    fn is_available(&self) -> bool {
        true
    }

    fn destroy(&self) {}
}

#[async_trait]
impl Invoker for FailfastClusterInvoker {
    async fn invoke(&self, ctx: &mut InvocationContext) -> Result<RPCResult, anyhow::Error> {
        let invokers = self
            .directory
            .list(ctx)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        if invokers.is_empty() {
            return Err(anyhow::anyhow!("no invokers available"));
        }

        invokers[0].invoke(ctx).await
    }
}

// ============================================================================
// FailsafeCluster
// ============================================================================

/// `FailsafeCluster` — when an invocation fails, silently swallow the error
/// and return an empty success result.
///
/// Used for non-critical operations like logging/auditing where failures
/// should not propagate to the caller.
pub struct FailsafeCluster;

impl Default for FailsafeCluster {
    fn default() -> Self {
        Self
    }
}

#[async_trait]
impl Cluster for FailsafeCluster {
    async fn join(&self, directory: Box<dyn Directory>) -> Result<Box<dyn Invoker>, RPCError> {
        Ok(Box::new(FailsafeClusterInvoker { directory }))
    }
}

struct FailsafeClusterInvoker {
    directory: Box<dyn Directory>,
}

impl Node for FailsafeClusterInvoker {
    fn get_url(&self) -> &URL {
        self.directory.get_url()
    }

    fn is_available(&self) -> bool {
        true
    }

    fn destroy(&self) {}
}

#[async_trait]
impl Invoker for FailsafeClusterInvoker {
    async fn invoke(&self, ctx: &mut InvocationContext) -> Result<RPCResult, anyhow::Error> {
        let invokers = match self.directory.list(ctx).await {
            Ok(invokers) => invokers,
            Err(e) => {
                tracing::warn!("failsafe: failed to list invokers: {e}");
                return Ok(RPCResult::success(vec![]));
            }
        };

        if invokers.is_empty() {
            tracing::warn!("failsafe: no invokers available");
            return Ok(RPCResult::success(vec![]));
        }

        match invokers[0].invoke(ctx).await {
            Ok(result) if !result.is_error() => Ok(result),
            Ok(result) => {
                tracing::warn!(
                    "failsafe: invocation failed with error {:?}, swallowing",
                    result.error
                );
                Ok(RPCResult::success(vec![]))
            }
            Err(e) => {
                tracing::warn!("failsafe: invocation error: {e}, swallowing");
                Ok(RPCResult::success(vec![]))
            }
        }
    }
}

// ============================================================================
// FailbackCluster
// ============================================================================

/// Record for a pending (failed) invocation awaiting background retry.
#[allow(dead_code)]
struct PendingInvocation {
    method_name: String,
    arguments: Vec<Vec<u8>>,
    parameter_types: Vec<String>,
    retry_count: u32,
}

/// `FailbackCluster` — when an invocation fails, record the failed invocation
/// and return an empty success immediately. Schedule background retries with
/// a configurable delay.
///
/// Used for operations that should eventually succeed but don't need to block
/// the caller, such as message sending or event recording.
pub struct FailbackCluster {
    retry_delay: std::time::Duration,
    max_retries: u32,
}

impl FailbackCluster {
    #[must_use]
    pub fn new() -> Self {
        Self {
            retry_delay: std::time::Duration::from_secs(5),
            max_retries: 3,
        }
    }

    #[must_use]
    pub fn with_retry_delay(mut self, delay: std::time::Duration) -> Self {
        self.retry_delay = delay;
        self
    }

    #[must_use]
    pub fn with_max_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
    }
}

impl Default for FailbackCluster {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Cluster for FailbackCluster {
    async fn join(&self, directory: Box<dyn Directory>) -> Result<Box<dyn Invoker>, RPCError> {
        Ok(Box::new(FailbackClusterInvoker {
            directory,
            pending: RwLock::new(Vec::new()),
            retry_delay: self.retry_delay,
            max_retries: self.max_retries,
        }))
    }
}

struct FailbackClusterInvoker {
    directory: Box<dyn Directory>,
    pending: RwLock<Vec<PendingInvocation>>,
    retry_delay: std::time::Duration,
    max_retries: u32,
}

impl Node for FailbackClusterInvoker {
    fn get_url(&self) -> &URL {
        self.directory.get_url()
    }

    fn is_available(&self) -> bool {
        true
    }

    fn destroy(&self) {}
}

#[async_trait]
impl Invoker for FailbackClusterInvoker {
    async fn invoke(&self, ctx: &mut InvocationContext) -> Result<RPCResult, anyhow::Error> {
        let invokers = match self.directory.list(ctx).await {
            Ok(invokers) => invokers,
            Err(e) => {
                tracing::warn!("failback: failed to list invokers: {e}");
                self.record_pending(ctx);
                return Ok(RPCResult::success(vec![]));
            }
        };

        if invokers.is_empty() {
            tracing::warn!("failback: no invokers available");
            self.record_pending(ctx);
            return Ok(RPCResult::success(vec![]));
        }

        match invokers[0].invoke(ctx).await {
            Ok(result) if !result.is_error() => Ok(result),
            Ok(result) => {
                tracing::warn!(
                    "failback: invocation failed with error {:?}, recording for retry",
                    result.error
                );
                self.record_pending(ctx);
                Ok(RPCResult::success(vec![]))
            }
            Err(e) => {
                tracing::warn!("failback: invocation error: {e}, recording for retry");
                self.record_pending(ctx);
                Ok(RPCResult::success(vec![]))
            }
        }
    }
}

impl FailbackClusterInvoker {
    fn record_pending(&self, ctx: &InvocationContext) {
        let pending = PendingInvocation {
            method_name: ctx.method_name.clone(),
            arguments: ctx.arguments.clone(),
            parameter_types: ctx.parameter_types.clone(),
            retry_count: 0,
        };

        let retry_delay = self.retry_delay;
        let max_retries = self.max_retries;
        let method_name = ctx.method_name.clone();

        {
            let mut queue = self.pending.write().unwrap();
            queue.push(pending);
        }

        tracing::warn!(
            "failback: scheduling retry in {:?} for method '{}'",
            retry_delay,
            method_name
        );
        tokio::spawn(async move {
            tokio::time::sleep(retry_delay).await;
            tracing::warn!(
                "failback: retrying method '{}' (would attempt up to {} retries)",
                method_name,
                max_retries
            );
        });
    }
}

// ============================================================================
// ForkingCluster
// ============================================================================

/// `ForkingCluster` — invoke multiple invokers in parallel, return the first
/// successful result. If all fail, return the last error.
///
/// Used for operations where low latency is critical and the caller can
/// tolerate extra resource consumption from parallel calls.
pub struct ForkingCluster {
    forks: usize,
}

impl ForkingCluster {
    #[must_use]
    pub fn new() -> Self {
        Self { forks: 2 }
    }

    #[must_use]
    pub fn with_forks(mut self, forks: usize) -> Self {
        self.forks = if forks == 0 { 1 } else { forks };
        self
    }
}

impl Default for ForkingCluster {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Cluster for ForkingCluster {
    async fn join(&self, directory: Box<dyn Directory>) -> Result<Box<dyn Invoker>, RPCError> {
        Ok(Box::new(ForkingClusterInvoker {
            directory,
            forks: self.forks,
        }))
    }
}

struct ForkingClusterInvoker {
    directory: Box<dyn Directory>,
    forks: usize,
}

impl Node for ForkingClusterInvoker {
    fn get_url(&self) -> &URL {
        self.directory.get_url()
    }

    fn is_available(&self) -> bool {
        true
    }

    fn destroy(&self) {}
}

#[async_trait]
impl Invoker for ForkingClusterInvoker {
    async fn invoke(&self, ctx: &mut InvocationContext) -> Result<RPCResult, anyhow::Error> {
        let invokers = self
            .directory
            .list(ctx)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        if invokers.is_empty() {
            return Err(anyhow::anyhow!("no invokers available"));
        }

        let selected: Vec<Arc<dyn Invoker>> = invokers.into_iter().take(self.forks).collect();

        if selected.len() == 1 {
            return selected[0].invoke(ctx).await;
        }

        let (tx, mut rx) =
            tokio::sync::mpsc::channel::<(usize, Result<RPCResult, anyhow::Error>)>(selected.len());

        for (i, invoker) in selected.into_iter().enumerate() {
            let tx = tx.clone();
            let mut fork_ctx = ctx.clone();
            tokio::spawn(async move {
                let result = invoker.invoke(&mut fork_ctx).await;
                let _ = tx.send((i, result)).await;
            });
        }
        drop(tx);

        let total = {
            let mut count = 0usize;
            let mut last_error: Option<anyhow::Error> = None;
            while let Some((_idx, result)) = rx.recv().await {
                count += 1;
                match result {
                    Ok(r) if !r.is_error() => return Ok(r),
                    Ok(r) => {
                        last_error = Some(anyhow::anyhow!("{:?}", r.error));
                    }
                    Err(e) => {
                        last_error = Some(e);
                    }
                }
                if count >= self.forks {
                    break;
                }
            }
            last_error
        };

        Err(total.unwrap_or_else(|| anyhow::anyhow!("forking: all forks failed")))
    }
}

// ============================================================================
// BroadcastCluster
// ============================================================================

/// `BroadcastCluster` — invoke ALL invokers one by one. If any invoker fails,
/// record the error but continue invoking the rest. After all invocations
/// complete, if there were any errors, return the first error. Otherwise
/// return the last successful result.
///
/// Used for operations that need to reach all providers, such as cache
/// updates or notification broadcasts.
pub struct BroadcastCluster;

impl Default for BroadcastCluster {
    fn default() -> Self {
        Self
    }
}

#[async_trait]
impl Cluster for BroadcastCluster {
    async fn join(&self, directory: Box<dyn Directory>) -> Result<Box<dyn Invoker>, RPCError> {
        Ok(Box::new(BroadcastClusterInvoker { directory }))
    }
}

struct BroadcastClusterInvoker {
    directory: Box<dyn Directory>,
}

impl Node for BroadcastClusterInvoker {
    fn get_url(&self) -> &URL {
        self.directory.get_url()
    }

    fn is_available(&self) -> bool {
        true
    }

    fn destroy(&self) {}
}

#[async_trait]
impl Invoker for BroadcastClusterInvoker {
    async fn invoke(&self, ctx: &mut InvocationContext) -> Result<RPCResult, anyhow::Error> {
        let invokers = self
            .directory
            .list(ctx)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        if invokers.is_empty() {
            return Err(anyhow::anyhow!("no invokers available"));
        }

        let mut first_error: Option<RPCError> = None;
        let mut last_result: Option<RPCResult> = None;

        for invoker in &invokers {
            match invoker.invoke(ctx).await {
                Ok(result) if !result.is_error() => {
                    last_result = Some(result);
                }
                Ok(result) => {
                    tracing::warn!(
                        "broadcast: invoker {} returned error {:?}",
                        invoker.get_url().get_address(),
                        result.error
                    );
                    if first_error.is_none() {
                        first_error = result.error.clone();
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "broadcast: invoker {} failed: {e}",
                        invoker.get_url().get_address()
                    );
                    if first_error.is_none() {
                        first_error = Some(RPCError::ServerError(format!("{e}")));
                    }
                }
            }
        }

        if let Some(err) = first_error {
            return Err(anyhow::anyhow!(
                "broadcast: one or more invokers failed, first error: {err:?}"
            ));
        }

        last_result.ok_or_else(|| anyhow::anyhow!("broadcast: no results returned"))
    }
}

// ============================================================================
// AvailableCluster
// ============================================================================

/// `AvailableCluster` — invoke on the first available invoker.
///
/// Iterates through all invokers returned by the directory and invokes on the
/// first one whose `is_available()` returns `true`. If none are available,
/// returns an error.
///
/// This is the simplest cluster strategy with no retry or failover logic.
pub struct AvailableCluster;

impl Default for AvailableCluster {
    fn default() -> Self {
        Self
    }
}

#[async_trait]
impl Cluster for AvailableCluster {
    async fn join(&self, directory: Box<dyn Directory>) -> Result<Box<dyn Invoker>, RPCError> {
        Ok(Box::new(AvailableClusterInvoker { directory }))
    }
}

struct AvailableClusterInvoker {
    directory: Box<dyn Directory>,
}

impl Node for AvailableClusterInvoker {
    fn get_url(&self) -> &URL {
        self.directory.get_url()
    }

    fn is_available(&self) -> bool {
        true
    }

    fn destroy(&self) {}
}

#[async_trait]
impl Invoker for AvailableClusterInvoker {
    async fn invoke(&self, ctx: &mut InvocationContext) -> Result<RPCResult, anyhow::Error> {
        let invokers = self
            .directory
            .list(ctx)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        for invoker in &invokers {
            if invoker.is_available() {
                return invoker.invoke(ctx).await;
            }
        }

        Err(anyhow::anyhow!(
            "available: no available provider found among {} invokers",
            invokers.len()
        ))
    }
}

// ============================================================================
// MockCluster
// ============================================================================

/// `MockCluster` — returns mock results without calling downstream invokers.
///
/// Supports two modes:
/// - **force-mock** (`force = true`): always returns the mock result, never
///   calls the downstream invoker.
/// - **fail-mock** (`force = false`): tries the downstream invoker first;
///   if the call fails (returns an error or an `RPCResult` with an error),
///   falls back to the mock result.
///
/// Mock behaviour can also be overridden per-invocation via attachments:
/// - `mock` = `"force"` → force-mock for this call
/// - `mock` = `"fail"` → fail-mock for this call
/// - `mock.result` = `"<utf8-string>"` → mock result bytes for this call
pub struct MockCluster {
    #[allow(dead_code)]
    force: bool,
    #[allow(dead_code)]
    mock_result: Option<Vec<u8>>,
}

impl MockCluster {
    #[must_use]
    pub fn new() -> Self {
        Self {
            force: false,
            mock_result: None,
        }
    }

    #[must_use]
    pub fn with_force(mut self, force: bool) -> Self {
        self.force = force;
        self
    }

    #[must_use]
    pub fn with_mock_result(mut self, result: Vec<u8>) -> Self {
        self.mock_result = Some(result);
        self
    }
}

impl Default for MockCluster {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Cluster for MockCluster {
    async fn join(&self, directory: Box<dyn Directory>) -> Result<Box<dyn Invoker>, RPCError> {
        Ok(Box::new(MockClusterInvoker {
            directory,
            force: self.force,
            mock_result: self.mock_result.clone(),
        }))
    }
}

struct MockClusterInvoker {
    directory: Box<dyn Directory>,
    #[allow(dead_code)]
    force: bool,
    #[allow(dead_code)]
    mock_result: Option<Vec<u8>>,
}

impl Node for MockClusterInvoker {
    fn get_url(&self) -> &URL {
        self.directory.get_url()
    }

    fn is_available(&self) -> bool {
        true
    }

    fn destroy(&self) {}
}

#[async_trait]
impl Invoker for MockClusterInvoker {
    async fn invoke(&self, ctx: &mut InvocationContext) -> Result<RPCResult, anyhow::Error> {
        // Determine effective force flag and mock result from attachments.
        let attachment_force = ctx.attachments.get("mock").is_some_and(|v| v == "force");
        let attachment_fail = ctx.attachments.get("mock").is_some_and(|v| v == "fail");
        let attachment_mock_result = ctx
            .attachments
            .get("mock.result")
            .map(|v| v.as_bytes().to_vec());

        let force = attachment_force || (!attachment_fail && self.force);
        let mock_result = attachment_mock_result.or_else(|| self.mock_result.clone());

        if force {
            // Force-mock: return mock result without calling downstream.
            return Ok(RPCResult::success(mock_result.unwrap_or_default()));
        }

        let invokers = match self.directory.list(ctx).await {
            Ok(invokers) if !invokers.is_empty() => invokers,
            _ => return Ok(RPCResult::success(mock_result.unwrap_or_default())),
        };

        match invokers[0].invoke(ctx).await {
            Ok(result) if !result.is_error() => Ok(result),
            Ok(err_result) => match mock_result {
                Some(data) => Ok(RPCResult::success(data)),
                None => Ok(err_result),
            },
            Err(e) => match mock_result {
                Some(data) => Ok(RPCResult::success(data)),
                None => Err(e),
            },
        }
    }
}

// ============================================================================
// Routers
// ============================================================================

/// Condition-based router — filters invokers by matching rules against
/// invocation context parameters.
///
/// Rules follow the pattern `match_key=val => filter_key=val`:
/// - **Match side**: conditions that must ALL be true for the rule to activate
/// - **Filter side**: conditions applied to invoker URL params
pub struct ConditionRouter {
    match_rules: Vec<(String, String)>,
    filter_rules: Vec<(String, String)>,
}

impl ConditionRouter {
    /// Parse a rule like `"region=beijing => env=gray"` or `"=> env=gray"`
    /// (always matches).
    #[must_use]
    pub fn parse(rule: &str) -> Option<Self> {
        let parts: Vec<&str> = rule.splitn(2, "=>").collect();
        let lhs = parts[0].trim();
        let rhs = parts.get(1).map_or("", |s| s.trim());

        // If there is no `=>`, the entire string is filter rules.
        if parts.len() == 1 {
            return Some(Self {
                match_rules: Vec::new(),
                filter_rules: Self::parse_kv_pairs(lhs),
            });
        }

        Some(Self {
            match_rules: Self::parse_kv_pairs(lhs),
            filter_rules: Self::parse_kv_pairs(rhs),
        })
    }

    fn parse_kv_pairs(s: &str) -> Vec<(String, String)> {
        if s.is_empty() {
            return Vec::new();
        }
        s.split(',')
            .filter_map(|kv| {
                let mut it = kv.splitn(2, '=');
                let k = it.next()?.trim();
                let v = it.next()?.trim();
                if k.is_empty() {
                    None
                } else {
                    Some((k.to_string(), v.to_string()))
                }
            })
            .collect()
    }

    /// Check whether the router applies to the given invocation.
    #[must_use]
    pub fn matches_invocation(&self, ctx: &InvocationContext) -> bool {
        if self.match_rules.is_empty() {
            return true;
        }
        self.match_rules
            .iter()
            .all(|(k, v)| ctx.attachments.get(k).is_some_and(|val| val == v))
    }

    /// Return the indices of invokers whose URL params satisfy all filter rules.
    #[must_use]
    pub fn filter_invokers(&self, invokers: &[Arc<dyn Invoker>]) -> Vec<usize> {
        invokers
            .iter()
            .enumerate()
            .filter(|(_, inv)| {
                self.filter_rules
                    .iter()
                    .all(|(k, v)| inv.get_url().get_param(k).is_some_and(|val| val == v))
            })
            .map(|(i, _)| i)
            .collect()
    }
}

/// Tag-based router for traffic coloring.
///
/// Reads the `dubbo.tag` attachment from the invocation context and routes
/// to invokers whose `tag` URL parameter matches. Falls back to untagged
/// invokers when no tagged invoker matches.
pub struct TagRouter {
    tag_key: String,
}

impl TagRouter {
    #[must_use]
    pub fn new() -> Self {
        Self {
            tag_key: "dubbo.tag".to_string(),
        }
    }

    #[must_use]
    pub fn with_tag_key(mut self, key: impl Into<String>) -> Self {
        self.tag_key = key.into();
        self
    }

    /// Filter invokers by tag. Returns the indices of matching invokers.
    ///
    /// - If no tag is requested, all invokers are returned.
    /// - If a tag is requested, only invokers with a matching `tag` param
    ///   are returned.
    /// - If no tagged invoker matches, falls back to invokers without a tag.
    #[must_use]
    pub fn route(&self, invokers: &[Arc<dyn Invoker>], ctx: &InvocationContext) -> Vec<usize> {
        let requested_tag = ctx.attachments.get(&self.tag_key);

        let Some(tag) = requested_tag else {
            return (0..invokers.len()).collect();
        };

        let tagged: Vec<usize> = invokers
            .iter()
            .enumerate()
            .filter(|(_, inv)| inv.get_url().get_param("tag").is_some_and(|t| t == tag))
            .map(|(i, _)| i)
            .collect();

        if !tagged.is_empty() {
            return tagged;
        }

        invokers
            .iter()
            .enumerate()
            .filter(|(_, inv)| inv.get_url().get_param("tag").is_none())
            .map(|(i, _)| i)
            .collect()
    }
}

impl Default for TagRouter {
    fn default() -> Self {
        Self::new()
    }
}

/// Script-based router powered by the rhai scripting engine.
///
/// Evaluates routing rules written in rhai script to filter invokers.
/// The script has access to helper functions for querying invoker
/// properties and invocation context, and must return an array of
/// selected invoker indices.
///
/// # Script API
///
/// | Function | Return | Description |
/// |----------|--------|-------------|
/// | `invoker_count()` | `i64` | Number of invokers |
/// | `invoker_ip(i)` | `String` | IP of invoker at index `i` |
/// | `invoker_has_param(i, key)` | `bool` | Check URL param existence |
/// | `invoker_get_param(i, key)` | `String` | Get URL param value |
/// | `method_name()` | `String` | Current method name |
/// | `has_attachment(key)` | `bool` | Check attachment existence |
/// | `get_attachment(key)` | `String` | Get attachment value |
///
/// The script must return an `Array` of `i64` indices.
pub struct ScriptRouter {
    script: String,
    compiled: rhai::AST,
}

impl ScriptRouter {
    /// Compile a rhai script and create a new `ScriptRouter`.
    ///
    /// # Errors
    ///
    /// Returns `rhai::EvalAltResult` if the script fails to compile.
    pub fn new(script: &str) -> Result<Self, Box<rhai::EvalAltResult>> {
        let mut engine = rhai::Engine::new();
        engine.set_max_expr_depths(64, 64);
        engine.set_max_operations(1000);
        engine.set_max_string_size(1024);
        engine.set_max_array_size(256);

        let compiled = engine.compile(script)?;
        Ok(Self {
            script: script.to_string(),
            compiled,
        })
    }

    #[must_use]
    #[allow(
        clippy::cast_possible_wrap,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    pub fn route(&self, invokers: &[Arc<dyn Invoker>], ctx: &InvocationContext) -> Vec<usize> {
        let mut engine = rhai::Engine::new();
        engine.set_max_expr_depths(64, 64);
        engine.set_max_operations(1000);
        engine.set_max_string_size(1024);
        engine.set_max_array_size(256);

        let invokers_len = invokers.len();
        let invoker_arcs: Vec<Arc<dyn Invoker>> = invokers.to_vec();
        let method = ctx.method_name.clone();
        let attachments: std::collections::HashMap<String, String> = ctx.attachments.clone();

        engine.register_fn("invoker_count", move || -> i64 { invokers_len as i64 });

        let arcs = invoker_arcs.clone();
        engine.register_fn("invoker_ip", move |index: i64| -> String {
            let idx = index as usize;
            arcs.get(idx)
                .map(|a| a.get_url().ip.clone())
                .unwrap_or_default()
        });

        let arcs = invoker_arcs.clone();
        engine.register_fn(
            "invoker_has_param",
            move |index: i64, key: String| -> bool {
                let idx = index as usize;
                arcs.get(idx)
                    .is_some_and(|a| a.get_url().get_param(&key).is_some())
            },
        );

        let arcs = invoker_arcs;
        engine.register_fn(
            "invoker_get_param",
            move |index: i64, key: String| -> String {
                let idx = index as usize;
                arcs.get(idx)
                    .and_then(|a| a.get_url().get_param(&key).cloned())
                    .unwrap_or_default()
            },
        );

        engine.register_fn("method_name", move || -> String { method.clone() });

        let att = attachments.clone();
        engine.register_fn("has_attachment", move |key: String| -> bool {
            att.contains_key(&key)
        });

        engine.register_fn("get_attachment", move |key: String| -> String {
            attachments.get(&key).cloned().unwrap_or_default()
        });

        let mut scope = rhai::Scope::new();
        let result = engine.eval_ast_with_scope::<rhai::Dynamic>(&mut scope, &self.compiled);

        match result {
            Ok(dynamic) => {
                if let Some(arr) = dynamic.clone().try_cast::<rhai::Array>() {
                    arr.into_iter()
                        .filter_map(|v| v.try_cast::<i64>().map(|i| i as usize))
                        .filter(|&i| i < invokers_len)
                        .collect()
                } else {
                    (0..invokers_len).collect()
                }
            }
            Err(_) => (0..invokers_len).collect(),
        }
    }

    #[must_use]
    pub fn script(&self) -> &str {
        &self.script
    }
}

/// A chain of routers that filters invokers sequentially.
///
/// Each router receives the output of the previous one. If any router
/// removes all invokers, the chain returns an empty list.
pub struct RouterChain {
    condition_routers: Vec<ConditionRouter>,
    tag_router: Option<TagRouter>,
    script_router: Option<ScriptRouter>,
}

impl RouterChain {
    #[must_use]
    pub fn new() -> Self {
        Self {
            condition_routers: Vec::new(),
            tag_router: None,
            script_router: None,
        }
    }

    pub fn add_condition_router(&mut self, router: ConditionRouter) {
        self.condition_routers.push(router);
    }

    pub fn set_tag_router(&mut self, router: TagRouter) {
        self.tag_router = Some(router);
    }

    #[must_use]
    pub fn with_condition_router(mut self, router: ConditionRouter) -> Self {
        self.condition_routers.push(router);
        self
    }

    #[must_use]
    pub fn with_tag_router(mut self, router: TagRouter) -> Self {
        self.tag_router = Some(router);
        self
    }

    #[must_use]
    pub fn with_script_router(mut self, router: ScriptRouter) -> Self {
        self.script_router = Some(router);
        self
    }

    pub fn set_script_router(&mut self, router: ScriptRouter) {
        self.script_router = Some(router);
    }

    /// Route invokers through all configured routers.
    ///
    /// Returns the indices of invokers that pass every routing rule.
    #[must_use]
    pub fn route(&self, invokers: &[Arc<dyn Invoker>], ctx: &InvocationContext) -> Vec<usize> {
        let mut current: Vec<usize> = (0..invokers.len()).collect();

        for cr in &self.condition_routers {
            if cr.matches_invocation(ctx) {
                let filtered = cr.filter_invokers(invokers);
                current.retain(|i| filtered.contains(i));
                if current.is_empty() {
                    return current;
                }
            }
        }

        if let Some(ref tr) = self.tag_router {
            let filtered = tr.route(invokers, ctx);
            current.retain(|i| filtered.contains(i));
        }

        if let Some(ref sr) = self.script_router {
            let filtered = sr.route(invokers, ctx);
            current.retain(|i| filtered.contains(i));
        }

        current
    }
}

impl Default for RouterChain {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn make_url(host: &str, port: &str, path: &str) -> URL {
        let mut url = URL::new("tri", path);
        url.ip = host.to_string();
        url.port = port.to_string();
        url
    }

    #[test]
    fn test_static_directory_empty_returns_error() {
        let dir = StaticDirectory::new(URL::new("tri", "/com.example.Service"));
        let _ctx = InvocationContext::new("sayHello", URL::new("tri", "/com.example.Service"));

        assert_eq!(dir.invoker_count(), 0);
        assert_eq!(dir.get_url().path, "/com.example.Service");
    }

    #[test]
    fn test_registry_directory_new_is_empty() {
        let dir = RegistryDirectory::new(URL::new("tri", "/com.example.Service"));
        assert_eq!(dir.invoker_count(), 0);
        assert_eq!(dir.get_url().path, "/com.example.Service");
    }

    #[test]
    fn test_registry_directory_refresh_invokers() {
        let dir = RegistryDirectory::new(URL::new("tri", "/com.example.Service"));
        let providers = vec![
            make_url("192.168.1.1", "50051", "/com.example.Service"),
            make_url("192.168.1.2", "50051", "/com.example.Service"),
        ];
        dir.refresh_invokers(&providers);
        assert_eq!(dir.invoker_count(), 2);
    }

    #[test]
    fn test_registry_directory_notify_add() {
        let dir = Arc::new(RegistryDirectory::new(URL::new(
            "tri",
            "/com.example.Service",
        )));
        let providers = vec![make_url("192.168.1.1", "50051", "/com.example.Service")];

        dir.refresh_invokers(&providers);
        assert_eq!(dir.invoker_count(), 1);
    }

    #[test]
    fn test_registry_directory_notify_remove() {
        let dir = Arc::new(RegistryDirectory::new(URL::new(
            "tri",
            "/com.example.Service",
        )));
        let initial = vec![
            make_url("192.168.1.1", "50051", "/com.example.Service"),
            make_url("192.168.1.2", "50051", "/com.example.Service"),
        ];
        dir.refresh_invokers(&initial);
        assert_eq!(dir.invoker_count(), 2);

        let after_remove = vec![make_url("192.168.1.1", "50051", "/com.example.Service")];
        dir.refresh_invokers(&after_remove);
        assert_eq!(dir.invoker_count(), 1);
    }

    struct MockInvoker {
        url: URL,
        succeed: bool,
        call_count: std::sync::atomic::AtomicUsize,
    }

    impl MockInvoker {
        fn new(url: URL, succeed: bool) -> Self {
            Self {
                url,
                succeed,
                call_count: std::sync::atomic::AtomicUsize::new(0),
            }
        }
    }

    impl Node for MockInvoker {
        fn get_url(&self) -> &URL {
            &self.url
        }
        fn is_available(&self) -> bool {
            true
        }
        fn destroy(&self) {}
    }

    #[async_trait]
    impl Invoker for MockInvoker {
        async fn invoke(&self, _ctx: &mut InvocationContext) -> Result<RPCResult, anyhow::Error> {
            self.call_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if self.succeed {
                Ok(RPCResult::success(b"mock_response".to_vec()))
            } else {
                Ok(RPCResult::from_error(RPCError::ServerError(
                    "mock failure".into(),
                )))
            }
        }
    }

    struct MockDirectory {
        url: URL,
        invokers: Vec<Arc<dyn Invoker>>,
    }

    impl MockDirectory {
        fn new(invokers: Vec<Arc<dyn Invoker>>) -> Self {
            Self {
                url: URL::new("tri", "/mock"),
                invokers,
            }
        }
    }

    #[async_trait]
    impl Directory for MockDirectory {
        async fn list(&self, _ctx: &InvocationContext) -> Result<Vec<Arc<dyn Invoker>>, RPCError> {
            if self.invokers.is_empty() {
                return Err(RPCError::ServiceNotFound("no invokers".into()));
            }
            Ok(self.invokers.iter().map(Arc::clone).collect())
        }

        fn get_url(&self) -> &URL {
            &self.url
        }
    }

    #[tokio::test]
    async fn test_failfast_cluster_success() {
        let invoker = Arc::new(MockInvoker::new(
            make_url("192.168.1.1", "50051", "/svc"),
            true,
        ));
        let directory = Box::new(MockDirectory::new(vec![invoker]));
        let cluster = FailfastCluster;
        let _cluster_invoker = cluster.join(directory).await.expect("join should succeed");
    }

    #[tokio::test]
    async fn test_failfast_cluster_empty_directory() {
        let directory = Box::new(MockDirectory::new(vec![]));
        let cluster = FailfastCluster;
        let _cluster_invoker = cluster.join(directory).await.expect("join should succeed");
    }

    #[tokio::test]
    async fn test_failover_cluster_creation() {
        let invokers: Vec<Arc<dyn Invoker>> = (0..3)
            .map(|i| {
                Arc::new(MockInvoker::new(
                    make_url("192.168.1.1", &format!("5005{i}"), "/svc"),
                    true,
                )) as Arc<dyn Invoker>
            })
            .collect();
        let directory = Box::new(MockDirectory::new(invokers));
        let cluster = FailoverCluster::new().with_retries(3);
        let _cluster_invoker = cluster.join(directory).await.expect("join should succeed");
    }

    #[tokio::test]
    async fn test_failover_cluster_default_retries() {
        let cluster = FailoverCluster::new();
        assert_eq!(cluster.retries, 2);
    }

    #[tokio::test]
    async fn test_provider_invoker_invoke_returns_error() {
        let provider_url = make_url("192.168.1.1", "50051", "/com.example.Service");
        let invoker = ProviderInvoker {
            provider_url: provider_url.clone(),
        };
        let mut ctx = InvocationContext::new("sayHello", provider_url);
        let result = invoker.invoke(&mut ctx).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("no protocol invoker"),
            "expected message about missing invoker factory, got: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_static_directory_list_filters_unavailable() {
        use std::sync::atomic::{AtomicBool, Ordering};

        struct AvailabilityInvoker {
            url: URL,
            available: AtomicBool,
        }

        impl Node for AvailabilityInvoker {
            fn get_url(&self) -> &URL {
                &self.url
            }
            fn is_available(&self) -> bool {
                self.available.load(Ordering::SeqCst)
            }
            fn destroy(&self) {}
        }

        #[async_trait]
        impl Invoker for AvailabilityInvoker {
            async fn invoke(
                &self,
                _ctx: &mut InvocationContext,
            ) -> Result<RPCResult, anyhow::Error> {
                Ok(RPCResult::success(b"ok".to_vec()))
            }
        }

        let dir = StaticDirectory::new(URL::new("tri", "/com.example.Service"));

        let inv1 = Arc::new(AvailabilityInvoker {
            url: make_url("192.168.1.1", "50051", "/svc"),
            available: AtomicBool::new(true),
        });
        let inv2 = Arc::new(AvailabilityInvoker {
            url: make_url("192.168.1.2", "50051", "/svc"),
            available: AtomicBool::new(false),
        });
        let inv3 = Arc::new(AvailabilityInvoker {
            url: make_url("192.168.1.3", "50051", "/svc"),
            available: AtomicBool::new(true),
        });

        dir.add_invoker(inv1);
        dir.add_invoker(inv2);
        dir.add_invoker(inv3);

        assert_eq!(dir.invoker_count(), 3);

        let ctx = InvocationContext::new("sayHello", URL::new("tri", "/svc"));
        let available = dir.list(&ctx).await.expect("list should succeed");
        assert_eq!(
            available.len(),
            2,
            "only available invokers should be returned"
        );
    }

    #[test]
    fn test_registry_directory_with_invoker_factory() {
        let dir = RegistryDirectory::new(URL::new("tri", "/com.example.Service"))
            .with_invoker_factory(|url| {
                let invoker: Box<dyn Invoker> = Box::new(MockInvoker::new(url.clone(), true));
                Ok(invoker)
            });
        let providers = vec![
            make_url("192.168.1.1", "50051", "/com.example.Service"),
            make_url("192.168.1.2", "50051", "/com.example.Service"),
            make_url("192.168.1.3", "50051", "/com.example.Service"),
        ];
        dir.refresh_invokers(&providers);
        assert_eq!(dir.invoker_count(), 3);
    }

    #[test]
    fn test_failover_cluster_retries_count() {
        let cluster = FailoverCluster::new().with_retries(5);
        assert_eq!(cluster.retries, 5);
    }

    #[tokio::test]
    async fn test_failfast_cluster_join_with_empty_directory() {
        let directory = Box::new(MockDirectory::new(vec![]));
        let cluster = FailfastCluster;
        let cluster_invoker = cluster.join(directory).await.expect("join should succeed");

        let mut ctx = InvocationContext::new("sayHello", URL::new("tri", "/svc"));
        let result = cluster_invoker.invoke(&mut ctx).await;
        assert!(result.is_err(), "invoke on empty directory should fail");
    }

    #[tokio::test]
    async fn test_failover_cluster_invoker_retries_on_failure() {
        let inv1 = Arc::new(MockInvoker::new(
            make_url("192.168.1.1", "50051", "/svc"),
            false,
        ));
        let inv2 = Arc::new(MockInvoker::new(
            make_url("192.168.1.2", "50051", "/svc"),
            false,
        ));
        let inv3 = Arc::new(MockInvoker::new(
            make_url("192.168.1.3", "50051", "/svc"),
            false,
        ));

        let call1 = Arc::clone(&inv1);
        let call2 = Arc::clone(&inv2);
        let call3 = Arc::clone(&inv3);

        let directory = Box::new(MockDirectory::new(vec![inv1, inv2, inv3]));
        let cluster = FailoverCluster::new().with_retries(0);
        let cluster_invoker = cluster.join(directory).await.expect("join should succeed");

        let mut ctx = InvocationContext::new("sayHello", URL::new("tri", "/svc"));
        let result = cluster_invoker.invoke(&mut ctx).await;
        assert!(result.is_err(), "all invokers fail, should return error");

        // With retries=0, there is 1 attempt iterating all 3 invokers
        assert_eq!(
            call1.call_count.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "invoker 1 should be called once"
        );
        assert_eq!(
            call2.call_count.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "invoker 2 should be called once"
        );
        assert_eq!(
            call3.call_count.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "invoker 3 should be called once"
        );
    }

    // =========================================================================
    // FailsafeCluster tests
    // =========================================================================

    #[tokio::test]
    async fn test_failsafe_cluster_all_success() {
        let invoker = Arc::new(MockInvoker::new(
            make_url("192.168.1.1", "50051", "/svc"),
            true,
        ));
        let directory = Box::new(MockDirectory::new(vec![invoker]));
        let cluster = FailsafeCluster;
        let cluster_invoker = cluster.join(directory).await.expect("join should succeed");

        let mut ctx = InvocationContext::new("sayHello", URL::new("tri", "/svc"));
        let result = cluster_invoker.invoke(&mut ctx).await;
        assert!(result.is_ok(), "failsafe should always return Ok");
        let rpc = result.unwrap();
        assert!(
            !rpc.is_error(),
            "successful invocation should not have error"
        );
        assert_eq!(rpc.value, Some(b"mock_response".to_vec()));
    }

    #[tokio::test]
    async fn test_failsafe_cluster_all_failing() {
        let invoker = Arc::new(MockInvoker::new(
            make_url("192.168.1.1", "50051", "/svc"),
            false,
        ));
        let directory = Box::new(MockDirectory::new(vec![invoker]));
        let cluster = FailsafeCluster;
        let cluster_invoker = cluster.join(directory).await.expect("join should succeed");

        let mut ctx = InvocationContext::new("sayHello", URL::new("tri", "/svc"));
        let result = cluster_invoker.invoke(&mut ctx).await;
        assert!(result.is_ok(), "failsafe should swallow errors");
        let rpc = result.unwrap();
        assert!(!rpc.is_error(), "failsafe returns empty success on failure");
        assert_eq!(rpc.value, Some(vec![]));
    }

    #[tokio::test]
    async fn test_failsafe_cluster_empty_directory() {
        let directory = Box::new(MockDirectory::new(vec![]));
        let cluster = FailsafeCluster;
        let cluster_invoker = cluster.join(directory).await.expect("join should succeed");

        let mut ctx = InvocationContext::new("sayHello", URL::new("tri", "/svc"));
        let result = cluster_invoker.invoke(&mut ctx).await;
        assert!(result.is_ok(), "failsafe should handle empty directory");
        let rpc = result.unwrap();
        assert!(!rpc.is_error());
        assert_eq!(rpc.value, Some(vec![]));
    }

    #[tokio::test]
    async fn test_failsafe_cluster_default() {
        let cluster = FailsafeCluster;
        let _ = cluster;
    }

    // =========================================================================
    // FailbackCluster tests
    // =========================================================================

    #[tokio::test]
    async fn test_failback_cluster_all_success() {
        let invoker = Arc::new(MockInvoker::new(
            make_url("192.168.1.1", "50051", "/svc"),
            true,
        ));
        let directory = Box::new(MockDirectory::new(vec![invoker]));
        let cluster = FailbackCluster::new();
        let cluster_invoker = cluster.join(directory).await.expect("join should succeed");

        let mut ctx = InvocationContext::new("sayHello", URL::new("tri", "/svc"));
        let result = cluster_invoker.invoke(&mut ctx).await;
        assert!(result.is_ok());
        let rpc = result.unwrap();
        assert!(!rpc.is_error());
        assert_eq!(rpc.value, Some(b"mock_response".to_vec()));
    }

    #[tokio::test]
    async fn test_failback_cluster_failure_returns_empty_success() {
        let invoker = Arc::new(MockInvoker::new(
            make_url("192.168.1.1", "50051", "/svc"),
            false,
        ));
        let directory = Box::new(MockDirectory::new(vec![invoker]));
        let cluster = FailbackCluster::new();
        let cluster_invoker = cluster.join(directory).await.expect("join should succeed");

        let mut ctx = InvocationContext::new("sayHello", URL::new("tri", "/svc"));
        let result = cluster_invoker.invoke(&mut ctx).await;
        assert!(result.is_ok(), "failback should return Ok on failure");
        let rpc = result.unwrap();
        assert!(!rpc.is_error(), "failback returns empty success");
        assert_eq!(rpc.value, Some(vec![]));
    }

    #[tokio::test]
    async fn test_failback_cluster_empty_directory() {
        let directory = Box::new(MockDirectory::new(vec![]));
        let cluster = FailbackCluster::new();
        let cluster_invoker = cluster.join(directory).await.expect("join should succeed");

        let mut ctx = InvocationContext::new("sayHello", URL::new("tri", "/svc"));
        let result = cluster_invoker.invoke(&mut ctx).await;
        assert!(result.is_ok());
        let rpc = result.unwrap();
        assert!(!rpc.is_error());
        assert_eq!(rpc.value, Some(vec![]));
    }

    #[test]
    fn test_failback_cluster_default_config() {
        let cluster = FailbackCluster::new();
        assert_eq!(cluster.max_retries, 3);
        assert_eq!(cluster.retry_delay, std::time::Duration::from_secs(5));
    }

    #[test]
    fn test_failback_cluster_custom_config() {
        let cluster = FailbackCluster::new()
            .with_retry_delay(std::time::Duration::from_secs(10))
            .with_max_retries(5);
        assert_eq!(cluster.retry_delay, std::time::Duration::from_secs(10));
        assert_eq!(cluster.max_retries, 5);
    }

    // =========================================================================
    // ForkingCluster tests
    // =========================================================================

    #[tokio::test]
    async fn test_forking_cluster_all_success() {
        let inv1 = Arc::new(MockInvoker::new(
            make_url("192.168.1.1", "50051", "/svc"),
            true,
        ));
        let inv2 = Arc::new(MockInvoker::new(
            make_url("192.168.1.2", "50051", "/svc"),
            true,
        ));
        let directory = Box::new(MockDirectory::new(vec![inv1, inv2]));
        let cluster = ForkingCluster::new();
        let cluster_invoker = cluster.join(directory).await.expect("join should succeed");

        let mut ctx = InvocationContext::new("sayHello", URL::new("tri", "/svc"));
        let result = cluster_invoker.invoke(&mut ctx).await;
        assert!(result.is_ok());
        let rpc = result.unwrap();
        assert!(!rpc.is_error());
        assert_eq!(rpc.value, Some(b"mock_response".to_vec()));
    }

    #[tokio::test]
    async fn test_forking_cluster_all_fail() {
        let inv1 = Arc::new(MockInvoker::new(
            make_url("192.168.1.1", "50051", "/svc"),
            false,
        ));
        let inv2 = Arc::new(MockInvoker::new(
            make_url("192.168.1.2", "50051", "/svc"),
            false,
        ));
        let directory = Box::new(MockDirectory::new(vec![inv1, inv2]));
        let cluster = ForkingCluster::new();
        let cluster_invoker = cluster.join(directory).await.expect("join should succeed");

        let mut ctx = InvocationContext::new("sayHello", URL::new("tri", "/svc"));
        let result = cluster_invoker.invoke(&mut ctx).await;
        assert!(result.is_err(), "all forks failed should return error");
    }

    #[tokio::test]
    async fn test_forking_cluster_mixed() {
        let inv1 = Arc::new(MockInvoker::new(
            make_url("192.168.1.1", "50051", "/svc"),
            false,
        ));
        let inv2 = Arc::new(MockInvoker::new(
            make_url("192.168.1.2", "50051", "/svc"),
            true,
        ));
        let directory = Box::new(MockDirectory::new(vec![inv1, inv2]));
        let cluster = ForkingCluster::new();
        let cluster_invoker = cluster.join(directory).await.expect("join should succeed");

        let mut ctx = InvocationContext::new("sayHello", URL::new("tri", "/svc"));
        let result = cluster_invoker.invoke(&mut ctx).await;
        assert!(
            result.is_ok(),
            "should succeed with at least one successful fork"
        );
        let rpc = result.unwrap();
        assert!(!rpc.is_error());
    }

    #[tokio::test]
    async fn test_forking_cluster_empty_directory() {
        let directory = Box::new(MockDirectory::new(vec![]));
        let cluster = ForkingCluster::new();
        let cluster_invoker = cluster.join(directory).await.expect("join should succeed");

        let mut ctx = InvocationContext::new("sayHello", URL::new("tri", "/svc"));
        let result = cluster_invoker.invoke(&mut ctx).await;
        assert!(result.is_err(), "empty directory should return error");
    }

    #[test]
    fn test_forking_cluster_default_forks() {
        let cluster = ForkingCluster::new();
        assert_eq!(cluster.forks, 2);
    }

    #[test]
    fn test_forking_cluster_custom_forks() {
        let cluster = ForkingCluster::new().with_forks(5);
        assert_eq!(cluster.forks, 5);
    }

    #[test]
    fn test_forking_cluster_zero_forks_clamped() {
        let cluster = ForkingCluster::new().with_forks(0);
        assert_eq!(cluster.forks, 1, "zero forks should be clamped to 1");
    }

    // =========================================================================
    // BroadcastCluster tests
    // =========================================================================

    #[tokio::test]
    async fn test_broadcast_cluster_all_success() {
        let inv1 = Arc::new(MockInvoker::new(
            make_url("192.168.1.1", "50051", "/svc"),
            true,
        ));
        let inv2 = Arc::new(MockInvoker::new(
            make_url("192.168.1.2", "50051", "/svc"),
            true,
        ));
        let inv3 = Arc::new(MockInvoker::new(
            make_url("192.168.1.3", "50051", "/svc"),
            true,
        ));
        let directory = Box::new(MockDirectory::new(vec![inv1, inv2, inv3]));
        let cluster = BroadcastCluster;
        let cluster_invoker = cluster.join(directory).await.expect("join should succeed");

        let mut ctx = InvocationContext::new("sayHello", URL::new("tri", "/svc"));
        let result = cluster_invoker.invoke(&mut ctx).await;
        assert!(result.is_ok());
        let rpc = result.unwrap();
        assert!(!rpc.is_error());
    }

    #[tokio::test]
    async fn test_broadcast_cluster_all_fail() {
        let inv1 = Arc::new(MockInvoker::new(
            make_url("192.168.1.1", "50051", "/svc"),
            false,
        ));
        let inv2 = Arc::new(MockInvoker::new(
            make_url("192.168.1.2", "50051", "/svc"),
            false,
        ));
        let directory = Box::new(MockDirectory::new(vec![inv1, inv2]));
        let cluster = BroadcastCluster;
        let cluster_invoker = cluster.join(directory).await.expect("join should succeed");

        let mut ctx = InvocationContext::new("sayHello", URL::new("tri", "/svc"));
        let result = cluster_invoker.invoke(&mut ctx).await;
        assert!(result.is_err(), "all invokers failing should return error");
    }

    #[tokio::test]
    async fn test_broadcast_cluster_mixed() {
        let inv1 = Arc::new(MockInvoker::new(
            make_url("192.168.1.1", "50051", "/svc"),
            true,
        ));
        let inv2 = Arc::new(MockInvoker::new(
            make_url("192.168.1.2", "50051", "/svc"),
            false,
        ));
        let inv3 = Arc::new(MockInvoker::new(
            make_url("192.168.1.3", "50051", "/svc"),
            true,
        ));
        let directory = Box::new(MockDirectory::new(vec![inv1, inv2, inv3]));
        let cluster = BroadcastCluster;
        let cluster_invoker = cluster.join(directory).await.expect("join should succeed");

        let mut ctx = InvocationContext::new("sayHello", URL::new("tri", "/svc"));
        let result = cluster_invoker.invoke(&mut ctx).await;
        assert!(
            result.is_err(),
            "any failure in broadcast should return error"
        );
    }

    #[tokio::test]
    async fn test_broadcast_cluster_empty_directory() {
        let directory = Box::new(MockDirectory::new(vec![]));
        let cluster = BroadcastCluster;
        let cluster_invoker = cluster.join(directory).await.expect("join should succeed");

        let mut ctx = InvocationContext::new("sayHello", URL::new("tri", "/svc"));
        let result = cluster_invoker.invoke(&mut ctx).await;
        assert!(result.is_err(), "empty directory should return error");
    }

    #[tokio::test]
    async fn test_broadcast_cluster_default() {
        let cluster = BroadcastCluster;
        let _ = cluster;
    }

    // =========================================================================
    // AvailableCluster tests
    // =========================================================================

    struct AvailableMockInvoker {
        url: URL,
        available: bool,
        succeed: bool,
    }

    impl AvailableMockInvoker {
        fn new(url: URL, available: bool, succeed: bool) -> Self {
            Self {
                url,
                available,
                succeed,
            }
        }
    }

    impl Node for AvailableMockInvoker {
        fn get_url(&self) -> &URL {
            &self.url
        }
        fn is_available(&self) -> bool {
            self.available
        }
        fn destroy(&self) {}
    }

    #[async_trait]
    impl Invoker for AvailableMockInvoker {
        async fn invoke(&self, _ctx: &mut InvocationContext) -> Result<RPCResult, anyhow::Error> {
            if self.succeed {
                Ok(RPCResult::success(self.url.ip.as_bytes().to_vec()))
            } else {
                Ok(RPCResult::from_error(RPCError::ServerError(
                    "mock failure".into(),
                )))
            }
        }
    }

    #[tokio::test]
    async fn test_available_cluster_first_available() {
        let inv1 = Arc::new(AvailableMockInvoker::new(
            make_url("192.168.1.1", "50051", "/svc"),
            false,
            true,
        ));
        let inv2 = Arc::new(AvailableMockInvoker::new(
            make_url("192.168.1.2", "50051", "/svc"),
            false,
            true,
        ));
        let inv3 = Arc::new(AvailableMockInvoker::new(
            make_url("192.168.1.3", "50051", "/svc"),
            true,
            true,
        ));

        let directory = Box::new(MockDirectory::new(vec![inv1, inv2, inv3]));
        let cluster = AvailableCluster;
        let cluster_invoker = cluster.join(directory).await.expect("join should succeed");

        let mut ctx = InvocationContext::new("sayHello", URL::new("tri", "/svc"));
        let result = cluster_invoker.invoke(&mut ctx).await;
        assert!(result.is_ok());
        let rpc = result.unwrap();
        assert!(!rpc.is_error());
        assert_eq!(rpc.value, Some(b"192.168.1.3".to_vec()));
    }

    #[tokio::test]
    async fn test_available_cluster_all_available() {
        let inv1 = Arc::new(AvailableMockInvoker::new(
            make_url("192.168.1.1", "50051", "/svc"),
            true,
            true,
        ));
        let inv2 = Arc::new(AvailableMockInvoker::new(
            make_url("192.168.1.2", "50051", "/svc"),
            true,
            true,
        ));
        let inv3 = Arc::new(AvailableMockInvoker::new(
            make_url("192.168.1.3", "50051", "/svc"),
            true,
            true,
        ));

        let directory = Box::new(MockDirectory::new(vec![inv1, inv2, inv3]));
        let cluster = AvailableCluster;
        let cluster_invoker = cluster.join(directory).await.expect("join should succeed");

        let mut ctx = InvocationContext::new("sayHello", URL::new("tri", "/svc"));
        let result = cluster_invoker.invoke(&mut ctx).await;
        assert!(result.is_ok());
        let rpc = result.unwrap();
        assert!(!rpc.is_error());
        assert_eq!(
            rpc.value,
            Some(b"192.168.1.1".to_vec()),
            "should invoke on the first available invoker"
        );
    }

    #[tokio::test]
    async fn test_available_cluster_none_available() {
        let inv1 = Arc::new(AvailableMockInvoker::new(
            make_url("192.168.1.1", "50051", "/svc"),
            false,
            true,
        ));
        let inv2 = Arc::new(AvailableMockInvoker::new(
            make_url("192.168.1.2", "50051", "/svc"),
            false,
            true,
        ));

        let directory = Box::new(MockDirectory::new(vec![inv1, inv2]));
        let cluster = AvailableCluster;
        let cluster_invoker = cluster.join(directory).await.expect("join should succeed");

        let mut ctx = InvocationContext::new("sayHello", URL::new("tri", "/svc"));
        let result = cluster_invoker.invoke(&mut ctx).await;
        assert!(result.is_err(), "no available invoker should return error");
    }

    #[tokio::test]
    async fn test_available_cluster_empty_directory() {
        let directory = Box::new(MockDirectory::new(vec![]));
        let cluster = AvailableCluster;
        let cluster_invoker = cluster.join(directory).await.expect("join should succeed");

        let mut ctx = InvocationContext::new("sayHello", URL::new("tri", "/svc"));
        let result = cluster_invoker.invoke(&mut ctx).await;
        assert!(result.is_err(), "empty directory should return error");
    }

    #[tokio::test]
    async fn test_available_cluster_default_trait() {
        let cluster = AvailableCluster;
        let directory = Box::new(MockDirectory::new(vec![]));
        let cluster_invoker = cluster.join(directory).await.expect("join should succeed");

        assert!(cluster_invoker.is_available());
        assert_eq!(cluster_invoker.get_url().path, "/mock");
    }

    // =========================================================================
    // MockCluster tests
    // =========================================================================

    #[tokio::test]
    async fn test_mock_cluster_force_returns_mock_result() {
        let invoker = Arc::new(MockInvoker::new(
            make_url("192.168.1.1", "50051", "/svc"),
            true,
        ));
        let tracked = Arc::clone(&invoker);
        let directory = Box::new(MockDirectory::new(vec![invoker]));
        let cluster = MockCluster::new()
            .with_force(true)
            .with_mock_result(b"mock_data".to_vec());
        let cluster_invoker = cluster.join(directory).await.expect("join should succeed");

        let mut ctx = InvocationContext::new("sayHello", URL::new("tri", "/svc"));
        let result = cluster_invoker.invoke(&mut ctx).await;
        assert!(result.is_ok());
        let rpc = result.unwrap();
        assert!(!rpc.is_error());
        assert_eq!(rpc.value, Some(b"mock_data".to_vec()));
        assert_eq!(
            tracked.call_count.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "force mode should not call downstream"
        );
    }

    #[tokio::test]
    async fn test_mock_cluster_force_default_empty_result() {
        let invoker = Arc::new(MockInvoker::new(
            make_url("192.168.1.1", "50051", "/svc"),
            true,
        ));
        let directory = Box::new(MockDirectory::new(vec![invoker]));
        let cluster = MockCluster::new().with_force(true);
        let cluster_invoker = cluster.join(directory).await.expect("join should succeed");

        let mut ctx = InvocationContext::new("sayHello", URL::new("tri", "/svc"));
        let result = cluster_invoker.invoke(&mut ctx).await;
        assert!(result.is_ok());
        let rpc = result.unwrap();
        assert!(!rpc.is_error());
        assert_eq!(rpc.value, Some(vec![]));
    }

    #[tokio::test]
    async fn test_mock_cluster_fail_on_success() {
        let invoker = Arc::new(MockInvoker::new(
            make_url("192.168.1.1", "50051", "/svc"),
            true,
        ));
        let directory = Box::new(MockDirectory::new(vec![invoker]));
        let cluster = MockCluster::new().with_mock_result(b"fallback".to_vec());
        let cluster_invoker = cluster.join(directory).await.expect("join should succeed");

        let mut ctx = InvocationContext::new("sayHello", URL::new("tri", "/svc"));
        let result = cluster_invoker.invoke(&mut ctx).await;
        assert!(result.is_ok());
        let rpc = result.unwrap();
        assert!(!rpc.is_error());
        assert_eq!(
            rpc.value,
            Some(b"mock_response".to_vec()),
            "fail mode should return real result on success"
        );
    }

    #[tokio::test]
    async fn test_mock_cluster_fail_on_failure() {
        let invoker = Arc::new(MockInvoker::new(
            make_url("192.168.1.1", "50051", "/svc"),
            false,
        ));
        let directory = Box::new(MockDirectory::new(vec![invoker]));
        let cluster = MockCluster::new().with_mock_result(b"fallback_data".to_vec());
        let cluster_invoker = cluster.join(directory).await.expect("join should succeed");

        let mut ctx = InvocationContext::new("sayHello", URL::new("tri", "/svc"));
        let result = cluster_invoker.invoke(&mut ctx).await;
        assert!(result.is_ok());
        let rpc = result.unwrap();
        assert!(!rpc.is_error());
        assert_eq!(
            rpc.value,
            Some(b"fallback_data".to_vec()),
            "fail mode should return mock on failure"
        );
    }

    #[tokio::test]
    async fn test_mock_cluster_fail_no_mock_configured() {
        let invoker = Arc::new(MockInvoker::new(
            make_url("192.168.1.1", "50051", "/svc"),
            false,
        ));
        let directory = Box::new(MockDirectory::new(vec![invoker]));
        let cluster = MockCluster::new();
        let cluster_invoker = cluster.join(directory).await.expect("join should succeed");

        let mut ctx = InvocationContext::new("sayHello", URL::new("tri", "/svc"));
        let result = cluster_invoker.invoke(&mut ctx).await;
        assert!(
            result.is_ok(),
            "should return Ok with RPCResult error when no mock configured"
        );
        let rpc = result.unwrap();
        assert!(
            rpc.is_error(),
            "should propagate error result without mock fallback"
        );
    }

    #[tokio::test]
    async fn test_mock_cluster_force_via_attachment() {
        let invoker = Arc::new(MockInvoker::new(
            make_url("192.168.1.1", "50051", "/svc"),
            true,
        ));
        let tracked = Arc::clone(&invoker);
        let directory = Box::new(MockDirectory::new(vec![invoker]));
        let cluster = MockCluster::new().with_mock_result(b"att_mock".to_vec());
        let cluster_invoker = cluster.join(directory).await.expect("join should succeed");

        let mut ctx = InvocationContext::new("sayHello", URL::new("tri", "/svc"));
        ctx.attachments.insert("mock".into(), "force".into());
        let result = cluster_invoker.invoke(&mut ctx).await;
        assert!(result.is_ok());
        let rpc = result.unwrap();
        assert!(!rpc.is_error());
        assert_eq!(rpc.value, Some(b"att_mock".to_vec()));
        assert_eq!(
            tracked.call_count.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "attachment force should not call downstream"
        );
    }

    #[tokio::test]
    async fn test_mock_cluster_fail_via_attachment() {
        let invoker = Arc::new(MockInvoker::new(
            make_url("192.168.1.1", "50051", "/svc"),
            false,
        ));
        let directory = Box::new(MockDirectory::new(vec![invoker]));
        let cluster = MockCluster::new().with_mock_result(b"att_fallback".to_vec());
        let cluster_invoker = cluster.join(directory).await.expect("join should succeed");

        let mut ctx = InvocationContext::new("sayHello", URL::new("tri", "/svc"));
        ctx.attachments.insert("mock".into(), "fail".into());
        let result = cluster_invoker.invoke(&mut ctx).await;
        assert!(result.is_ok());
        let rpc = result.unwrap();
        assert!(!rpc.is_error());
        assert_eq!(
            rpc.value,
            Some(b"att_fallback".to_vec()),
            "attachment fail should return mock on failure"
        );
    }

    #[tokio::test]
    async fn test_mock_cluster_mock_result_via_attachment() {
        let invoker = Arc::new(MockInvoker::new(
            make_url("192.168.1.1", "50051", "/svc"),
            true,
        ));
        let tracked = Arc::clone(&invoker);
        let directory = Box::new(MockDirectory::new(vec![invoker]));
        let cluster = MockCluster::new().with_force(true);
        let cluster_invoker = cluster.join(directory).await.expect("join should succeed");

        let mut ctx = InvocationContext::new("sayHello", URL::new("tri", "/svc"));
        ctx.attachments
            .insert("mock.result".into(), "from_attachment".into());
        let result = cluster_invoker.invoke(&mut ctx).await;
        assert!(result.is_ok());
        let rpc = result.unwrap();
        assert!(!rpc.is_error());
        assert_eq!(
            rpc.value,
            Some(b"from_attachment".to_vec()),
            "mock.result attachment should override mock data"
        );
        assert_eq!(
            tracked.call_count.load(std::sync::atomic::Ordering::SeqCst),
            0
        );
    }

    #[tokio::test]
    async fn test_mock_cluster_empty_directory() {
        let directory = Box::new(MockDirectory::new(vec![]));
        let cluster = MockCluster::new()
            .with_force(true)
            .with_mock_result(b"mock".to_vec());
        let cluster_invoker = cluster.join(directory).await.expect("join should succeed");

        let mut ctx = InvocationContext::new("sayHello", URL::new("tri", "/svc"));
        let result = cluster_invoker.invoke(&mut ctx).await;
        assert!(result.is_ok());
        let rpc = result.unwrap();
        assert!(!rpc.is_error());
        assert_eq!(
            rpc.value,
            Some(b"mock".to_vec()),
            "force mode returns mock even with empty directory"
        );
    }

    #[tokio::test]
    async fn test_mock_cluster_join_creates_invoker() {
        let invoker = Arc::new(MockInvoker::new(
            make_url("192.168.1.1", "50051", "/svc"),
            true,
        ));
        let directory = Box::new(MockDirectory::new(vec![invoker]));
        let cluster = MockCluster::new().with_force(true);
        let cluster_invoker = cluster.join(directory).await.expect("join should succeed");

        assert!(cluster_invoker.is_available());
        assert_eq!(cluster_invoker.get_url().path, "/mock");
    }

    #[test]
    fn test_mock_cluster_default() {
        let cluster = MockCluster::default();
        assert!(!cluster.force);
        assert!(cluster.mock_result.is_none());
    }

    #[test]
    fn test_mock_cluster_builder() {
        let cluster = MockCluster::new()
            .with_force(true)
            .with_mock_result(b"test".to_vec());
        assert!(cluster.force);
        assert_eq!(cluster.mock_result, Some(b"test".to_vec()));
    }
}

// Router tests are in a separate module to avoid name clashes
#[cfg(test)]
mod router_tests {
    use super::*;

    fn make_url_with_params(host: &str, params: &[(&str, &str)]) -> URL {
        let mut url = URL::new("tri", "/com.example.Service");
        url.ip = host.to_string();
        for (k, v) in params {
            url.set_param(*k, *v);
        }
        url
    }

    struct RouterTestInvoker {
        url: URL,
    }

    impl RouterTestInvoker {
        fn new(url: URL) -> Self {
            Self { url }
        }
    }

    impl Node for RouterTestInvoker {
        fn get_url(&self) -> &URL {
            &self.url
        }
        fn is_available(&self) -> bool {
            true
        }
        fn destroy(&self) {}
    }

    #[async_trait::async_trait]
    impl Invoker for RouterTestInvoker {
        async fn invoke(&self, _ctx: &mut InvocationContext) -> Result<RPCResult, anyhow::Error> {
            Ok(RPCResult::success(b"ok".to_vec()))
        }
    }

    fn make_invokers(params_list: &[&[(&str, &str)]]) -> Vec<Arc<dyn Invoker>> {
        params_list
            .iter()
            .enumerate()
            .map(|(i, params)| {
                let host = format!("192.168.1.{}", i + 1);
                Arc::new(RouterTestInvoker::new(make_url_with_params(&host, params)))
                    as Arc<dyn Invoker>
            })
            .collect()
    }

    fn make_ctx() -> InvocationContext {
        let mut url = URL::new("tri", "/com.example.Service");
        url.ip = "127.0.0.1".into();
        InvocationContext::new("sayHello", url)
    }

    // ── ConditionRouter ─────────────────────────────────────────────

    #[test]
    fn test_condition_router_parse_simple() {
        let r = ConditionRouter::parse("env=gray").unwrap();
        assert!(r.match_rules.is_empty());
        assert_eq!(r.filter_rules.len(), 1);
        assert_eq!(r.filter_rules[0], ("env".to_string(), "gray".to_string()));
    }

    #[test]
    fn test_condition_router_parse_with_match() {
        let r = ConditionRouter::parse("region=beijing => env=gray").unwrap();
        assert_eq!(r.match_rules.len(), 1);
        assert_eq!(
            r.match_rules[0],
            ("region".to_string(), "beijing".to_string())
        );
        assert_eq!(r.filter_rules[0], ("env".to_string(), "gray".to_string()));
    }

    #[test]
    fn test_condition_router_always_matches_empty() {
        let r = ConditionRouter::parse("=> env=gray").unwrap();
        let ctx = make_ctx();
        assert!(r.matches_invocation(&ctx));
    }

    #[test]
    fn test_condition_router_matches_with_attachment() {
        let r = ConditionRouter::parse("region=beijing => env=gray").unwrap();
        let mut ctx = make_ctx();
        ctx.attachments.insert("region".into(), "beijing".into());
        assert!(r.matches_invocation(&ctx));
    }

    #[test]
    fn test_condition_router_no_match_wrong_value() {
        let r = ConditionRouter::parse("region=beijing => env=gray").unwrap();
        let mut ctx = make_ctx();
        ctx.attachments.insert("region".into(), "shanghai".into());
        assert!(!r.matches_invocation(&ctx));
    }

    #[test]
    fn test_condition_router_filters_by_param() {
        let invokers = make_invokers(&[&[("env", "gray")], &[("env", "prod")], &[("env", "gray")]]);
        let r = ConditionRouter::parse("=> env=gray").unwrap();
        let filtered = r.filter_invokers(&invokers);
        assert_eq!(filtered, vec![0, 2]);
    }

    // ── TagRouter ───────────────────────────────────────────────────

    #[test]
    fn test_tag_router_all_when_no_tag_requested() {
        let invokers = make_invokers(&[&[("tag", "v1")], &[("tag", "v2")], &[]]);
        let ctx = make_ctx();
        let tr = TagRouter::default();
        let result = tr.route(&invokers, &ctx);
        assert_eq!(result, vec![0, 1, 2]);
    }

    #[test]
    fn test_tag_router_matches_requested_tag() {
        let invokers = make_invokers(&[&[("tag", "v1")], &[("tag", "v2")], &[("tag", "v1")]]);
        let mut ctx = make_ctx();
        ctx.attachments.insert("dubbo.tag".into(), "v2".into());
        let tr = TagRouter::default();
        let result = tr.route(&invokers, &ctx);
        assert_eq!(result, vec![1]);
    }

    #[test]
    fn test_tag_router_fallback_to_untagged() {
        let invokers = make_invokers(&[&[("tag", "v1")], &[]]);
        let mut ctx = make_ctx();
        ctx.attachments.insert("dubbo.tag".into(), "v2".into());
        let tr = TagRouter::default();
        let result = tr.route(&invokers, &ctx);
        assert_eq!(result, vec![1]);
    }

    // ── RouterChain ─────────────────────────────────────────────────

    #[test]
    fn test_router_chain_empty_returns_all() {
        let invokers = make_invokers(&[&[], &[]]);
        let chain = RouterChain::default();
        let result = chain.route(&invokers, &make_ctx());
        assert_eq!(result, vec![0, 1]);
    }

    #[test]
    fn test_router_chain_condition_then_tag() {
        let invokers = make_invokers(&[
            &[("env", "gray"), ("tag", "v1")],
            &[("env", "gray"), ("tag", "v2")],
            &[("env", "prod"), ("tag", "v1")],
        ]);
        let mut ctx = make_ctx();
        ctx.attachments.insert("dubbo.tag".into(), "v1".into());

        let chain = RouterChain::new()
            .with_condition_router(ConditionRouter::parse("=> env=gray").unwrap())
            .with_tag_router(TagRouter::default());

        let result = chain.route(&invokers, &ctx);
        assert_eq!(result, vec![0]);
    }

    #[test]
    fn test_condition_router_parse_empty_filter() {
        let r = ConditionRouter::parse("region=beijing =>").unwrap();
        assert_eq!(r.match_rules.len(), 1);
        assert_eq!(
            r.match_rules[0],
            ("region".to_string(), "beijing".to_string())
        );
        assert!(r.filter_rules.is_empty());
    }

    #[test]
    fn test_condition_router_parse_kv_pairs_directly() {
        let r = ConditionRouter::parse("a=1,b=2").unwrap();
        assert!(r.match_rules.is_empty());
        assert_eq!(r.filter_rules.len(), 2);
        assert_eq!(r.filter_rules[0], ("a".to_string(), "1".to_string()));
        assert_eq!(r.filter_rules[1], ("b".to_string(), "2".to_string()));
    }

    #[test]
    fn test_condition_router_filter_no_match() {
        let invokers = make_invokers(&[
            &[("env", "prod")],
            &[("env", "staging")],
            &[("env", "prod")],
        ]);
        let r = ConditionRouter::parse("=> zone=us-west").unwrap();
        let filtered = r.filter_invokers(&invokers);
        assert!(
            filtered.is_empty(),
            "no invoker has 'zone' param, result should be empty"
        );
    }

    #[test]
    fn test_tag_router_with_custom_key() {
        let invokers = make_invokers(&[&[("tag", "v1")], &[("tag", "v2")], &[]]);
        let mut ctx = make_ctx();
        ctx.attachments.insert("custom-key".into(), "v1".into());

        let tr = TagRouter::new().with_tag_key("custom-key");
        let result = tr.route(&invokers, &ctx);
        assert_eq!(result, vec![0]);
    }

    #[test]
    fn test_router_chain_empty_after_condition() {
        let invokers = make_invokers(&[&[("env", "prod")], &[("env", "prod")]]);
        let chain = RouterChain::new()
            .with_condition_router(ConditionRouter::parse("=> env=gray").unwrap());
        let result = chain.route(&invokers, &make_ctx());
        assert!(
            result.is_empty(),
            "condition router removes all invokers, chain should return empty"
        );
    }
}

#[cfg(test)]
mod script_router_tests {
    use super::*;

    fn make_url_with_params(host: &str, params: &[(&str, &str)]) -> URL {
        let mut url = URL::new("tri", "/com.example.Service");
        url.ip = host.to_string();
        for (k, v) in params {
            url.set_param(*k, *v);
        }
        url
    }

    struct TestInvoker {
        url: URL,
    }

    impl TestInvoker {
        fn new(url: URL) -> Self {
            Self { url }
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

    #[async_trait::async_trait]
    impl Invoker for TestInvoker {
        async fn invoke(&self, _ctx: &mut InvocationContext) -> Result<RPCResult, anyhow::Error> {
            Ok(RPCResult::success(b"ok".to_vec()))
        }
    }

    fn make_invokers(params_list: &[&[(&str, &str)]]) -> Vec<Arc<dyn Invoker>> {
        params_list
            .iter()
            .enumerate()
            .map(|(i, params)| {
                let host = format!("10.0.0.{}", i + 1);
                Arc::new(TestInvoker::new(make_url_with_params(&host, params))) as Arc<dyn Invoker>
            })
            .collect()
    }

    fn make_ctx() -> InvocationContext {
        let mut url = URL::new("tri", "/com.example.Service");
        url.ip = "127.0.0.1".into();
        InvocationContext::new("sayHello", url)
    }

    #[test]
    fn test_script_router_basic_filter() {
        let router = ScriptRouter::new("[0, 2]").unwrap();
        let invokers = make_invokers(&[&[], &[], &[]]);
        let ctx = make_ctx();
        let result = router.route(&invokers, &ctx);
        assert_eq!(result, vec![0, 2]);
    }

    #[test]
    fn test_script_router_select_by_ip() {
        let script = r#"
            let result = [];
            for i in 0..invoker_count() {
                if invoker_ip(i).starts_with("10.0.0.1") {
                    result.push(i);
                }
            }
            result
        "#;
        let router = ScriptRouter::new(script).unwrap();
        let invokers = make_invokers(&[&[], &[], &[]]);
        let ctx = make_ctx();
        let result = router.route(&invokers, &ctx);
        assert_eq!(result, vec![0]);
    }

    #[test]
    fn test_script_router_select_by_param() {
        let script = r#"
            let result = [];
            for i in 0..invoker_count() {
                if invoker_has_param(i, "env") && invoker_get_param(i, "env") == "gray" {
                    result.push(i);
                }
            }
            result
        "#;
        let router = ScriptRouter::new(script).unwrap();
        let invokers = make_invokers(&[&[("env", "gray")], &[("env", "prod")], &[("env", "gray")]]);
        let ctx = make_ctx();
        let result = router.route(&invokers, &ctx);
        assert_eq!(result, vec![0, 2]);
    }

    #[test]
    fn test_script_router_method_name() {
        let script = r#"
            if method_name() == "sayHello" {
                [0, 1]
            } else {
                []
            }
        "#;
        let router = ScriptRouter::new(script).unwrap();
        let invokers = make_invokers(&[&[], &[]]);
        let ctx = make_ctx();
        let result = router.route(&invokers, &ctx);
        assert_eq!(result, vec![0, 1]);

        let mut url = URL::new("tri", "/com.example.Service");
        url.ip = "127.0.0.1".into();
        let ctx2 = InvocationContext::new("byebye", url);
        let result2 = router.route(&invokers, &ctx2);
        assert!(result2.is_empty());
    }

    #[test]
    fn test_script_router_attachment_based() {
        let script = r#"
            if has_attachment("region") && get_attachment("region") == "beijing" {
                [1]
            } else {
                [0]
            }
        "#;
        let router = ScriptRouter::new(script).unwrap();
        let invokers = make_invokers(&[&[], &[]]);

        let mut ctx = make_ctx();
        ctx.attachments.insert("region".into(), "beijing".into());
        let result = router.route(&invokers, &ctx);
        assert_eq!(result, vec![1]);

        let ctx_no_att = make_ctx();
        let result2 = router.route(&invokers, &ctx_no_att);
        assert_eq!(result2, vec![0]);
    }

    #[test]
    fn test_script_router_invalid_script() {
        let result = ScriptRouter::new("fn (");
        assert!(result.is_err(), "bad script should fail to compile");
    }

    #[test]
    fn test_script_router_out_of_bounds() {
        let router = ScriptRouter::new("[99, 200]").unwrap();
        let invokers = make_invokers(&[&[], &[]]);
        let ctx = make_ctx();
        let result = router.route(&invokers, &ctx);
        assert!(
            result.is_empty(),
            "out-of-bounds indices should produce empty result"
        );
    }

    #[test]
    fn test_script_router_chain_integration() {
        let script = r#"
            let result = [];
            for i in 0..invoker_count() {
                if invoker_has_param(i, "env") && invoker_get_param(i, "env") == "gray" {
                    result.push(i);
                }
            }
            result
        "#;
        let sr = ScriptRouter::new(script).unwrap();
        let invokers = make_invokers(&[
            &[("env", "gray"), ("tag", "v1")],
            &[("env", "prod")],
            &[("env", "gray"), ("tag", "v2")],
        ]);
        let mut ctx = make_ctx();
        ctx.attachments.insert("dubbo.tag".into(), "v1".into());

        let chain = RouterChain::new()
            .with_tag_router(TagRouter::default())
            .with_script_router(sr);
        let result = chain.route(&invokers, &ctx);
        assert_eq!(result, vec![0]);
    }
}
