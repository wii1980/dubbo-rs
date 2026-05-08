pub use dubbo_rs_common;
pub use dubbo_rs_tls;

use anyhow::Result;
use async_trait::async_trait;
use dubbo_rs_common::url::URL;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub id: u64,
    pub is_twoway: bool,
    pub is_event: bool,
    pub data: Vec<u8>,
}

impl Request {
    #[must_use]
    pub fn new(id: u64, is_twoway: bool, data: Vec<u8>) -> Self {
        Self {
            id,
            is_twoway,
            is_event: false,
            data,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub id: u64,
    pub status: u8,
    pub data: Vec<u8>,
}

impl Response {
    #[must_use]
    pub fn new(id: u64, status: u8, data: Vec<u8>) -> Self {
        Self { id, status, data }
    }

    #[must_use]
    pub fn success(id: u64, data: Vec<u8>) -> Self {
        Self::new(id, dubbo_rs_common::constants::OK_STATUS, data)
    }

    #[must_use]
    pub fn error(id: u64, status: u8, data: Vec<u8>) -> Self {
        Self::new(id, status, data)
    }

    #[must_use]
    pub fn is_error(&self) -> bool {
        self.status != dubbo_rs_common::constants::OK_STATUS
    }
}

pub trait Codec: Send + Sync {
    /// Encode a request into wire-format bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if encoding fails.
    fn encode_request(&self, req: &Request) -> Result<Vec<u8>>;

    /// Decode wire-format bytes into a request.
    ///
    /// # Errors
    ///
    /// Returns an error if the data is malformed.
    fn decode_request(&self, data: &[u8]) -> Result<Request>;

    /// Encode a response into wire-format bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if encoding fails.
    fn encode_response(&self, resp: &Response) -> Result<Vec<u8>>;

    /// Decode wire-format bytes into a response.
    ///
    /// # Errors
    ///
    /// Returns an error if the data is malformed.
    fn decode_response(&self, data: &[u8]) -> Result<Response>;
}

#[async_trait]
pub trait ExchangeClient: Send + Sync {
    /// Connect to a remote endpoint.
    async fn connect(&mut self, url: &URL) -> Result<()>;

    /// Send a request and await a response.
    async fn request(&self, req: Request) -> Result<Response>;

    /// Close the client connection.
    fn close(&self);
}

#[async_trait]
pub trait ExchangeServer: Send + Sync {
    /// Bind to a local address and start serving.
    async fn bind(&self, url: &URL) -> Result<()>;

    /// Gracefully shut down the server.
    async fn close(&self);
}

pub use pool::{PoolConfig, PooledConnection, PooledConnectionPool};
pub use reconnect::{KeepAliveConfig, ReconnectConfig, ReconnectManager};

#[async_trait]
pub trait ConnectionPool: Send + Sync {
    /// Get a connection from the pool for the given URL.
    ///
    /// Returns a `PooledConnection` that will automatically return
    /// the underlying client to the pool when dropped.
    ///
    /// # Errors
    ///
    /// Returns an error if connection creation fails or the pool is exhausted.
    async fn get(&self, url: &URL) -> Result<PooledConnection>;
}

pub mod pool {
    use std::collections::HashMap;
    use std::ops::Deref;
    #[cfg(test)]
    use std::sync::atomic::Ordering;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use anyhow::{Context, Result};
    use async_trait::async_trait;
    use dubbo_rs_common::url::URL;
    use tokio::sync::Mutex;

    use crate::{ConnectionPool, ExchangeClient};

    struct PoolEntry {
        client: Box<dyn ExchangeClient>,
        last_used: Instant,
    }

    struct PoolState {
        available: HashMap<String, Vec<PoolEntry>>,
        in_use: HashMap<String, usize>,
    }

    impl PoolState {
        fn new() -> Self {
            Self {
                available: HashMap::new(),
                in_use: HashMap::new(),
            }
        }
    }

    /// A connection wrapper that returns the underlying client to the pool on drop.
    ///
    /// Implements `Deref<Target = dyn ExchangeClient>` so it can be used
    /// transparently wherever an `ExchangeClient` reference is needed.
    pub struct PooledConnection {
        client: Option<Box<dyn ExchangeClient>>,
        key: String,
        state: Arc<Mutex<PoolState>>,
        max_idle: usize,
    }

    impl PooledConnection {
        /// Check if the underlying connection is still present (not already returned).
        #[must_use]
        pub fn is_valid(&self) -> bool {
            self.client.is_some()
        }
    }

    impl Deref for PooledConnection {
        type Target = dyn ExchangeClient;

        fn deref(&self) -> &Self::Target {
            self.client
                .as_ref()
                .expect("PooledConnection client already taken")
                .as_ref()
        }
    }

    impl Drop for PooledConnection {
        fn drop(&mut self) {
            if let Some(client) = self.client.take() {
                let state = self.state.clone();
                let key = self.key.clone();
                let max_idle = self.max_idle;
                // Best-effort async return
                if let Ok(handle) = tokio::runtime::Handle::try_current() {
                    handle.spawn(async move {
                        let mut guard = state.lock().await;
                        if let Some(count) = guard.in_use.get_mut(&key) {
                            *count = count.saturating_sub(1);
                        }
                        let idle_list = guard.available.entry(key).or_default();
                        if idle_list.len() < max_idle {
                            idle_list.push(PoolEntry {
                                client,
                                last_used: Instant::now(),
                            });
                        }
                    });
                }
            }
        }
    }

    /// Configuration for `PooledConnectionPool`.
    pub struct PoolConfig {
        /// Maximum number of simultaneous connections per address.
        pub max_connections_per_address: usize,
        /// Maximum number of idle connections kept per address.
        pub max_idle_connections: usize,
        /// Time after which an idle connection is considered stale.
        pub idle_timeout: Duration,
    }

    impl Default for PoolConfig {
        fn default() -> Self {
            Self {
                max_connections_per_address: 8,
                max_idle_connections: 4,
                idle_timeout: Duration::from_secs(30),
            }
        }
    }

    impl PoolConfig {
        /// Build config from URL parameters.
        ///
        /// Reads `"connections"` parameter for `max_connections_per_address`.
        #[must_use]
        pub fn from_url(url: &URL) -> Self {
            let mut config = Self::default();
            if let Some(conns) = url.get_param("connections") {
                if let Ok(n) = conns.parse::<usize>() {
                    config.max_connections_per_address = n.max(1);
                }
            }
            config
        }
    }

    /// A simple connection pool that caches and reuses connections per address.
    ///
    /// Connections are returned to the pool when the `PooledConnection` is
    /// dropped, allowing subsequent `get()` calls to reuse them.
    pub struct SimpleConnectionPool<F> {
        factory: F,
        state: Arc<Mutex<PoolState>>,
    }

    impl<F> SimpleConnectionPool<F>
    where
        F: Fn() -> Box<dyn ExchangeClient> + Send + Sync + 'static,
    {
        pub fn new(factory: F) -> Self {
            Self {
                factory,
                state: Arc::new(Mutex::new(PoolState::new())),
            }
        }
    }

    #[async_trait]
    impl<F> ConnectionPool for SimpleConnectionPool<F>
    where
        F: Fn() -> Box<dyn ExchangeClient> + Send + Sync + 'static,
    {
        async fn get(&self, url: &URL) -> Result<PooledConnection> {
            let key = url.get_address();
            let mut guard = self.state.lock().await;

            if let Some(idle_list) = guard.available.get_mut(&key) {
                if let Some(entry) = idle_list.pop() {
                    return Ok(PooledConnection {
                        client: Some(entry.client),
                        key,
                        state: self.state.clone(),
                        max_idle: usize::MAX,
                    });
                }
            }

            let mut client = (self.factory)();
            client
                .connect(url)
                .await
                .with_context(|| format!("failed to connect to {key}"))?;

            Ok(PooledConnection {
                client: Some(client),
                key,
                state: self.state.clone(),
                max_idle: usize::MAX,
            })
        }
    }

    /// A feature-rich connection pool with per-address limits and idle eviction.
    ///
    /// - Maintains separate connection pools per remote address.
    /// - Enforces `max_connections_per_address` limit.
    /// - Returns idle connections to the pool via `PooledConnection::Drop`.
    /// - Background task periodically evicts connections idle longer than
    ///   `idle_timeout`.
    pub struct PooledConnectionPool<F> {
        factory: F,
        config: PoolConfig,
        state: Arc<Mutex<PoolState>>,
        cleanup_handle: Option<tokio::task::JoinHandle<()>>,
    }

    impl<F> PooledConnectionPool<F>
    where
        F: Fn() -> Box<dyn ExchangeClient> + Send + Sync + 'static,
    {
        /// Create a new pooled connection pool with the given config and factory.
        ///
        /// The factory closure is called to create a new `ExchangeClient`
        /// when no idle connection is available.
        pub fn new(config: PoolConfig, factory: F) -> Self {
            let state = Arc::new(Mutex::new(PoolState::new()));

            let cleanup_state = state.clone();
            let idle_timeout = config.idle_timeout;
            let cleanup_handle = tokio::spawn(async move {
                let mut interval = tokio::time::interval(idle_timeout);
                loop {
                    interval.tick().await;
                    let mut guard = cleanup_state.lock().await;
                    for idle_list in guard.available.values_mut() {
                        idle_list.retain(|entry| entry.last_used.elapsed() < idle_timeout);
                    }
                }
            });

            Self {
                factory,
                config,
                state,
                cleanup_handle: Some(cleanup_handle),
            }
        }

        /// Returns the number of currently idle connections for the given address.
        ///
        /// Useful for testing.
        pub async fn idle_count(&self, address: &str) -> usize {
            let guard = self.state.lock().await;
            guard.available.get(address).map_or(0, Vec::len)
        }

        /// Returns the number of currently in-use connections for the given address.
        ///
        /// Useful for testing.
        pub async fn in_use_count(&self, address: &str) -> usize {
            let guard = self.state.lock().await;
            guard.in_use.get(address).copied().unwrap_or(0)
        }
    }

    impl<F> Drop for PooledConnectionPool<F> {
        fn drop(&mut self) {
            if let Some(handle) = self.cleanup_handle.take() {
                handle.abort();
            }
        }
    }

    #[async_trait]
    impl<F> ConnectionPool for PooledConnectionPool<F>
    where
        F: Fn() -> Box<dyn ExchangeClient> + Send + Sync + 'static,
    {
        async fn get(&self, url: &URL) -> Result<PooledConnection> {
            let key = url.get_address();
            let mut guard = self.state.lock().await;

            if let Some(idle_list) = guard.available.get_mut(&key) {
                while let Some(entry) = idle_list.pop() {
                    if entry.last_used.elapsed() < self.config.idle_timeout {
                        *guard.in_use.entry(key.clone()).or_insert(0) += 1;
                        return Ok(PooledConnection {
                            client: Some(entry.client),
                            key,
                            state: self.state.clone(),
                            max_idle: self.config.max_idle_connections,
                        });
                    }
                }
            }

            let in_use = guard.in_use.get(&key).copied().unwrap_or(0);
            if in_use >= self.config.max_connections_per_address {
                anyhow::bail!(
                    "connection pool for {key} is exhausted ({in_use}/{} connections in use)",
                    self.config.max_connections_per_address
                );
            }

            *guard.in_use.entry(key.clone()).or_insert(0) += 1;
            drop(guard);

            let mut client = (self.factory)();
            match client.connect(url).await {
                Ok(()) => Ok(PooledConnection {
                    client: Some(client),
                    key,
                    state: self.state.clone(),
                    max_idle: self.config.max_idle_connections,
                }),
                Err(e) => {
                    let mut guard = self.state.lock().await;
                    if let Some(count) = guard.in_use.get_mut(&key) {
                        *count = count.saturating_sub(1);
                    }
                    Err(e.context(format!("failed to connect to {key}")))
                }
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::{ExchangeClient, Request, Response};
        use std::sync::atomic::AtomicUsize;

        struct MockExchangeClient;

        #[async_trait]
        impl ExchangeClient for MockExchangeClient {
            async fn connect(&mut self, _url: &URL) -> Result<()> {
                Ok(())
            }

            async fn request(&self, _req: Request) -> Result<Response> {
                Ok(Response::success(1, b"mock-response".to_vec()))
            }

            fn close(&self) {}
        }

        #[tokio::test]
        async fn test_simple_connection_pool_new() {
            let _pool = SimpleConnectionPool::new(|| Box::new(MockExchangeClient));
        }

        #[tokio::test]
        async fn test_simple_connection_pool_get() {
            let pool = SimpleConnectionPool::new(|| Box::new(MockExchangeClient));
            let url = URL::new("tri", "127.0.0.1:20880");
            let client = pool.get(&url).await.expect("get should succeed");
            let resp = client
                .request(Request::new(1, true, vec![]))
                .await
                .expect("request should succeed");
            assert!(!resp.is_error());
            assert_eq!(resp.data, b"mock-response");
        }

        #[tokio::test]
        async fn test_simple_connection_pool_both_gets_succeed() {
            let pool = SimpleConnectionPool::new(|| Box::new(MockExchangeClient));
            let url = URL::new("tri", "127.0.0.1:20880");
            let _client1 = pool.get(&url).await.expect("first get ok");
            let _client2 = pool.get(&url).await.expect("second get ok");
        }

        #[tokio::test]
        async fn test_simple_connection_pool_caches() {
            let create_count = Arc::new(AtomicUsize::new(0));
            let create_clone = create_count.clone();
            let pool = SimpleConnectionPool::new(move || {
                create_clone.fetch_add(1, Ordering::SeqCst);
                Box::new(MockExchangeClient)
            });
            let url = URL::new("tri", "127.0.0.1:20880");

            {
                let _conn = pool.get(&url).await.expect("get ok");
                assert_eq!(create_count.load(Ordering::SeqCst), 1);
            }

            tokio::task::yield_now().await;

            let _conn = pool.get(&url).await.expect("get ok");
            assert_eq!(
                create_count.load(Ordering::SeqCst),
                1,
                "should reuse cached connection, not create a new one"
            );
        }

        #[tokio::test]
        async fn test_pooled_connection_deref() {
            let pool = SimpleConnectionPool::new(|| Box::new(MockExchangeClient));
            let url = URL::new("tri", "127.0.0.1:20880");
            let conn = pool.get(&url).await.expect("get ok");

            let resp = conn
                .request(Request::new(42, true, b"hello".to_vec()))
                .await
                .expect("request ok");
            assert_eq!(resp.id, 1);
            assert_eq!(resp.data, b"mock-response");
        }

        #[tokio::test]
        async fn test_pooled_connection_is_valid() {
            let pool = SimpleConnectionPool::new(|| Box::new(MockExchangeClient));
            let url = URL::new("tri", "127.0.0.1:20880");
            let conn = pool.get(&url).await.expect("get ok");
            assert!(conn.is_valid());
        }

        #[tokio::test]
        async fn test_pooled_connection_pool_get_and_return() {
            let create_count = Arc::new(AtomicUsize::new(0));
            let create_clone = create_count.clone();
            let pool = PooledConnectionPool::new(PoolConfig::default(), move || {
                create_clone.fetch_add(1, Ordering::SeqCst);
                Box::new(MockExchangeClient)
            });
            let url = URL::new("tri", "127.0.0.1:20880");
            let addr = url.get_address();

            {
                let conn = pool.get(&url).await.expect("get ok");
                assert!(conn.is_valid());
                assert_eq!(create_count.load(Ordering::SeqCst), 1);
                assert_eq!(pool.in_use_count(&addr).await, 1);
                assert_eq!(pool.idle_count(&addr).await, 0);
            }

            tokio::task::yield_now().await;

            assert_eq!(pool.in_use_count(&addr).await, 0);
            assert_eq!(pool.idle_count(&addr).await, 1);
        }

        #[tokio::test]
        async fn test_pooled_connection_pool_reuse() {
            let create_count = Arc::new(AtomicUsize::new(0));
            let create_clone = create_count.clone();
            let pool = PooledConnectionPool::new(PoolConfig::default(), move || {
                create_clone.fetch_add(1, Ordering::SeqCst);
                Box::new(MockExchangeClient)
            });
            let url = URL::new("tri", "127.0.0.1:20880");

            {
                let _conn = pool.get(&url).await.expect("get 1 ok");
                assert_eq!(create_count.load(Ordering::SeqCst), 1);
            }
            tokio::task::yield_now().await;

            let _conn = pool.get(&url).await.expect("get 2 ok");
            assert_eq!(
                create_count.load(Ordering::SeqCst),
                1,
                "should reuse the returned connection"
            );
        }

        #[tokio::test]
        async fn test_pooled_connection_pool_max_connections() {
            let config = PoolConfig {
                max_connections_per_address: 2,
                max_idle_connections: 2,
                idle_timeout: Duration::from_secs(300),
            };
            let pool = PooledConnectionPool::new(config, || Box::new(MockExchangeClient));
            let url = URL::new("tri", "127.0.0.1:20880");

            let _conn1 = pool.get(&url).await.expect("get 1 ok");
            let _conn2 = pool.get(&url).await.expect("get 2 ok");

            let result = pool.get(&url).await;
            assert!(result.is_err(), "should fail when pool is exhausted");
            let err_msg = match result {
                Err(e) => format!("{e}"),
                Ok(_) => String::from("unexpected Ok"),
            };
            assert!(
                err_msg.contains("exhausted"),
                "error should mention exhaustion: {err_msg}"
            );
        }

        #[tokio::test]
        async fn test_pooled_connection_pool_idle_timeout() {
            let config = PoolConfig {
                max_connections_per_address: 8,
                max_idle_connections: 4,
                idle_timeout: Duration::from_millis(50),
            };
            let create_count = Arc::new(AtomicUsize::new(0));
            let create_clone = create_count.clone();
            let pool = PooledConnectionPool::new(config, move || {
                create_clone.fetch_add(1, Ordering::SeqCst);
                Box::new(MockExchangeClient)
            });
            let url = URL::new("tri", "127.0.0.1:20880");

            {
                let _conn = pool.get(&url).await.expect("get ok");
            }
            tokio::task::yield_now().await;
            assert_eq!(pool.idle_count(&url.get_address()).await, 1);
            assert_eq!(create_count.load(Ordering::SeqCst), 1);

            tokio::time::sleep(Duration::from_millis(100)).await;

            let _conn = pool.get(&url).await.expect("get after timeout ok");
            assert_eq!(
                create_count.load(Ordering::SeqCst),
                2,
                "should create a new connection after idle timeout"
            );
        }

        #[tokio::test]
        async fn test_pooled_connection_pool_config_from_url() {
            let mut url = URL::new("dubbo", "127.0.0.1:20880");
            url.set_param("connections", "16");
            let config = PoolConfig::from_url(&url);
            assert_eq!(config.max_connections_per_address, 16);
        }

        #[tokio::test]
        async fn test_pooled_connection_pool_config_default() {
            let config = PoolConfig::default();
            assert_eq!(config.max_connections_per_address, 8);
            assert_eq!(config.max_idle_connections, 4);
            assert_eq!(config.idle_timeout, Duration::from_secs(30));
        }

        #[tokio::test]
        async fn test_pooled_connection_pool_multiple_addresses() {
            let pool =
                PooledConnectionPool::new(PoolConfig::default(), || Box::new(MockExchangeClient));
            let mut url1 = URL::new("tri", "/svc1");
            url1.ip = "10.0.0.1".to_string();
            url1.port = "20880".to_string();
            let mut url2 = URL::new("tri", "/svc2");
            url2.ip = "10.0.0.2".to_string();
            url2.port = "20880".to_string();

            let _conn1 = pool.get(&url1).await.expect("addr1 ok");
            let _conn2 = pool.get(&url2).await.expect("addr2 ok");

            assert_eq!(pool.in_use_count(&url1.get_address()).await, 1);
            assert_eq!(pool.in_use_count(&url2.get_address()).await, 1);
        }

        #[tokio::test]
        async fn test_pooled_connection_pool_connect_failure_rollback() {
            struct FailingClient;

            #[async_trait]
            impl ExchangeClient for FailingClient {
                async fn connect(&mut self, _url: &URL) -> Result<()> {
                    anyhow::bail!("connection refused")
                }
                async fn request(&self, _req: Request) -> Result<Response> {
                    unreachable!()
                }
                fn close(&self) {}
            }

            let pool = PooledConnectionPool::new(PoolConfig::default(), || Box::new(FailingClient));
            let url = URL::new("tri", "127.0.0.1:20880");
            let addr = url.get_address();

            let result = pool.get(&url).await;
            assert!(result.is_err());

            assert_eq!(pool.in_use_count(&addr).await, 0);
        }
    }
}

pub mod reconnect {
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::time::Duration;

    use dubbo_rs_common::url::URL;

    /// TCP keepalive configuration.
    #[derive(Debug, Clone)]
    pub struct KeepAliveConfig {
        /// Time after which TCP keepalive probes are sent (default: 60s).
        pub idle_time: Duration,
        /// Interval between keepalive probes (default: 10s).
        pub interval: Duration,
        /// Number of failed probes before connection is considered dead (default: 3).
        pub retry_count: u32,
    }

    impl Default for KeepAliveConfig {
        fn default() -> Self {
            Self {
                idle_time: Duration::from_secs(60),
                interval: Duration::from_secs(10),
                retry_count: 3,
            }
        }
    }

    impl KeepAliveConfig {
        /// Build config from URL parameters.
        ///
        /// Reads `keepalive_idle`, `keepalive_interval`, `keepalive_count` parameters.
        #[must_use]
        pub fn from_url(url: &URL) -> Self {
            let mut config = Self::default();
            if let Some(v) = url.get_param("keepalive_idle") {
                if let Ok(secs) = v.parse::<u64>() {
                    config.idle_time = Duration::from_secs(secs);
                }
            }
            if let Some(v) = url.get_param("keepalive_interval") {
                if let Ok(secs) = v.parse::<u64>() {
                    config.interval = Duration::from_secs(secs);
                }
            }
            if let Some(v) = url.get_param("keepalive_count") {
                if let Ok(n) = v.parse::<u32>() {
                    config.retry_count = n;
                }
            }
            config
        }

        /// Returns `true` if keepalive is effectively disabled (`idle_time` is zero).
        #[must_use]
        pub fn is_disabled(&self) -> bool {
            self.idle_time.is_zero()
        }
    }

    /// Auto-reconnect configuration with exponential backoff.
    #[derive(Debug, Clone)]
    pub struct ReconnectConfig {
        /// Whether auto-reconnect is enabled (default: false).
        pub enabled: bool,
        /// Initial delay before first reconnect attempt (default: 1s).
        pub initial_delay: Duration,
        /// Maximum delay between reconnect attempts (default: 30s).
        pub max_delay: Duration,
        /// Multiplier for exponential backoff (default: 2.0).
        pub backoff_multiplier: f64,
        /// Maximum number of reconnect attempts (default: 10, 0 = unlimited).
        pub max_attempts: u32,
    }

    impl Default for ReconnectConfig {
        fn default() -> Self {
            Self {
                enabled: false,
                initial_delay: Duration::from_secs(1),
                max_delay: Duration::from_secs(30),
                backoff_multiplier: 2.0,
                max_attempts: 10,
            }
        }
    }

    impl ReconnectConfig {
        /// Build config from URL parameters.
        ///
        /// Reads `reconnect`, `reconnect.initial_delay`, `reconnect.max_delay`,
        /// `reconnect.backoff_multiplier`, `reconnect.max_attempts` parameters.
        #[must_use]
        pub fn from_url(url: &URL) -> Self {
            let mut config = Self::default();
            if let Some(v) = url.get_param("reconnect") {
                config.enabled = v == "true";
            }
            if let Some(v) = url.get_param("reconnect.initial_delay") {
                if let Ok(secs) = v.parse::<u64>() {
                    config.initial_delay = Duration::from_secs(secs);
                }
            }
            if let Some(v) = url.get_param("reconnect.max_delay") {
                if let Ok(secs) = v.parse::<u64>() {
                    config.max_delay = Duration::from_secs(secs);
                }
            }
            if let Some(v) = url.get_param("reconnect.backoff_multiplier") {
                if let Ok(m) = v.parse::<f64>() {
                    config.backoff_multiplier = m;
                }
            }
            if let Some(v) = url.get_param("reconnect.max_attempts") {
                if let Ok(n) = v.parse::<u32>() {
                    config.max_attempts = n;
                }
            }
            config
        }
    }

    /// Manages reconnect state per address with exponential backoff.
    pub struct ReconnectManager {
        config: ReconnectConfig,
        attempts: Mutex<HashMap<String, u32>>,
        next_delay: Mutex<HashMap<String, Duration>>,
    }

    impl ReconnectManager {
        /// Create a new `ReconnectManager` with the given configuration.
        #[must_use]
        pub fn new(config: ReconnectConfig) -> Self {
            Self {
                config,
                attempts: Mutex::new(HashMap::new()),
                next_delay: Mutex::new(HashMap::new()),
            }
        }

        /// Returns `true` if a reconnect attempt should be made for the given address.
        ///
        /// Returns `false` if the maximum number of attempts has been reached.
        /// When `max_attempts` is 0, always returns `true` (unlimited attempts).
        ///
        /// # Panics
        ///
        /// Panics if the internal mutex is poisoned.
        pub fn should_reconnect(&self, address: &str) -> bool {
            if !self.config.enabled {
                return false;
            }
            if self.config.max_attempts == 0 {
                return true;
            }
            let attempts = self.attempts.lock().unwrap();
            let count = attempts.get(address).copied().unwrap_or(0);
            count < self.config.max_attempts
        }

        /// Returns the current exponential backoff delay for the given address.
        ///
        /// Returns `initial_delay` if no previous failure has been recorded.
        ///
        /// # Panics
        ///
        /// Panics if the internal mutex is poisoned.
        pub fn next_delay(&self, address: &str) -> Duration {
            let delays = self.next_delay.lock().unwrap();
            delays
                .get(address)
                .copied()
                .unwrap_or(self.config.initial_delay)
        }

        /// Record a successful reconnection, resetting the attempt counter for the address.
        ///
        /// # Panics
        ///
        /// Panics if the internal mutex is poisoned.
        pub fn record_success(&self, address: &str) {
            let mut attempts = self.attempts.lock().unwrap();
            attempts.remove(address);
            let mut delays = self.next_delay.lock().unwrap();
            delays.remove(address);
        }

        /// Record a failed reconnect attempt, incrementing the counter and updating the delay.
        ///
        /// # Panics
        ///
        /// Panics if the internal mutex is poisoned.
        pub fn record_failure(&self, address: &str) {
            let mut attempts = self.attempts.lock().unwrap();
            let count = attempts.entry(address.to_string()).or_insert(0);
            *count += 1;

            let current_delay = self
                .next_delay
                .lock()
                .unwrap()
                .get(address)
                .copied()
                .unwrap_or(self.config.initial_delay);

            let new_delay_secs = (current_delay.as_secs_f64() * self.config.backoff_multiplier)
                .min(self.config.max_delay.as_secs_f64());
            let new_delay = Duration::from_secs_f64(new_delay_secs);

            let mut delays = self.next_delay.lock().unwrap();
            delays.insert(address.to_string(), new_delay);
        }

        /// Fully reset the reconnect state for the given address.
        ///
        /// # Panics
        ///
        /// Panics if the internal mutex is poisoned.
        pub fn reset(&self, address: &str) {
            let mut attempts = self.attempts.lock().unwrap();
            attempts.remove(address);
            let mut delays = self.next_delay.lock().unwrap();
            delays.remove(address);
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_keepalive_config_default() {
            let config = KeepAliveConfig::default();
            assert_eq!(config.idle_time, Duration::from_secs(60));
            assert_eq!(config.interval, Duration::from_secs(10));
            assert_eq!(config.retry_count, 3);
            assert!(!config.is_disabled());
        }

        #[test]
        fn test_keepalive_config_from_url() {
            let mut url = URL::new("tri", "127.0.0.1:20880");
            url.set_param("keepalive_idle", "120");
            url.set_param("keepalive_interval", "20");
            url.set_param("keepalive_count", "5");
            let config = KeepAliveConfig::from_url(&url);
            assert_eq!(config.idle_time, Duration::from_secs(120));
            assert_eq!(config.interval, Duration::from_secs(20));
            assert_eq!(config.retry_count, 5);
        }

        #[test]
        fn test_keepalive_config_zero_idle_disables() {
            let mut url = URL::new("tri", "127.0.0.1:20880");
            url.set_param("keepalive_idle", "0");
            let config = KeepAliveConfig::from_url(&url);
            assert!(config.is_disabled());
            assert!(config.idle_time.is_zero());
        }

        #[test]
        fn test_reconnect_config_default() {
            let config = ReconnectConfig::default();
            assert!(!config.enabled);
            assert_eq!(config.initial_delay, Duration::from_secs(1));
            assert_eq!(config.max_delay, Duration::from_secs(30));
            assert!((config.backoff_multiplier - 2.0).abs() < f64::EPSILON);
            assert_eq!(config.max_attempts, 10);
        }

        #[test]
        fn test_reconnect_config_from_url() {
            let mut url = URL::new("tri", "127.0.0.1:20880");
            url.set_param("reconnect", "true");
            url.set_param("reconnect.initial_delay", "5");
            url.set_param("reconnect.max_delay", "60");
            url.set_param("reconnect.backoff_multiplier", "3.0");
            url.set_param("reconnect.max_attempts", "20");
            let config = ReconnectConfig::from_url(&url);
            assert!(config.enabled);
            assert_eq!(config.initial_delay, Duration::from_secs(5));
            assert_eq!(config.max_delay, Duration::from_secs(60));
            assert!((config.backoff_multiplier - 3.0).abs() < f64::EPSILON);
            assert_eq!(config.max_attempts, 20);
        }

        #[test]
        fn test_reconnect_manager_initial_state() {
            let config = ReconnectConfig {
                enabled: true,
                ..ReconnectConfig::default()
            };
            let mgr = ReconnectManager::new(config);
            assert!(mgr.should_reconnect("127.0.0.1:20880"));
            assert_eq!(mgr.next_delay("127.0.0.1:20880"), Duration::from_secs(1));
        }

        #[test]
        fn test_reconnect_manager_exponential_backoff() {
            let config = ReconnectConfig {
                enabled: true,
                initial_delay: Duration::from_secs(1),
                max_delay: Duration::from_secs(60),
                backoff_multiplier: 2.0,
                max_attempts: 0,
            };
            let mgr = ReconnectManager::new(config);
            let addr = "127.0.0.1:20880";

            assert_eq!(mgr.next_delay(addr), Duration::from_secs(1));

            mgr.record_failure(addr);
            assert_eq!(mgr.next_delay(addr), Duration::from_secs(2));

            mgr.record_failure(addr);
            assert_eq!(mgr.next_delay(addr), Duration::from_secs(4));

            mgr.record_failure(addr);
            assert_eq!(mgr.next_delay(addr), Duration::from_secs(8));
        }

        #[test]
        fn test_reconnect_manager_max_attempts() {
            let config = ReconnectConfig {
                enabled: true,
                max_attempts: 3,
                ..ReconnectConfig::default()
            };
            let mgr = ReconnectManager::new(config);
            let addr = "127.0.0.1:20880";

            assert!(mgr.should_reconnect(addr)); // 0 attempts
            mgr.record_failure(addr);
            assert!(mgr.should_reconnect(addr)); // 1 attempt
            mgr.record_failure(addr);
            assert!(mgr.should_reconnect(addr)); // 2 attempts
            mgr.record_failure(addr);
            assert!(!mgr.should_reconnect(addr)); // 3 attempts = max
        }

        #[test]
        fn test_reconnect_manager_reset() {
            let config = ReconnectConfig {
                enabled: true,
                max_attempts: 2,
                ..ReconnectConfig::default()
            };
            let mgr = ReconnectManager::new(config);
            let addr = "127.0.0.1:20880";

            mgr.record_failure(addr);
            mgr.record_failure(addr);
            assert!(!mgr.should_reconnect(addr));

            mgr.reset(addr);
            assert!(mgr.should_reconnect(addr));
            assert_eq!(mgr.next_delay(addr), Duration::from_secs(1));
        }

        #[test]
        fn test_reconnect_manager_record_success_resets() {
            let config = ReconnectConfig {
                enabled: true,
                max_attempts: 3,
                ..ReconnectConfig::default()
            };
            let mgr = ReconnectManager::new(config);
            let addr = "127.0.0.1:20880";

            mgr.record_failure(addr);
            mgr.record_failure(addr);
            mgr.record_failure(addr);
            assert!(!mgr.should_reconnect(addr));

            mgr.record_success(addr);
            assert!(mgr.should_reconnect(addr));
            assert_eq!(mgr.next_delay(addr), Duration::from_secs(1));
        }

        #[test]
        fn test_reconnect_manager_disabled() {
            let config = ReconnectConfig {
                enabled: false,
                ..ReconnectConfig::default()
            };
            let mgr = ReconnectManager::new(config);
            assert!(!mgr.should_reconnect("127.0.0.1:20880"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_creation() {
        let req = Request::new(42, true, b"hello".to_vec());
        assert_eq!(req.id, 42);
        assert!(req.is_twoway);
        assert_eq!(req.data, b"hello");
    }

    #[test]
    fn test_request_oneway() {
        let req = Request::new(1, false, vec![]);
        assert!(!req.is_twoway);
    }

    #[test]
    fn test_request_event() {
        let mut req = Request::new(1, false, vec![]);
        req.is_event = true;
        assert!(req.is_event);
        assert!(!req.is_twoway);
    }

    #[test]
    fn test_response_success() {
        let resp = Response::success(42, b"world".to_vec());
        assert_eq!(resp.id, 42);
        assert_eq!(resp.status, dubbo_rs_common::constants::OK_STATUS);
        assert_eq!(resp.data, b"world");
    }

    #[test]
    fn test_response_error() {
        let resp = Response::error(
            42,
            dubbo_rs_common::constants::SERVICE_NOT_FOUND_STATUS,
            b"not found".to_vec(),
        );
        assert_eq!(resp.id, 42);
        assert_eq!(
            resp.status,
            dubbo_rs_common::constants::SERVICE_NOT_FOUND_STATUS
        );
        assert!(resp.is_error());
    }

    #[test]
    fn test_response_is_ok() {
        let ok_resp = Response::success(1, vec![]);
        assert!(!ok_resp.is_error());

        let err_resp = Response::error(1, dubbo_rs_common::constants::SERVER_ERROR_STATUS, vec![]);
        assert!(err_resp.is_error());
    }

    #[test]
    fn test_request_new_is_not_event() {
        let req = Request::new(99, true, b"payload".to_vec());
        assert!(
            !req.is_event,
            "new requests should not be events by default"
        );
    }

    #[test]
    fn test_request_is_twoway_flag() {
        let twoway = Request::new(1, true, vec![]);
        assert!(twoway.is_twoway);

        let oneway = Request::new(2, false, vec![]);
        assert!(!oneway.is_twoway);
    }

    #[test]
    fn test_response_success_helper() {
        let resp = Response::success(42, b"ok-data".to_vec());
        assert_eq!(resp.id, 42);
        assert_eq!(resp.status, dubbo_rs_common::constants::OK_STATUS);
        assert_eq!(resp.data, b"ok-data");
        assert!(!resp.is_error());
    }

    #[test]
    fn test_response_error_helper() {
        let resp = Response::error(
            7,
            dubbo_rs_common::constants::SERVER_ERROR_STATUS,
            b"boom".to_vec(),
        );
        assert_eq!(resp.id, 7);
        assert_eq!(resp.status, dubbo_rs_common::constants::SERVER_ERROR_STATUS);
        assert!(resp.is_error());
    }

    #[test]
    fn test_response_debug() {
        let resp = Response::new(1, 20, b"abc".to_vec());
        let debug_str = format!("{resp:?}");
        assert!(debug_str.contains("Response"));
        assert!(debug_str.contains('1'));
    }

    #[test]
    fn test_request_debug() {
        let req = Request::new(2, true, b"xyz".to_vec());
        let debug_str = format!("{req:?}");
        assert!(debug_str.contains("Request"));
        assert!(debug_str.contains('2'));
    }
}

#[cfg(test)]
mod exchange_tests {
    use super::*;

    struct TestExchangeClient;

    #[async_trait]
    impl ExchangeClient for TestExchangeClient {
        async fn connect(&mut self, _url: &URL) -> Result<()> {
            Ok(())
        }

        async fn request(&self, _req: Request) -> Result<Response> {
            Ok(Response::success(1, b"test".to_vec()))
        }

        fn close(&self) {}
    }

    #[tokio::test]
    async fn test_exchange_client_trait() {
        let mut client = TestExchangeClient;
        let url = URL::new("tri", "/test");
        client.connect(&url).await.expect("connect ok");
        let resp = client
            .request(Request::new(1, true, vec![]))
            .await
            .expect("request ok");
        assert!(!resp.is_error());
    }
}

#[cfg(test)]
mod codec_tests {
    use anyhow::Result;
    use dubbo_rs_common::constants::OK_STATUS;

    use super::{Codec, Request, Response};

    struct TestCodec;

    impl Codec for TestCodec {
        fn encode_request(&self, req: &Request) -> Result<Vec<u8>> {
            Ok(format!("{}:{}:{:?}", req.id, req.is_twoway, req.data).into_bytes())
        }

        fn decode_request(&self, data: &[u8]) -> Result<Request> {
            let s = std::str::from_utf8(data)?;
            let mut parts = s.splitn(3, ':');
            let id: u64 = parts.next().unwrap().parse()?;
            let is_twoway: bool = parts.next().unwrap().parse()?;
            let data_str = parts.next().unwrap();
            let inner = &data_str[1..data_str.len() - 1];
            let data: Vec<u8> = if inner.is_empty() {
                vec![]
            } else {
                inner
                    .split(", ")
                    .map(|b| b.parse::<u8>().map_err(Into::into))
                    .collect::<Result<Vec<u8>>>()?
            };
            Ok(Request {
                id,
                is_twoway,
                is_event: false,
                data,
            })
        }

        fn encode_response(&self, resp: &Response) -> Result<Vec<u8>> {
            Ok(format!("{}:{}:{:?}", resp.id, resp.status, resp.data).into_bytes())
        }

        fn decode_response(&self, data: &[u8]) -> Result<Response> {
            let s = std::str::from_utf8(data)?;
            let mut parts = s.splitn(3, ':');
            let id: u64 = parts.next().unwrap().parse()?;
            let status: u8 = parts.next().unwrap().parse()?;
            let data_str = parts.next().unwrap();
            let inner = &data_str[1..data_str.len() - 1];
            let resp_data: Vec<u8> = if inner.is_empty() {
                vec![]
            } else {
                inner
                    .split(", ")
                    .map(|b| b.parse::<u8>().map_err(Into::into))
                    .collect::<Result<Vec<u8>>>()?
            };
            Ok(Response {
                id,
                status,
                data: resp_data,
            })
        }
    }

    #[test]
    fn test_codec_encode_decode_request_roundtrip() {
        let codec = TestCodec;
        let original = Request::new(123, true, b"hello".to_vec());
        let encoded = codec.encode_request(&original).expect("encode ok");
        let decoded = codec.decode_request(&encoded).expect("decode ok");
        assert_eq!(decoded.id, original.id);
        assert_eq!(decoded.is_twoway, original.is_twoway);
        assert!(!decoded.is_event);
        assert_eq!(decoded.data, original.data);
    }

    #[test]
    fn test_codec_encode_decode_response_roundtrip() {
        let codec = TestCodec;
        let original = Response::success(456, b"world".to_vec());
        assert_eq!(original.status, OK_STATUS);
        let encoded = codec.encode_response(&original).expect("encode ok");
        let decoded = codec.decode_response(&encoded).expect("decode ok");
        assert_eq!(decoded.id, original.id);
        assert_eq!(decoded.status, original.status);
        assert_eq!(decoded.data, original.data);
    }

    #[test]
    fn test_codec_roundtrip_empty_data() {
        let codec = TestCodec;
        let req = Request::new(0, false, vec![]);
        let encoded = codec.encode_request(&req).expect("encode ok");
        let decoded = codec.decode_request(&encoded).expect("decode ok");
        assert!(decoded.data.is_empty());
        assert_eq!(decoded.id, 0);
        assert!(!decoded.is_twoway);
    }
}
