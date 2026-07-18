# Contributing to dubbo-rs

## Development Environment

- Rust 1.87+ (stable)
- Cargo workspace with 19+ crates

```bash
# Clone and build
git clone <repo-url>
cd dubbo-rs
cargo build

# Run tests
cargo test --workspace
```

## Code Style

- Follow `rustfmt` (run `cargo fmt --all` before committing)
- Pass `clippy` checks (`cargo clippy --workspace --all-targets -- -D warnings`)
- Avoid `unsafe` code; wrappers in `lib.rs` expose safe re-exports
- Use `thiserror` for error types, `async_trait` for async traits
- Prefer `workspace = true` for dependency versions

## Testing

- Write unit tests in `#[cfg(test)] mod tests` blocks within source files
- Target >80% coverage for all P0 crates
- Integration tests go in `examples/` directory
- Run `cargo test --workspace` before submitting PR

## Commit Guidelines

- Atomic commits with descriptive messages
- Format: `scope: brief description`
- Examples: `common: add URL type and Node trait`, `ci: add GitHub Actions workflow`

## Crate Organization

Phase 0 (current) enables a compilable workspace skeleton. See `DEVELOPMENT_PLAN.md` for the full roadmap.

| Crate | Purpose | Phase |
|-------|---------|-------|
| `dubbo-rs-common` | Core types, URL, Node trait, SPI | P0 |
| `dubbo-rs-macros` | Service/proxy proc-macros | P0 |
| `dubbo-rs-protocol` | Protocol trait definitions | P0 |
| `dubbo-rs-serialization` | Serialization trait | P0 |
| `dubbo-rs-serialization-protobuf` | Protobuf serialization | P0 |
| `dubbo-rs-remoting` | Network transport layer | P0 |
| `dubbo-rs-protocol-triple` | Triple protocol (HTTP/2 gRPC) | P0 |
| `dubbo-rs-registry` | Registry trait definitions | P0 |
| `dubbo-rs-registry-zookeeper` | ZooKeeper registry | P0 |
| `dubbo-rs-registry-nacos` | Nacos registry | P0 |
| `dubbo-rs-cluster` | Cluster fault tolerance | P0 |
| `dubbo-rs-loadbalance` | Load balancing strategies | P0 |
| `dubbo-rs-filter` | Filter chain framework | P0 |
| `dubbo-rs-proxy` | Service proxy layer | P0 |
| `dubbo-rs-client` | Client high-level API | P0 |
| `dubbo-rs-server` | Server high-level API | P0 |
| `dubbo` | Top-level Instance API | P0 |
| `dubbo-rs-config` | Configuration loading | P1 |
| `dubbo-rs-logger` | Logging abstraction | P2 |

## License

Apache-2.0
