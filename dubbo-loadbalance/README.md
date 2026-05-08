# dubbo-rs-loadbalance

[![crates.io](https://img.shields.io/crates/v/dubbo-rs-loadbalance)](https://crates.io/crates/dubbo-rs-loadbalance)
[![docs.rs](https://docs.rs/dubbo-rs-loadbalance/badge.svg)](https://docs.rs/dubbo-rs-loadbalance)
[![Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Load balancing strategies for selecting among multiple service invokers in dubbo-rs.

Provides the `LoadBalance` trait and four implementations: weighted random, weighted round-robin, least-active, and consistent hashing. Supports warmup-based weight scaling for newly started providers.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
dubbo-rs-loadbalance = "0.1"
```

Or use `cargo add`:

```bash
cargo add dubbo-rs-loadbalance
```

## Key Types

### `LoadBalance` trait

```rust
pub trait LoadBalance: Send + Sync {
    fn select(
        &self,
        invokers: &[Box<dyn Invoker>],
        url: &URL,
        invocation: &InvocationContext,
    ) -> Result<usize, RPCError>;
}
```

Returns the index of the selected invoker, or `RPCError::ServiceNotFound` if the list is empty.

### Implementations

| Struct | Strategy | Weighted |
|--------|----------|----------|
| `RandomLoadBalance` | Weighted random via `rand` | Yes — proportional to `weight` param |
| `RoundRobinLoadBalance` | Weighted round-robin with `AtomicUsize` counter | Yes — GCD-based scheduling |
| `LeastActiveLoadBalance` | Selects invoker with lowest `active` param | Yes — ties broken by weight |
| `ConsistentHashLoadBalance` | Hashes first argument to an invoker index | No — uses `DefaultHasher` |

### Weight Helpers

| Function | Description |
|----------|-------------|
| `get_weight(invoker)` | Reads `weight` URL param (default: 100) |
| `get_warmup(invoker)` | Reads `warmup` URL param (default: 600_000 ms) |
| `calculate_warmup_weight(invoker, weight)` | Scales weight during warmup period based on `timestamp` param |

## Usage

```rust
use dubbo_rs_common::url::URL;
use dubbo_rs_protocol::{InvocationContext, Invoker};
use dubbo_rs_loadbalance::{LoadBalance, RandomLoadBalance, RoundRobinLoadBalance,
    LeastActiveLoadBalance, ConsistentHashLoadBalance};

fn example(invokers: &[Box<dyn Invoker>]) {
    let url = URL::new("tri", "/com.example.GreetService");
    let ctx = InvocationContext::new("sayHello", url.clone());

    // Weighted random — distributes proportionally to weight param
    let lb = RandomLoadBalance;
    let idx = lb.select(invokers, &url, &ctx).unwrap();

    // Round-robin — sequential, respects weights
    let lb = RoundRobinLoadBalance::new();
    let idx = lb.select(invokers, &url, &ctx).unwrap();

    // Least active — picks invoker with lowest "active" URL param
    let lb = LeastActiveLoadBalance;
    let idx = lb.select(invokers, &url, &ctx).unwrap();

    // Consistent hash — same args always route to same invoker
    let lb = ConsistentHashLoadBalance::new()
        .with_virtual_nodes(320);
    let idx = lb.select(invokers, &url, &ctx).unwrap();
}
```

### Configuring provider weights

Set the `weight` URL parameter on provider URLs when registering:

```rust
let mut url = URL::new("tri", "/com.example.GreetService");
url.ip = "192.168.1.1".to_string();
url.set_param("weight", "200");  // receives ~2x traffic
url.set_param("active", "3");    // used by LeastActiveLoadBalance
```

## Re-exports

- `pub use dubbo_rs_common as common`
- `pub use dubbo_rs_protocol as protocol`

## License

Apache-2.0
