use std::time::Duration;

#[derive(Debug, Clone)]
pub struct StressReport {
    pub total_requests: u64,
    pub successful: u64,
    pub failed: u64,
    pub elapsed: Duration,
    pub latencies_us: Vec<u64>,
}

impl StressReport {
    #[must_use] pub fn qps(&self) -> f64 {
        if self.elapsed.is_zero() {
            return 0.0;
        }
        self.total_requests as f64 / self.elapsed.as_secs_f64()
    }

    #[must_use] pub fn avg_latency_us(&self) -> f64 {
        if self.latencies_us.is_empty() {
            return 0.0;
        }
        let sum: u64 = self.latencies_us.iter().sum();
        sum as f64 / self.latencies_us.len() as f64
    }

    #[must_use] pub fn percentile_latency_us(&self, p: f64) -> u64 {
        if self.latencies_us.is_empty() {
            return 0;
        }
        let mut sorted = self.latencies_us.clone();
        sorted.sort_unstable();
        let idx = ((p / 100.0) * (sorted.len() - 1) as f64).round() as usize;
        sorted[idx.min(sorted.len() - 1)]
    }

    pub fn print_report(&self, label: &str) {
        println!("\n=== Stress Test Report: {label} ===");
        println!("Total requests:  {}", self.total_requests);
        println!("Successful:      {}", self.successful);
        println!("Failed:          {}", self.failed);
        println!("Elapsed:         {:.3}s", self.elapsed.as_secs_f64());
        println!("QPS:             {:.0}", self.qps());
        println!("Avg latency:     {:.1} us", self.avg_latency_us());
        println!("P50 latency:     {} us", self.percentile_latency_us(50.0));
        println!("P90 latency:     {} us", self.percentile_latency_us(90.0));
        println!("P99 latency:     {} us", self.percentile_latency_us(99.0));
        println!(
            "Max latency:     {} us",
            self.latencies_us.iter().max().unwrap_or(&0)
        );
        println!("=========================================");
    }
}

pub struct StressCollector {
    pub latencies_us: Vec<u64>,
    successful: u64,
    failed: u64,
}

impl StressCollector {
    #[must_use] pub fn new(capacity: usize) -> Self {
        Self {
            latencies_us: Vec::with_capacity(capacity),
            successful: 0,
            failed: 0,
        }
    }

    pub fn record_success(&mut self, latency: Duration) {
        self.latencies_us.push(latency.as_micros() as u64);
        self.successful += 1;
    }

    pub fn record_failure(&mut self) {
        self.failed += 1;
    }

    #[must_use] pub fn finalize(self, elapsed: Duration) -> StressReport {
        StressReport {
            total_requests: self.successful + self.failed,
            successful: self.successful,
            failed: self.failed,
            elapsed,
            latencies_us: self.latencies_us,
        }
    }
}
