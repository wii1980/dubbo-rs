# dubbo-rs-cluster

[![crates.io](https://img.shields.io/crates/v/dubbo-rs-cluster)](https://crates.io/crates/dubbo-rs-cluster)
[![docs.rs](https://docs.rs/dubbo-rs-cluster/badge.svg)](https://docs.rs/dubbo-rs-cluster)
[![Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Cluster fault-tolerance strategies, service directory abstraction, and traffic routing for dubbo-rs.

Provides the `Directory` and `Cluster` traits that decouple service discovery from invocation strategy, along with `ConditionRouter`, `TagRouter`, and `RouterChain` for traffic management.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
dubbo-rs-cluster = "0.1"
```

Or use `cargo add`:

```bash
cargo add dubbo-rs-cluster
```

## Key Types

### `Directory` trait

Supplies available invokers for a service. Can be static or backed by a registry.

| Method | Description |
|--------|-------------|
| `list(ctx)` | Returns `Vec<Arc<dyn Invoker>>` for the invocation |
| `get_url()` | Returns the service URL |

### `Cluster` trait

Joins a directory into a single fault-tolerant invoker.

| Method | Description |
|--------|-------------|
| `join(directory)` | Returns `Box<dyn Invoker>` with cluster logic |

### Cluster Implementations

| Struct | Behavior |
|--------|----------|
| `FailoverCluster` | Retries up to `retries + 1` total attempts across all invokers (default retries: 2) |
| `FailfastCluster` | Single attempt on the first available invoker, no retry |

### Directory Implementations

| Struct | Description |
|--------|-------------|
| `StaticDirectory` | Fixed invoker list for direct-connect mode |
| `RegistryDirectory` | Dynamic list updated via `NotifyListener`; supports custom `InvokerFactory` |

### Routers

| Type | Description |
|------|-------------|
| `ConditionRouter` | Rule-based filtering: `"region=beijing => env=gray"` syntax |
| `TagRouter` | Traffic coloring via `dubbo.tag` attachment; falls back to untagged invokers |
| `RouterChain` | Sequential pipeline of condition + tag routers |

## Usage

### Failover cluster with static directory

```rust
use dubbo_rs_common::url::URL;
use dubbo_rs_cluster::{StaticDirectory, FailoverCluster, Cluster, Directory};
use dubbo_rs_protocol::InvocationContext;

#[tokio::main]
async fn main() {
    let dir = StaticDirectory::new(URL::new("tri", "/com.example.GreetService"));
    // dir.add_invoker(arc_invoker_1);
    // dir.add_invoker(arc_invoker_2);

    let cluster = FailoverCluster::new().with_retries(3);
    let invoker = cluster.join(Box::new(dir)).await.unwrap();

    let mut ctx = InvocationContext::new("sayHello", URL::new("tri", "/com.example.GreetService"));
    let result = invoker.invoke(&mut ctx).await;
}
```

### Router chain

```rust
use dubbo_rs_cluster::{RouterChain, ConditionRouter, TagRouter};

let chain = RouterChain::new()
    .with_condition_router(ConditionRouter::parse("region=beijing => env=gray").unwrap())
    .with_tag_router(TagRouter::default());
// let filtered_indices = chain.route(&invokers, &ctx);
```

## Re-exports

- `pub use dubbo_rs_common as common`
- `pub use dubbo_rs_protocol as protocol`
- `pub use dubbo_rs_registry as registry`

## License

Apache-2.0
