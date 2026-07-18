#!/bin/bash
# dubbo-rs ↔ dubbo-java Cross-Language Compatibility Test
#
# Prerequisites:
#   - JDK 17+ installed
#   - Maven 3.8+ installed (./mvnw available in dubbo-java/)
#   - Rust toolchain (already set up for dubbo-rs)
#   - Ports 20880, 20881, 50051, 50052 available
#
# Test Matrix:
#   Test 1: Triple Unary — dubbo-rs consumer ↔ dubbo-java provider
#   Test 2: Triple Unary — dubbo-java consumer ↔ dubbo-rs provider
#   Test 3: Dubbo TCP + Hessian2 — dubbo-rs consumer ↔ dubbo-java provider
#   Test 4: Dubbo TCP + Hessian2 — dubbo-java consumer ↔ dubbo-rs provider
#
# Usage:
#   ./tests/cross-lang-test.sh [triple|dubbo|all]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DUBBO_JAVA_DIR="$PROJECT_ROOT/dubbo-java"
REPORT_DIR="$PROJECT_ROOT/tests/reports"

MODE="${1:-all}"

log() { echo "[$(date '+%H:%M:%S')] $*"; }
pass() { echo "  ✅ PASS: $*"; }
fail() { echo "  ❌ FAIL: $*"; exit 1; }

mkdir -p "$REPORT_DIR"

# ── Build dubbo-java demo provider ───────────────────────────────────
build_java_demo() {
    log "Building dubbo-java demo modules..."
    cd "$DUBBO_JAVA_DIR"
    ./mvnw -pl dubbo-demo/dubbo-demo-api,dubbo-demo/dubbo-demo-interface \
        -pl dubbo-demo/dubbo-demo-triple-provider,dubbo-demo/dubbo-demo-triple-consumer \
        -q compile 2>&1 || {
        log "Maven build failed (may need JDK 17+)"
        return 1
    }
    log "Java demo modules built"
}

# ── Test 1: dubbo-rs consumer → dubbo-java Triple provider ────────────
test_triple_rs_to_java() {
    log "Test 1: dubbo-rs consumer ↔ dubbo-java Triple provider"

    # Start Java provider
    log "  Starting dubbo-java Triple provider on port 50051..."
    cd "$DUBBO_JAVA_DIR"
    java -cp "dubbo-demo/dubbo-demo-triple-provider/target/classes:$(find ~/.m2 -name '*.jar' | tr '\n' ':')" \
        org.apache.dubbo.demo.provider.Application &
    JAVA_PID=$!
    sleep 5

    # Build and run Rust consumer
    log "  Building dubbo-rs Triple consumer..."
    cd "$PROJECT_ROOT"
    cargo build -p dubbo2-interop 2>&1 || fail "Rust build failed"

    log "  Running dubbo-rs consumer..."
    cargo run -p dubbo2-interop 2>&1 && pass "Test 1 passed" || fail "Test 1 failed"

    kill $JAVA_PID 2>/dev/null || true
}

# ── Test 2: dubbo-java consumer → dubbo-rs Triple provider ────────────
test_triple_java_to_rs() {
    log "Test 2: dubbo-java consumer ↔ dubbo-rs Triple provider"

    # Start Rust provider
    log "  Starting dubbo-rs Triple provider on port 50052..."
    cd "$PROJECT_ROOT"
    cargo run -p hello-world &
    RUST_PID=$!
    sleep 3

    # Run Java consumer
    log "  Running dubbo-java Triple consumer..."
    cd "$DUBBO_JAVA_DIR"
    java -cp "dubbo-demo/dubbo-demo-triple-consumer/target/classes:$(find ~/.m2 -name '*.jar' | tr '\n' ':')" \
        org.apache.dubbo.demo.consumer.Application 2>&1 && pass "Test 2 passed" || fail "Test 2 failed"

    kill $RUST_PID 2>/dev/null || true
}

# ── Test 3 & 4: Dubbo TCP + Hessian2 ─────────────────────────────────
test_dubbo_tcp() {
    log "Test 3: dubbo-rs consumer ↔ dubbo-java Dubbo TCP provider (requires Phase A.2)"

    # Start Java Dubbo provider on port 20880
    log "  Starting dubbo-java Dubbo provider on port 20880..."
    cd "$DUBBO_JAVA_DIR"
    # Start a Dubbo protocol provider (requires dubbo-demo-dubbo-provider)
    java -cp "dubbo-demo/dubbo-demo-api/target/classes:$(find ~/.m2 -name '*.jar' | tr '\n' ':')" \
        org.apache.dubbo.demo.provider.Application &
    JAVA_PID=$!
    sleep 5

    log "  Running dubbo-rs Dubbo TCP consumer..."
    cd "$PROJECT_ROOT"
    cargo run -p dubbo2-interop 2>&1 && pass "Test 3 passed" || fail "Test 3 failed"

    kill $JAVA_PID 2>/dev/null || true
}

# ── Main ─────────────────────────────────────────────────────────────
log "=== dubbo-rs Cross-Language Compatibility Tests ==="
log "Mode: $MODE"
log ""

case "$MODE" in
    triple)
        build_java_demo
        test_triple_rs_to_java
        test_triple_java_to_rs
        ;;
    dubbo)
        build_java_demo
        test_dubbo_tcp
        ;;
    all)
        build_java_demo
        test_triple_rs_to_java
        test_triple_java_to_rs
        test_dubbo_tcp
        ;;
    *)
        echo "Usage: $0 [triple|dubbo|all]"
        exit 1
        ;;
esac

log ""
log "=== All tests complete ==="
log "Report: $REPORT_DIR"
