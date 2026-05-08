#![allow(clippy::borrowed_box)]

pub use dubbo_rs_common;
pub use dubbo_rs_protocol;

use std::cmp::Ordering as CmpOrdering;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;

use dubbo_rs_common::error::RPCError;
use dubbo_rs_common::url::URL;
use dubbo_rs_protocol::{InvocationContext, Invoker};
use rand::Rng;

pub trait LoadBalance: Send + Sync {
    /// Select an invoker index using a load-balancing strategy.
    ///
    /// # Errors
    ///
    /// Returns `RPCError::ServiceNotFound` if the invoker list is empty.
    fn select(
        &self,
        invokers: &[Box<dyn Invoker>],
        url: &URL,
        invocation: &InvocationContext,
    ) -> Result<usize, RPCError>;
}

fn get_weight(invoker: &Box<dyn Invoker>) -> u64 {
    invoker
        .get_url()
        .get_param("weight")
        .and_then(|w| w.parse().ok())
        .unwrap_or(100)
}

fn get_warmup(invoker: &Box<dyn Invoker>) -> u64 {
    invoker
        .get_url()
        .get_param("warmup")
        .and_then(|w| w.parse().ok())
        .unwrap_or(600_000)
}

/// Calculate effective weight considering warmup time.
///
/// If the invoker's `timestamp` parameter indicates it was started
/// within the warmup period, the weight is proportionally scaled.
fn calculate_warmup_weight(invoker: &Box<dyn Invoker>, weight: u64) -> u64 {
    let Some(ts_str) = invoker.get_url().get_param("timestamp") else {
        return weight;
    };
    let Ok(ts) = ts_str.parse::<u64>() else {
        return weight;
    };
    let warmup = get_warmup(invoker);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let now = u64::try_from(now).unwrap_or(u64::MAX);
    let uptime = now.saturating_sub(ts);
    if uptime > 0 && uptime < warmup {
        (weight * uptime) / warmup
    } else {
        weight
    }
}

fn effective_weight(invoker: &Box<dyn Invoker>) -> u64 {
    let raw_weight = get_weight(invoker);
    calculate_warmup_weight(invoker, raw_weight)
}

pub struct RandomLoadBalance;

impl LoadBalance for RandomLoadBalance {
    fn select(
        &self,
        invokers: &[Box<dyn Invoker>],
        _url: &URL,
        _invocation: &InvocationContext,
    ) -> Result<usize, RPCError> {
        if invokers.is_empty() {
            return Err(RPCError::ServiceNotFound(
                "no invokers for random selection".into(),
            ));
        }

        if invokers.len() == 1 {
            return Ok(0);
        }

        let total_weight: u64 = invokers.iter().map(effective_weight).sum();

        if total_weight == 0 {
            let idx = rand::thread_rng().gen_range(0..invokers.len());
            return Ok(idx);
        }

        let mut offset = rand::thread_rng().gen_range(0..total_weight);
        for (i, inv) in invokers.iter().enumerate() {
            let w = effective_weight(inv);
            if offset < w {
                return Ok(i);
            }
            offset -= w;
        }

        Ok(invokers.len() - 1)
    }
}

pub struct RoundRobinLoadBalance {
    sequences: Mutex<std::collections::HashMap<String, AtomicUsize>>,
    weight_sequences: Mutex<std::collections::HashMap<String, AtomicUsize>>,
}

impl RoundRobinLoadBalance {
    #[must_use]
    pub fn new() -> Self {
        Self {
            sequences: Mutex::new(std::collections::HashMap::new()),
            weight_sequences: Mutex::new(std::collections::HashMap::new()),
        }
    }

    fn get_sequence_key(url: &URL) -> String {
        format!("{}:{}", url.path, url.get_version())
    }
}

impl Default for RoundRobinLoadBalance {
    fn default() -> Self {
        Self::new()
    }
}

impl LoadBalance for RoundRobinLoadBalance {
    fn select(
        &self,
        invokers: &[Box<dyn Invoker>],
        url: &URL,
        _invocation: &InvocationContext,
    ) -> Result<usize, RPCError> {
        if invokers.is_empty() {
            return Err(RPCError::ServiceNotFound(
                "no invokers for round-robin selection".into(),
            ));
        }

        let key = Self::get_sequence_key(url);
        let weights: Vec<u64> = invokers.iter().map(effective_weight).collect();
        let total_weight: u64 = weights.iter().sum();

        if total_weight == 0 {
            let mut seq_map = self.sequences.lock().unwrap();
            let counter = seq_map.entry(key).or_insert_with(|| AtomicUsize::new(0));
            let idx = counter.fetch_add(1, Ordering::SeqCst) % invokers.len();
            return Ok(idx);
        }

        let max_weight = *weights.iter().max().unwrap_or(&1);
        let mut wseq_map = self.weight_sequences.lock().unwrap();
        let counter = wseq_map.entry(key).or_insert_with(|| AtomicUsize::new(0));

        let modulo = invokers.len();
        loop {
            let current = counter.fetch_add(1, Ordering::SeqCst) % modulo;
            let gcd_weight = gcd(weights[current], max_weight);
            let current_weight = weights[current] / (max_weight / gcd_weight);
            if current_weight > 0 {
                return Ok(current);
            }
        }
    }
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

pub struct LeastActiveLoadBalance;

impl LoadBalance for LeastActiveLoadBalance {
    fn select(
        &self,
        invokers: &[Box<dyn Invoker>],
        _url: &URL,
        _invocation: &InvocationContext,
    ) -> Result<usize, RPCError> {
        if invokers.is_empty() {
            return Err(RPCError::ServiceNotFound(
                "no invokers for least-active selection".into(),
            ));
        }

        let mut best_idx = 0;
        let mut least_active = u64::MAX;
        let mut best_weight = 0u64;

        for (i, invoker) in invokers.iter().enumerate() {
            let active: u64 = invoker
                .get_url()
                .get_param("active")
                .and_then(|a| a.parse().ok())
                .unwrap_or(0);

            let weight = effective_weight(invoker);

            if active < least_active || (active == least_active && weight > best_weight) {
                least_active = active;
                best_idx = i;
                best_weight = weight;
            }
        }

        Ok(best_idx)
    }
}

pub struct ConsistentHashLoadBalance {
    virtual_nodes: u32,
}

impl ConsistentHashLoadBalance {
    #[must_use]
    pub fn new() -> Self {
        Self { virtual_nodes: 160 }
    }

    #[must_use]
    pub fn with_virtual_nodes(mut self, n: u32) -> Self {
        self.virtual_nodes = n;
        self
    }
}

impl Default for ConsistentHashLoadBalance {
    fn default() -> Self {
        Self::new()
    }
}

impl LoadBalance for ConsistentHashLoadBalance {
    fn select(
        &self,
        invokers: &[Box<dyn Invoker>],
        _url: &URL,
        invocation: &InvocationContext,
    ) -> Result<usize, RPCError> {
        if invokers.is_empty() {
            return Err(RPCError::ServiceNotFound(
                "no invokers for consistent-hash selection".into(),
            ));
        }

        if invokers.len() == 1 {
            return Ok(0);
        }

        let hash_key = invocation
            .arguments
            .first()
            .map_or_else(|| invocation.method_name.as_bytes(), Vec::as_slice);

        let mut hasher = DefaultHasher::new();
        hash_key.hash(&mut hasher);
        let hash_value = hasher.finish();

        #[allow(clippy::cast_possible_truncation)]
        let idx = (hash_value % invokers.len() as u64) as usize;
        Ok(idx)
    }
}

pub struct ShortestResponseLoadBalance;

impl LoadBalance for ShortestResponseLoadBalance {
    fn select(
        &self,
        invokers: &[Box<dyn Invoker>],
        _url: &URL,
        _invocation: &InvocationContext,
    ) -> Result<usize, RPCError> {
        if invokers.is_empty() {
            return Err(RPCError::ServiceNotFound(
                "no invokers for shortest-response selection".into(),
            ));
        }

        if invokers.len() == 1 {
            return Ok(0);
        }

        let rt_data: Vec<Option<(u64, u64)>> = invokers
            .iter()
            .map(|invoker| {
                let rt: Option<u64> = invoker
                    .get_url()
                    .get_param("rt")
                    .and_then(|v| v.parse().ok());
                let rt_count: Option<u64> = invoker
                    .get_url()
                    .get_param("rt_count")
                    .and_then(|v| v.parse().ok());
                match (rt, rt_count) {
                    (Some(rt), Some(count)) => Some((rt, count)),
                    _ => None,
                }
            })
            .collect();

        let has_any_rt = rt_data.iter().any(Option::is_some);

        if !has_any_rt {
            let total_weight: u64 = invokers.iter().map(effective_weight).sum();

            if total_weight == 0 {
                let idx = rand::thread_rng().gen_range(0..invokers.len());
                return Ok(idx);
            }

            let mut offset = rand::thread_rng().gen_range(0..total_weight);
            for (i, inv) in invokers.iter().enumerate() {
                let w = effective_weight(inv);
                if offset < w {
                    return Ok(i);
                }
                offset -= w;
            }

            return Ok(invokers.len() - 1);
        }

        let min_rt = rt_data
            .iter()
            .filter_map(|&opt| opt.map(|(rt, _)| rt))
            .min()
            .unwrap_or(0);

        let best_indices: Vec<usize> = rt_data
            .iter()
            .enumerate()
            .filter(|(_, opt)| opt.is_some_and(|(rt, _)| rt == min_rt))
            .map(|(i, _)| i)
            .collect();

        if best_indices.len() == 1 {
            return Ok(best_indices[0]);
        }

        let mut best_weight = effective_weight(&invokers[best_indices[0]]);

        for &idx in &best_indices[1..] {
            let w = effective_weight(&invokers[idx]);
            if w > best_weight {
                best_weight = w;
            }
        }

        let tied_indices: Vec<usize> = best_indices
            .into_iter()
            .filter(|&idx| effective_weight(&invokers[idx]) == best_weight)
            .collect();

        if tied_indices.len() == 1 {
            return Ok(tied_indices[0]);
        }

        let rand_idx = rand::thread_rng().gen_range(0..tied_indices.len());
        Ok(tied_indices[rand_idx])
    }
}

/// Per-invoker statistics tracked by [`P2CLoadBalance`].
struct InvokerStats {
    active_requests: AtomicU64,
    total_requests: AtomicU64,
    total_errors: AtomicU64,
    ewma_latency_ms: AtomicU64,
}

impl InvokerStats {
    fn new() -> Self {
        Self {
            active_requests: AtomicU64::new(0),
            total_requests: AtomicU64::new(0),
            total_errors: AtomicU64::new(0),
            ewma_latency_ms: AtomicU64::new(0),
        }
    }
}

/// Power of Two Choices (P2C) load balancing with EWMA latency estimation.
///
/// Call [`P2CLoadBalance::record_result`] after each invocation to update stats.
pub struct P2CLoadBalance {
    stats: dashmap::DashMap<String, InvokerStats>,
}

const EWMA_FIXED_POINT: u64 = 1000;
const EWMA_ALPHA_FP: u64 = 300;
const LOAD_FACTOR: u64 = 100;
const LATENCY_FACTOR: u64 = 1;

impl P2CLoadBalance {
    #[must_use]
    pub fn new() -> Self {
        Self {
            stats: dashmap::DashMap::new(),
        }
    }

    fn invoker_key(invoker: &Box<dyn Invoker>) -> String {
        let url = invoker.get_url();
        format!("{}:{}", url.ip, url.port)
    }

    fn get_or_create_stats(
        &self,
        key: &str,
    ) -> dashmap::mapref::one::Ref<'_, String, InvokerStats> {
        self.stats
            .entry(key.to_string())
            .or_insert_with(InvokerStats::new)
            .downgrade()
    }

    /// `score = LOAD_FACTOR * active / weight + LATENCY_FACTOR * ewma / EWMA_FIXED_POINT`
    fn compute_score(&self, invoker: &Box<dyn Invoker>) -> u64 {
        let key = Self::invoker_key(invoker);
        let stats = self.get_or_create_stats(&key);
        let active = stats.active_requests.load(Ordering::Relaxed);
        let ewma_fp = stats.ewma_latency_ms.load(Ordering::Relaxed);

        let weight = effective_weight(invoker).max(1);

        let load_component = LOAD_FACTOR * active / weight;
        let latency_component = LATENCY_FACTOR * ewma_fp / EWMA_FIXED_POINT;

        load_component + latency_component
    }

    /// Update stats after invocation: decrement active, update EWMA latency, track errors.
    pub fn record_result(&self, invoker_key: &str, latency_ms: u64, success: bool) {
        let stats = self.get_or_create_stats(invoker_key);

        stats.active_requests.fetch_sub(1, Ordering::Relaxed);
        stats.total_requests.fetch_add(1, Ordering::Relaxed);

        // EWMA update in fixed-point: new = alpha*latency + (1-alpha)*old
        let latency_fp = latency_ms * EWMA_FIXED_POINT;
        let old_fp = stats.ewma_latency_ms.load(Ordering::Relaxed);
        let new_fp = (EWMA_ALPHA_FP * latency_fp + (EWMA_FIXED_POINT - EWMA_ALPHA_FP) * old_fp)
            / EWMA_FIXED_POINT;
        stats.ewma_latency_ms.store(new_fp, Ordering::Relaxed);

        if !success {
            stats.total_errors.fetch_add(1, Ordering::Relaxed);
        }
    }
}

impl Default for P2CLoadBalance {
    fn default() -> Self {
        Self::new()
    }
}

impl LoadBalance for P2CLoadBalance {
    fn select(
        &self,
        invokers: &[Box<dyn Invoker>],
        _url: &URL,
        _invocation: &InvocationContext,
    ) -> Result<usize, RPCError> {
        if invokers.is_empty() {
            return Err(RPCError::ServiceNotFound(
                "no invokers for p2c selection".into(),
            ));
        }

        if invokers.len() == 1 {
            let key = Self::invoker_key(&invokers[0]);
            let stats = self.get_or_create_stats(&key);
            stats.active_requests.fetch_add(1, Ordering::Relaxed);
            return Ok(0);
        }

        let mut rng = rand::thread_rng();
        let i = rng.gen_range(0..invokers.len());
        let mut j = rng.gen_range(0..invokers.len() - 1);
        if j >= i {
            j += 1;
        }

        let score_i = self.compute_score(&invokers[i]);
        let score_j = self.compute_score(&invokers[j]);

        let chosen = match score_i.cmp(&score_j) {
            CmpOrdering::Less => i,
            CmpOrdering::Greater => j,
            CmpOrdering::Equal => {
                let weight_i = effective_weight(&invokers[i]);
                let weight_j = effective_weight(&invokers[j]);
                if weight_i >= weight_j {
                    i
                } else {
                    j
                }
            }
        };

        let key = Self::invoker_key(&invokers[chosen]);
        let stats = self.get_or_create_stats(&key);
        stats.active_requests.fetch_add(1, Ordering::Relaxed);

        Ok(chosen)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use dubbo_rs_common::node::Node;

    struct TestInvoker {
        url: URL,
    }

    impl TestInvoker {
        fn new(host: &str, weight: u64) -> Self {
            let mut url = URL::new("tri", "/com.example.Service");
            url.ip = host.to_string();
            url.set_param("weight", weight.to_string());
            Self { url }
        }

        fn with_active(host: &str, weight: u64, active: u64) -> Self {
            let mut url = URL::new("tri", "/com.example.Service");
            url.ip = host.to_string();
            url.set_param("weight", weight.to_string());
            url.set_param("active", active.to_string());
            Self { url }
        }

        fn with_rt(host: &str, weight: u64, rt: u64, rt_count: u64) -> Self {
            let mut url = URL::new("tri", "/com.example.Service");
            url.ip = host.to_string();
            url.set_param("weight", weight.to_string());
            url.set_param("rt", rt.to_string());
            url.set_param("rt_count", rt_count.to_string());
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
        async fn invoke(
            &self,
            _ctx: &mut InvocationContext,
        ) -> Result<dubbo_rs_protocol::RPCResult, anyhow::Error> {
            Ok(dubbo_rs_protocol::RPCResult::success(b"test".to_vec()))
        }
    }

    fn make_invokers(count: usize) -> Vec<Box<dyn Invoker>> {
        (0..count)
            .map(|i| {
                Box::new(TestInvoker::new(&format!("192.168.1.{}", i + 1), 100)) as Box<dyn Invoker>
            })
            .collect()
    }

    fn make_invokers_weighted(weights: &[u64]) -> Vec<Box<dyn Invoker>> {
        weights
            .iter()
            .enumerate()
            .map(|(i, &w)| {
                Box::new(TestInvoker::new(&format!("192.168.1.{}", i + 1), w)) as Box<dyn Invoker>
            })
            .collect()
    }

    fn dummy_ctx() -> InvocationContext {
        let mut url = URL::new("tri", "/com.example.Service");
        url.ip = "127.0.0.1".into();
        url.port = "50051".into();
        InvocationContext::new("sayHello", url)
    }

    #[test]
    fn test_random_empty_invokers() {
        let lb = RandomLoadBalance;
        let result = lb.select(&[], &URL::default(), &dummy_ctx());
        assert!(result.is_err());
    }

    #[test]
    fn test_random_single_invoker() {
        let lb = RandomLoadBalance;
        let invokers = make_invokers(1);
        let idx = lb.select(&invokers, &URL::default(), &dummy_ctx()).unwrap();
        assert_eq!(idx, 0);
    }

    #[test]
    fn test_random_distribution() {
        let lb = RandomLoadBalance;
        let invokers = make_invokers(3);
        let mut counts = [0usize; 3];

        for _ in 0..300 {
            let idx = lb.select(&invokers, &URL::default(), &dummy_ctx()).unwrap();
            counts[idx] += 1;
        }

        for count in &counts {
            assert!(*count > 50, "each invoker should get at least 50/300 calls");
        }
    }

    #[test]
    fn test_random_weighted_preference() {
        let lb = RandomLoadBalance;
        let invokers = make_invokers_weighted(&[100, 900]);
        let mut counts = [0usize; 2];

        for _ in 0..1000 {
            let idx = lb.select(&invokers, &URL::default(), &dummy_ctx()).unwrap();
            counts[idx] += 1;
        }

        assert!(
            counts[1] > counts[0] * 3,
            "heavier invoker should get significantly more calls"
        );
    }

    #[test]
    fn test_round_robin_empty() {
        let lb = RoundRobinLoadBalance::default();
        let result = lb.select(&[], &URL::default(), &dummy_ctx());
        assert!(result.is_err());
    }

    #[test]
    fn test_round_robin_sequence() {
        let lb = RoundRobinLoadBalance::default();
        let invokers = make_invokers(3);
        let url = URL::new("tri", "/com.example.Service");
        let ctx = dummy_ctx();

        let idx0 = lb.select(&invokers, &url, &ctx).unwrap();
        let idx1 = lb.select(&invokers, &url, &ctx).unwrap();
        let idx2 = lb.select(&invokers, &url, &ctx).unwrap();
        let idx3 = lb.select(&invokers, &url, &ctx).unwrap();

        assert_eq!(idx0, 0);
        assert_eq!(idx1, 1);
        assert_eq!(idx2, 2);
        assert_eq!(idx3, 0);
    }

    #[test]
    fn test_least_active_empty() {
        let lb = LeastActiveLoadBalance;
        let result = lb.select(&[], &URL::default(), &dummy_ctx());
        assert!(result.is_err());
    }

    #[test]
    fn test_least_active_prefers_lower() {
        let lb = LeastActiveLoadBalance;
        let invokers: Vec<Box<dyn Invoker>> = vec![
            Box::new(TestInvoker::with_active("192.168.1.1", 100, 10)),
            Box::new(TestInvoker::with_active("192.168.1.2", 100, 2)),
            Box::new(TestInvoker::with_active("192.168.1.3", 100, 5)),
        ];

        let idx = lb.select(&invokers, &URL::default(), &dummy_ctx()).unwrap();
        assert_eq!(idx, 1);
    }

    #[test]
    fn test_consistent_hash_empty() {
        let lb = ConsistentHashLoadBalance::default();
        let result = lb.select(&[], &URL::default(), &dummy_ctx());
        assert!(result.is_err());
    }

    #[test]
    fn test_consistent_hash_same_input() {
        let lb = ConsistentHashLoadBalance::default();
        let invokers = make_invokers(5);

        let mut ctx = dummy_ctx();
        ctx.arguments = vec![b"user-123".to_vec()];

        let idx1 = lb.select(&invokers, &URL::default(), &ctx).unwrap();
        let idx2 = lb.select(&invokers, &URL::default(), &ctx).unwrap();

        assert_eq!(idx1, idx2, "same input should always hash to same invoker");
    }

    #[test]
    fn test_get_weight_default() {
        let invoker: Box<dyn Invoker> = Box::new(TestInvoker::new("192.168.1.1", 0));
        assert_eq!(get_weight(&invoker), 0);
    }

    #[test]
    fn test_gcd() {
        assert_eq!(gcd(12, 8), 4);
        assert_eq!(gcd(7, 13), 1);
        assert_eq!(gcd(0, 5), 5);
    }

    #[test]
    fn test_calculate_warmup_weight_no_timestamp() {
        let invoker: Box<dyn Invoker> = Box::new(TestInvoker::new("192.168.1.1", 100));
        assert_eq!(calculate_warmup_weight(&invoker, 100), 100);
        assert_eq!(calculate_warmup_weight(&invoker, 0), 0);
    }

    #[test]
    fn test_calculate_warmup_weight_in_warmup() {
        // warmup formula: weight * uptime / warmup ≈ 100 * 500 / 1000 = 50
        let mut inv = TestInvoker::new("192.168.1.1", 100);
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let now_u64 = u64::try_from(now_ms).unwrap();
        inv.url.set_param("timestamp", (now_u64 - 500).to_string());
        inv.url.set_param("warmup", "1000");

        let invoker: Box<dyn Invoker> = Box::new(inv);
        let result = calculate_warmup_weight(&invoker, 100);
        assert!(
            (40..=60).contains(&result),
            "expected ~50 during warmup, got {result}"
        );
    }

    #[test]
    fn test_calculate_warmup_weight_after_warmup() {
        let mut inv = TestInvoker::new("192.168.1.1", 100);
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let now_u64 = u64::try_from(now_ms).unwrap();
        inv.url.set_param("timestamp", (now_u64 - 2000).to_string());
        inv.url.set_param("warmup", "1000");

        let invoker: Box<dyn Invoker> = Box::new(inv);
        assert_eq!(calculate_warmup_weight(&invoker, 100), 100);
    }

    #[test]
    fn test_effective_weight() {
        let invoker: Box<dyn Invoker> = Box::new(TestInvoker::new("192.168.1.1", 200));
        assert_eq!(effective_weight(&invoker), 200);

        let invoker_low: Box<dyn Invoker> = Box::new(TestInvoker::new("192.168.1.2", 50));
        assert_eq!(effective_weight(&invoker_low), 50);
    }

    #[test]
    fn test_get_weight_default_no_param() {
        let mut url = URL::new("tri", "/com.example.Service");
        url.ip = "192.168.1.1".to_string();
        let invoker: Box<dyn Invoker> = Box::new(TestInvoker { url });
        assert_eq!(get_weight(&invoker), 100);
    }

    #[test]
    fn test_get_weight_custom() {
        let invoker: Box<dyn Invoker> = Box::new(TestInvoker::new("192.168.1.1", 200));
        assert_eq!(get_weight(&invoker), 200);
    }

    #[test]
    fn test_get_warmup_default() {
        let invoker: Box<dyn Invoker> = Box::new(TestInvoker::new("192.168.1.1", 100));
        assert_eq!(get_warmup(&invoker), 600_000);
    }

    #[test]
    fn test_get_warmup_custom() {
        let mut inv = TestInvoker::new("192.168.1.1", 100);
        inv.url.set_param("warmup", "300000");
        let invoker: Box<dyn Invoker> = Box::new(inv);
        assert_eq!(get_warmup(&invoker), 300_000);
    }

    #[test]
    fn test_consistent_hash_default_virtual_nodes() {
        let lb = ConsistentHashLoadBalance::new();
        let invokers = make_invokers(5);
        let mut ctx = dummy_ctx();
        ctx.arguments = vec![b"hash-key".to_vec()];
        let idx = lb.select(&invokers, &URL::default(), &ctx).unwrap();
        assert!(idx < 5, "index {idx} out of range");
    }

    #[test]
    fn test_consistent_hash_custom_virtual_nodes() {
        let lb = ConsistentHashLoadBalance::new().with_virtual_nodes(320);
        let invokers = make_invokers(5);
        let mut ctx = dummy_ctx();
        ctx.arguments = vec![b"hash-key".to_vec()];
        let idx = lb.select(&invokers, &URL::default(), &ctx).unwrap();
        assert!(idx < 5, "index {idx} out of range");
        let idx2 = lb.select(&invokers, &URL::default(), &ctx).unwrap();
        assert_eq!(idx, idx2);
    }

    #[test]
    fn test_round_robin_weighted_distribution() {
        let lb = RoundRobinLoadBalance::new();
        let invokers = make_invokers_weighted(&[100, 0, 200]);
        let url = URL::new("tri", "/com.example.Service");
        let ctx = dummy_ctx();

        let mut counts = [0usize; 3];
        for _ in 0..60 {
            let idx = lb.select(&invokers, &url, &ctx).unwrap();
            counts[idx] += 1;
        }

        assert_eq!(counts[1], 0, "zero-weight invoker should never be selected");
        assert!(counts[0] > 0);
        assert!(counts[2] > 0);
    }

    #[test]
    fn test_random_all_zero_weights() {
        let lb = RandomLoadBalance;
        let invokers = make_invokers_weighted(&[0, 0, 0]);

        let mut seen = [false; 3];
        for _ in 0..300 {
            let idx = lb.select(&invokers, &URL::default(), &dummy_ctx()).unwrap();
            assert!(idx < 3);
            seen[idx] = true;
        }
        for (i, &s) in seen.iter().enumerate() {
            assert!(s, "index {i} was never selected");
        }
    }

    #[test]
    fn test_gcd_zero() {
        assert_eq!(gcd(0, 0), 0);
        assert_eq!(gcd(5, 0), 5);
    }

    #[test]
    fn test_shortest_response_empty_invokers() {
        let lb = ShortestResponseLoadBalance;
        let result = lb.select(&[], &URL::default(), &dummy_ctx());
        assert!(result.is_err());
    }

    #[test]
    fn test_shortest_response_single_invoker() {
        let lb = ShortestResponseLoadBalance;
        let invokers: Vec<Box<dyn Invoker>> =
            vec![Box::new(TestInvoker::with_rt("192.168.1.1", 100, 50, 100))];
        let idx = lb.select(&invokers, &URL::default(), &dummy_ctx()).unwrap();
        assert_eq!(idx, 0);
    }

    #[test]
    fn test_shortest_response_prefers_lowest_rt() {
        let lb = ShortestResponseLoadBalance;
        let invokers: Vec<Box<dyn Invoker>> = vec![
            Box::new(TestInvoker::with_rt("192.168.1.1", 100, 200, 50)),
            Box::new(TestInvoker::with_rt("192.168.1.2", 100, 30, 50)),
            Box::new(TestInvoker::with_rt("192.168.1.3", 100, 150, 50)),
        ];
        let idx = lb.select(&invokers, &URL::default(), &dummy_ctx()).unwrap();
        assert_eq!(idx, 1);
    }

    #[test]
    fn test_shortest_response_fallback_to_random_without_rt() {
        let lb = ShortestResponseLoadBalance;
        let invokers = make_invokers(3);

        let mut counts = [0usize; 3];
        for _ in 0..300 {
            let idx = lb.select(&invokers, &URL::default(), &dummy_ctx()).unwrap();
            counts[idx] += 1;
        }

        for count in &counts {
            assert!(
                *count > 50,
                "each invoker should get at least 50/300 calls in fallback mode"
            );
        }
    }

    #[test]
    fn test_shortest_response_breaks_ties_by_weight() {
        let lb = ShortestResponseLoadBalance;
        let invokers: Vec<Box<dyn Invoker>> = vec![
            Box::new(TestInvoker::with_rt("192.168.1.1", 50, 100, 50)),
            Box::new(TestInvoker::with_rt("192.168.1.2", 200, 100, 50)),
            Box::new(TestInvoker::with_rt("192.168.1.3", 100, 200, 50)),
        ];
        let idx = lb.select(&invokers, &URL::default(), &dummy_ctx()).unwrap();
        assert_eq!(
            idx, 1,
            "should select invoker with higher weight among tied RT"
        );
    }

    #[test]
    fn test_shortest_response_mixed_rt_data() {
        let lb = ShortestResponseLoadBalance;
        let invokers: Vec<Box<dyn Invoker>> = vec![
            Box::new(TestInvoker::with_rt("192.168.1.1", 100, 80, 50)),
            Box::new(TestInvoker::new("192.168.1.2", 100)),
            Box::new(TestInvoker::with_rt("192.168.1.3", 100, 120, 50)),
        ];
        let idx = lb.select(&invokers, &URL::default(), &dummy_ctx()).unwrap();
        assert_eq!(
            idx, 0,
            "should select invoker with lowest RT among those with RT data"
        );
    }

    #[test]
    fn test_p2c_empty_invokers() {
        let lb = P2CLoadBalance::new();
        let result = lb.select(&[], &URL::default(), &dummy_ctx());
        assert!(result.is_err());
    }

    #[test]
    fn test_p2c_single_invoker() {
        let lb = P2CLoadBalance::new();
        let invokers = make_invokers(1);
        for _ in 0..10 {
            let idx = lb.select(&invokers, &URL::default(), &dummy_ctx()).unwrap();
            assert_eq!(idx, 0);
        }
    }

    #[test]
    fn test_p2c_distribution_over_1000_calls() {
        let lb = P2CLoadBalance::new();
        let invokers = make_invokers(3);
        let mut counts = [0usize; 3];

        for _ in 0..1000 {
            let idx = lb.select(&invokers, &URL::default(), &dummy_ctx()).unwrap();
            counts[idx] += 1;
            let key = P2CLoadBalance::invoker_key(&invokers[idx]);
            lb.record_result(&key, 10, true);
        }

        for (i, count) in counts.iter().enumerate() {
            assert!(*count > 50, "invoker {i} got {count} calls, expected > 50");
        }
    }

    #[test]
    fn test_p2c_prefers_lighter_load() {
        let lb = P2CLoadBalance::new();

        let invokers: Vec<Box<dyn Invoker>> = vec![
            Box::new(TestInvoker::new("192.168.1.1", 100)),
            Box::new(TestInvoker::new("192.168.1.2", 100)),
        ];

        let key_heavy = "192.168.1.1:".to_string();
        let key_light = "192.168.1.2:".to_string();

        let stats_heavy = lb.get_or_create_stats(&key_heavy);
        stats_heavy.active_requests.store(100, Ordering::Relaxed);
        stats_heavy
            .ewma_latency_ms
            .store(500_000, Ordering::Relaxed);
        drop(stats_heavy);

        let stats_light = lb.get_or_create_stats(&key_light);
        stats_light.active_requests.store(1, Ordering::Relaxed);
        stats_light.ewma_latency_ms.store(10_000, Ordering::Relaxed);
        drop(stats_light);

        let mut light_count = 0usize;
        for _ in 0..100 {
            let idx = lb.select(&invokers, &URL::default(), &dummy_ctx()).unwrap();
            if idx == 1 {
                light_count += 1;
            }
            let key = P2CLoadBalance::invoker_key(&invokers[idx]);
            lb.record_result(&key, 10, true);
        }

        assert!(
            light_count > 80,
            "lighter invoker should get most traffic, got {light_count}/100"
        );
    }

    #[test]
    fn test_p2c_record_result_updates_stats() {
        let lb = P2CLoadBalance::new();
        let key = "192.168.1.1:50051".to_string();

        let stats = lb.get_or_create_stats(&key);
        stats.active_requests.store(5, Ordering::Relaxed);
        drop(stats);

        lb.record_result(&key, 20, true);

        let stats = lb.get_or_create_stats(&key);
        assert_eq!(stats.active_requests.load(Ordering::Relaxed), 4);
        assert_eq!(stats.total_requests.load(Ordering::Relaxed), 1);
        assert_eq!(stats.total_errors.load(Ordering::Relaxed), 0);
        assert!(stats.ewma_latency_ms.load(Ordering::Relaxed) > 0);
    }

    #[test]
    fn test_p2c_ewma_converges() {
        let lb = P2CLoadBalance::new();
        let key = "192.168.1.1:50051".to_string();

        let stats = lb.get_or_create_stats(&key);
        stats.active_requests.store(100, Ordering::Relaxed);
        drop(stats);

        for _ in 0..50 {
            lb.record_result(&key, 100, true);
        }

        let stats = lb.get_or_create_stats(&key);
        let ewma_fp = stats.ewma_latency_ms.load(Ordering::Relaxed);
        let ewma_ms = ewma_fp / 1000;
        assert!(
            (80..=120).contains(&ewma_ms),
            "EWMA should converge near 100ms, got {ewma_ms}ms"
        );
    }

    #[test]
    fn test_p2c_weight_bias() {
        let lb = P2CLoadBalance::new();

        let invokers: Vec<Box<dyn Invoker>> = vec![
            Box::new(TestInvoker::new("192.168.1.1", 10)),
            Box::new(TestInvoker::new("192.168.1.2", 500)),
        ];

        let mut counts = [0usize; 2];
        for _ in 0..500 {
            let idx = lb.select(&invokers, &URL::default(), &dummy_ctx()).unwrap();
            counts[idx] += 1;
            let key = P2CLoadBalance::invoker_key(&invokers[idx]);
            lb.record_result(&key, 10, true);
        }

        assert!(
            counts[1] > counts[0],
            "higher-weight invoker should get more traffic: {} vs {}",
            counts[1],
            counts[0]
        );
    }

    #[test]
    fn test_p2c_handles_zero_latency() {
        let lb = P2CLoadBalance::new();
        let key = "192.168.1.1:50051".to_string();

        let stats = lb.get_or_create_stats(&key);
        stats.active_requests.store(1, Ordering::Relaxed);
        drop(stats);

        lb.record_result(&key, 0, true);

        let stats = lb.get_or_create_stats(&key);
        assert_eq!(stats.active_requests.load(Ordering::Relaxed), 0);
        assert_eq!(stats.total_requests.load(Ordering::Relaxed), 1);
        assert_eq!(stats.ewma_latency_ms.load(Ordering::Relaxed), 0);

        let invokers: Vec<Box<dyn Invoker>> = vec![
            Box::new(TestInvoker::new("192.168.1.1", 100)),
            Box::new(TestInvoker::new("192.168.1.2", 100)),
        ];
        let result = lb.select(&invokers, &URL::default(), &dummy_ctx());
        assert!(result.is_ok());
        assert!(result.unwrap() < 2);
    }

    #[test]
    fn test_p2c_records_errors() {
        let lb = P2CLoadBalance::new();
        let key = "192.168.1.1:50051".to_string();

        let stats = lb.get_or_create_stats(&key);
        stats.active_requests.store(3, Ordering::Relaxed);
        drop(stats);

        lb.record_result(&key, 50, false);
        lb.record_result(&key, 30, false);
        lb.record_result(&key, 10, true);

        let stats = lb.get_or_create_stats(&key);
        assert_eq!(stats.active_requests.load(Ordering::Relaxed), 0);
        assert_eq!(stats.total_requests.load(Ordering::Relaxed), 3);
        assert_eq!(stats.total_errors.load(Ordering::Relaxed), 2);
    }
}
