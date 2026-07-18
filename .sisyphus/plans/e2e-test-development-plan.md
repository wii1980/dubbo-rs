# E2E Test Development Plan

## Objective
Systematically fill the identified e2e test coverage gaps in `cross-lang-tests/tests/`, prioritized by impact and dependency.

## Current Coverage Summary
- **Total e2e tests**: 235 (all passing)
- **Test files**: hessian2_test (54), dubbo_tcp_test (32), triple_test (13), registry_test (55), governance_test (67), cross_lang_test (14)
- **Identified gaps**: 38 features across governance, cross-language, integration, and zero-coverage crates
- **Closing rate**: 14/38 gaps closed in this session

## Phases

### Phase 1 — Triple Cross-Language Tests (P0) — BLOCKED
**Goal**: Add Rust↔Java cross-language Triple protocol tests.
**Status**: 🚫 Blocked — `TripleTestServer.java` is only a skeleton/stub (no real Triple server). Needs Java-side implementation first (Dubbo ServiceConfig with "tri" protocol).
**Infrastructure**: `JavaServer::start_triple()` exists. `JavaServer::start_dubbo_tcp()` works for TCP tests.
**Files affected**: `tests/java-fixtures/src/main/java/.../TripleTestServer.java` (needs real impl first)

### Phase 2 — Instance Full-Stack Smoke Test (P0)
**Goal**: Add a single end-to-end test that exercises the complete dubbo-rs stack.
**Files affected**: New file `cross-lang-tests/tests/instance_test.rs`

| # | Task | Description | Verification |
|---|------|-------------|-------------|
| 2.1 | Instance start + serve + invoke | Create minimal YAML config (in-memory), build Instance, start server, connect client through Cluster/Filter/Protocol stack, invoke sayHello | `cargo test --package cross-lang-tests --test instance_test` |
| 2.2 | Triple protocol via Instance | Same but explicitly using Triple protocol | same |

### Phase 3 — Cluster Strategy Tests (P0)
**Goal**: Add e2e tests for cluster implementations not yet covered.
**Files affected**: `cross-lang-tests/tests/governance_test.rs`

| # | Task | Description | Verification |
|---|------|-------------|-------------|
| 3.1 | ForkingCluster e2e | Test parallel invocation across multiple invokers | `cargo test --package cross-lang-tests --test governance_test -- e_002i` |
| 3.2 | BroadcastCluster e2e | Test broadcast to all invokers | `cargo test --package cross-lang-tests --test governance_test -- e_002j` |
| 3.3 | ConditionRouter e2e | Test rule-based routing ("region=beijing => env=gray") | `cargo test --package cross-lang-tests --test governance_test -- e_002k` |

### Phase 4 — Load Balancer Tests (P1)
**Goal**: Add e2e tests for remaining load balancers.
**Files affected**: `cross-lang-tests/tests/governance_test.rs`

| # | Task | Description | Verification |
|---|------|-------------|-------------|
| 4.1 | ShortestResponseLoadBalance e2e | Test shortest-response-time selection with RT data | `cargo test --package cross-lang-tests --test governance_test -- e_001m` |
| 4.2 | P2CLoadBalance e2e | Test Power of Two Choices + EWMA latency estimation | `cargo test --package cross-lang-tests --test governance_test -- e_001n` |

### Phase 5 — Additional Filter Tests (P1)
**Goal**: Add e2e tests for important untested filters.
**Files affected**: `cross-lang-tests/tests/governance_test.rs`

| # | Task | Description | Verification |
|---|------|-------------|-------------|
| 5.1 | TPSLimitFilter e2e | Test rate limiting behavior | `cargo test --package cross-lang-tests --test governance_test -- e_003n` |
| 5.2 | AuthFilter e2e | Test HMAC-SHA256 signature authentication | `cargo test --package cross-lang-tests --test governance_test -- e_003o` |
| 5.3 | GenericInvoker e2e | Test dynamic invocation without POJOs | `cargo test --package cross-lang-tests --test governance_test -- e_003p` |

### Phase 6 — TLS Integration Tests (P1)
**Goal**: Add TLS/mTLS integration tests.
**Files affected**: New file `cross-lang-tests/tests/tls_test.rs`

| # | Task | Description | Verification |
|---|------|-------------|-------------|
| 6.1 | TLS Triple server+client | Start Triple server with TLS, connect with TLS client, invoke RPC | `cargo test --package cross-lang-tests --test tls_test` |
| 6.2 | mTLS mutual authentication | Both server and client present certificates | same |

### Phase 7 — Remaining Governance Tests (P2)
**Goal**: Cover remaining cluster strategies, filters, and routers.
**Files affected**: `cross-lang-tests/tests/governance_test.rs`

| # | Task | Description | Verification |
|---|------|-------------|-------------|
| 7.1 | FailsafeCluster e2e | Test silent error swallowing | `cargo test --package cross-lang-tests --test governance_test` |
| 7.2 | FailbackCluster e2e | Test auto-recovery retry | same |
| 7.3 | TagRouter e2e | Test traffic coloring | same |
| 7.4 | Rest of filters | ActiveLimit, ExecuteLimit, Timeout, Context, Exception, Cache | same |

## File Change Summary

| File | Action | Phase |
|------|--------|-------|
| `cross-lang-tests/tests/triple_test.rs` | Add tests | Phase 1 |
| `cross-lang-tests/tests/instance_test.rs` | **New file** | Phase 2 |
| `cross-lang-tests/tests/governance_test.rs` | Add tests | Phase 3, 4, 5, 7 |
| `cross-lang-tests/tests/tls_test.rs` | **New file** | Phase 6 |
| `cross-lang-tests/Cargo.toml` | Add deps if needed | Phase 2, 6 |

## Running Tests
```bash
# All tests (including ignored)
cargo test --package cross-lang-tests -- --include-ignored

# Single file
cargo test --package cross-lang-tests --test governance_test
cargo test --package cross-lang-tests --test triple_test

# Single test
cargo test --package cross-lang-tests --test governance_test -- e_002i
```

## Session Completion Summary

### Completed (this session)
| Phase | Tasks | Tests Added |
|-------|-------|:-----------:|
| Cleanup | Removed 2 trivial tests (cross_lang_test dubbo_tcp) | -2 |
| Strengthen | Replaced 11 shallow construction tests with real assertions (governance_test) | 0 |
| Phase 3 | ForkingCluster, BroadcastCluster, ConditionRouter, TagRouter | +8 |
| Phase 4 | ShortestResponseLoadBalance, P2CLoadBalance | +4 |
| Phase 5 | AuthFilter (provider-side signature validation) | +2 |
| **Total new tests** | | **+14** |

### New coverage areas
- **Clusters**: ForkingCluster (parallel invoke, all-fail), BroadcastCluster (invoke all, partial failure)
- **Routers**: ConditionRouter (region-based filtering), TagRouter (tag matching, untagged fallback)
- **Load Balancers**: ShortestResponseLoadBalance (lowest-RT selection, no-RT fallback), P2CLoadBalance (balanced distribution)
- **Filters**: AuthFilter (missing signature passthrough, unknown key rejection)

### Remaining for future sessions
- Phase 1: Triple cross-language (blocked on Java side)
- Phase 2: Instance full-stack smoke test
- Other untested crates: config, tls, metrics, tracing, codegen, metadata, configcenter, native gRPC, JSON-RPC, REST
- Untested governance: ActiveLimitFilter, ExecuteLimitFilter, TimeoutFilter, GenericInvoker, ContextFilter, ExceptionFilter, CacheFilter
- Untested clusters: FailsafeCluster, FailbackCluster, AvailableCluster, MockCluster
- Dubbo TCP reverse direction (Java→Rust)
```
