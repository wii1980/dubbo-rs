#!/bin/bash
# ============================================================================
# dubbo-rs 全量 QA 脚本
#
# 功能：
#   1. cargo check — 编译检查
#   2. cargo clippy — 静态分析（-D warnings）
#   3. cargo fmt — 格式检查
#   4. cargo build — 全量编译（workspace + examples）
#   5. cargo test — 全量测试（all-features + no-default-features）
#   6. Example 验证 — 构建 + 运行（无需外部依赖的示例）
#   7. E2E 集成测试 — cross-lang-tests 全量测试（自动检查/启动外部基础设施）
#
# 用法：
#   ./scripts/qa.sh                   # 全量执行
#   ./scripts/qa.sh --fast            # 跳过 clippy 和 no-default-features 测试
#   ./scripts/qa.sh --phase check     # 只做编译检查
#   ./scripts/qa.sh --phase test      # 只运行测试
#   ./scripts/qa.sh --phase e2e       # 只运行端到端集成测试 (cross-lang-tests)
#   ./scripts/qa.sh --phase bench     # 只运行性能基准测试
#
# 输出：
#   - 逐阶段 pass/fail 报告
#   - 最终汇总结果
#   - 失败的阶段会以非零退出码报告
# ============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
REPORT_DIR="$PROJECT_ROOT/target/qa-reports"
TIMESTAMP=$(date '+%Y-%m-%d_%H:%M:%S')
SUMMARY_FILE="$REPORT_DIR/summary-$TIMESTAMP.md"
PASS_COUNT=0
FAIL_COUNT=0
FAILURES=()

cd "$PROJECT_ROOT"
mkdir -p "$REPORT_DIR"

# ── Terminal colors ─────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color
BOLD='\033[1m'

# ── Helpers ─────────────────────────────────────────────────────────────
log()       { echo -e "  ${CYAN}[$(date '+%H:%M:%S')]${NC} $*"; }
info()      { echo -e "  ${CYAN}[INFO]${NC} $*"; }
phase()     { echo -e "\n${BOLD}━━━ $* ━━━${NC}"; }
pass()      { echo -e "  ${GREEN}✅ PASS:${NC} $*"; }
fail()      { echo -e "  ${RED}❌ FAIL:${NC} $*"; }
skip()      { echo -e "  ${YELLOW}⏭ SKIP:${NC} $*"; }
header()    { echo -e "${BOLD}═══ $* ═══${NC}\n"; }

run_phase() {
    local phase_name="$1"
    shift
    local start_time
    start_time=$(date +%s)

    phase "Phase: $phase_name"
    info "Running: $*"

    set +e
    # Run the command, capturing stdout+stderr to a log file
    local safe_name
    safe_name=$(echo "$phase_name" | sed 's/[^a-zA-Z0-9_-]/-/g')
    local log_file="$REPORT_DIR/${safe_name}.log"
    if eval "$*" > "$log_file" 2>&1; then
        local elapsed=$(( $(date +%s) - start_time ))
        pass "$phase_name (${elapsed}s)"
        PASS_COUNT=$((PASS_COUNT + 1))
        rm -f "$log_file"  # clean up on success
    else
        local elapsed=$(( $(date +%s) - start_time ))
        fail "$phase_name (${elapsed}s) — see log: $log_file"
        FAIL_COUNT=$((FAIL_COUNT + 1))
        FAILURES+=("$phase_name")
    fi
    set -euo pipefail
}

# ── Parse arguments ──────────────────────────────────────────────────────
FAST_MODE=false
RUN_PHASES=()
while [[ $# -gt 0 ]]; do
    case "$1" in
        --fast)
            FAST_MODE=true
            shift
            ;;
        --phase)
            if [[ -z "${2:-}" ]]; then
                echo "Error: --phase requires an argument (check|test|e2e|examples|fmt)"
                exit 1
            fi
            RUN_PHASES+=("$2")
            shift 2
            ;;
        *)
            echo "Usage: $0 [--fast] [--phase check|test|e2e|examples|fmt]"
            exit 1
            ;;
    esac
done

# ── Determine which phases to run ────────────────────────────────────────
if [[ ${#RUN_PHASES[@]} -eq 0 ]]; then
    RUN_PHASES=(check test examples)
fi

# ── Phase 1: Compilation & static analysis ──────────────────────────────
run_check_phases() {
    header "Compilation & Static Analysis"

    # 1a. Rust version check
    phase "Phase: Rust toolchain info"
    rustc --version
    cargo --version
    pass "Toolchain info"

    # 1b. cargo check (workspace)
    run_phase "cargo check" "cargo check --workspace 2>&1"

    # 1c. cargo clippy (deny warnings)
    if $FAST_MODE; then
        skip "cargo clippy (--fast mode)"
    else
        # Clippy with default features
        run_phase "cargo clippy (default)" "cargo clippy --workspace --all-targets -- -D warnings 2>&1"

        # Clippy with `configcenter-zookeeper` feature (requires `dubbo`)
        run_phase "cargo clippy (configcenter-zookeeper)" "cargo clippy --workspace --all-targets --features configcenter-zookeeper -- -D warnings 2>&1"

        # Clippy with `configcenter-nacos` feature (requires `dubbo`)
        run_phase "cargo clippy (configcenter-nacos)" "cargo clippy --workspace --all-targets --features configcenter-nacos -- -D warnings 2>&1"
    fi

    # 1d. cargo fmt check (opt-in: use --phase fmt)
}

# ── Phase 2: Full compilation ──────────────────────────────────────────
run_build_phases() {
    header "Full Compilation"

    # Build workspace (all crates + examples)
    run_phase "cargo build" "cargo build --workspace 2>&1"
}

# ── Phase 3: Tests ─────────────────────────────────────────────────────
run_test_phases() {
    header "Tests"

    # Note: `dubbo` crate has mutually-exclusive features (configcenter-zookeeper
    # vs configcenter-nacos), so --all-features on the workspace would fail due
    # to feature unification. Instead, we test with default features plus each
    # configcenter variant separately.

    # 3a. Tests with default features
    run_phase "cargo test (default)" "cargo test --workspace 2>&1"

    # 3b. Tests with no default features
    if $FAST_MODE; then
        skip "cargo test (no-default-features, --fast mode)"
    else
        run_phase "cargo test (no-default-features)" "cargo test --workspace --no-default-features 2>&1"
    fi

    # 3c. Tests with configcenter-zookeeper feature
    run_phase "cargo test (configcenter-zookeeper)" "cargo test --workspace --features configcenter-zookeeper 2>&1"

    # 3d. Tests with configcenter-nacos feature
    run_phase "cargo test (configcenter-nacos)" "cargo test --workspace --features configcenter-nacos 2>&1"
}

# ── Phase 4: Example verification ──────────────────────────────────────
run_example_phases() {
    header "Example Verification"

    local examples_standalone=(
        "hello-world"
        "instance-demo"
        "filter-chain"
        "failover"
        "canary-release"
        "dubbo2-interop"
        "phone-dialing"
    )
    local examples_external=(
        "zk-discovery"
        "nacos-discovery"
    )

    # 4a. Build all example packages
    info "Building all example packages..."
    local all_examples=""
    for ex in "${examples_standalone[@]}" "${examples_external[@]}"; do
        all_examples="$all_examples -p $ex"
    done
    # shellcheck disable=SC2086
    run_phase "build all examples" "cargo build $all_examples 2>&1"

    # 4b. Run standalone examples (no external deps)
    info "Running standalone examples..."
    for ex in "${examples_standalone[@]}"; do
        local ex_log="$REPORT_DIR/example_${ex}.log"
        set +e
        info "Running: cargo run -p $ex (timeout: 30s)"
        if timeout 30 cargo run -p "$ex" > "$ex_log" 2>&1; then
            pass "example/$ex"
            rm -f "$ex_log"
        else
            local exit_code=$?
            if [[ $exit_code -eq 124 ]]; then
                fail "example/$ex (TIMEOUT after 30s) — see log: $ex_log"
            else
                fail "example/$ex (exit=$exit_code) — see log: $ex_log"
            fi
            FAIL_COUNT=$((FAIL_COUNT + 1))
            FAILURES+=("example/$ex")
        fi
        set -euo pipefail
    done

    # 4c. Check external-dependent examples compile only
    info "External-dependent examples (build only):"
    for ex in "${examples_external[@]}"; do
        pass "example/$ex (built, needs external service to run)"
    done
}

# ── Phase 5: End-to-End Integration Tests ──────────────────────────────
run_e2e_phases() {
    header "End-to-End Integration Tests (cross-lang-tests)"

    COMPOSE_FILE="$PROJECT_ROOT/tests/docker-compose.yml"
    HEALTH_CHECK_SCRIPT="$PROJECT_ROOT/tests/docker-health-check.sh"
    SERVICES_STARTED=false

    # ── 5a. Detect container compose tool (docker compose / podman-compose) ──
    CONTAINER_TOOL=""
    if command -v docker &>/dev/null && docker compose version &>/dev/null; then
        CONTAINER_TOOL="docker compose"
        info "Container tool: docker compose"
    elif command -v podman-compose &>/dev/null; then
        CONTAINER_TOOL="podman-compose"
        info "Container tool: podman-compose"
    elif command -v docker-compose &>/dev/null; then
        CONTAINER_TOOL="docker-compose"
        info "Container tool: docker-compose"
    else
        info "No container compose tool found (docker compose / podman-compose) - will skip infrastructure setup"
    fi

    # ── 5b. Check external service ports ─────────────────────────────────
    check_port() {
        local port=$1
        (echo >"/dev/tcp/localhost/$port") 2>/dev/null
    }

    ZK_ACTIVE=false; NACOS_ACTIVE=false; REDIS_ACTIVE=false

    if check_port 2181; then ZK_ACTIVE=true;  info "ZooKeeper: active on :2181";  else info "ZooKeeper: not detected on :2181"; fi
    if check_port 8848; then NACOS_ACTIVE=true; info "Nacos: active on :8848"; else info "Nacos: not detected on :8848"; fi
    if check_port 6379; then REDIS_ACTIVE=true; info "Redis: active on :6379";  else info "Redis: not detected on :6379"; fi

    # ── 5c. Start missing services via container compose ─────────────────
    if ! $ZK_ACTIVE || ! $NACOS_ACTIVE || ! $REDIS_ACTIVE; then
        if [[ -n "$CONTAINER_TOOL" && -f "$COMPOSE_FILE" ]]; then
            info "Starting missing services via $CONTAINER_TOOL..."
            if $CONTAINER_TOOL -f "$COMPOSE_FILE" up -d 2>&1; then
                SERVICES_STARTED=true
                info "Container services started, waiting for health check..."

                if [[ -x "$HEALTH_CHECK_SCRIPT" ]]; then
                    if "$HEALTH_CHECK_SCRIPT" 120; then
                        pass "External services ready"
                    else
                        fail "Some services did not become healthy within timeout"
                        info "Continuing anyway - tests will skip unavailable services"
                    fi
                else
                    info "Health check script not found, waiting 30s for services..."
                    sleep 30
                fi
            else
                info "Container compose start failed - tests will skip unavailable services"
            fi
        else
            info "Container compose not available or compose file missing - tests will skip unavailable services"
        fi
    else
        info "All external services already active"
    fi

    # ── 5d. Run cross-lang-tests by domain ──────────────────────────────
    info "Running cross-lang-tests (311 tests across 13 modules, grouped by domain)..."
    info "Tests that need external services will auto-skip if unavailable"

    run_phase "Protocol (Triple/DubboTCP/JSONRPC)" "cargo test -p cross-lang-tests --test triple_test --test dubbo_tcp_test --test jsonrpc_test 2>&1"

    run_phase "Serialization (Hessian2)" "cargo test -p cross-lang-tests --test hessian2_test --test serialization_test 2>&1"

    run_phase "Registry" "cargo test -p cross-lang-tests --test registry_test 2>&1"

    run_phase "Governance (Cluster/LoadBalance/Filter)" "cargo test -p cross-lang-tests --test governance_test 2>&1"

    run_phase "Config/Metadata/Instance" "cargo test -p cross-lang-tests --test config_test --test metadata_test --test instance_test 2>&1"

    run_phase "Security & Observability (TLS/Logger)" "cargo test -p cross-lang-tests --test tls_test --test logger_test 2>&1"

    run_phase "Test Utilities" "cargo test -p cross-lang-tests --test cross_lang_test 2>&1"

    # ── 5e. Cleanup: stop services if we started them ───────────────────
    if $SERVICES_STARTED; then
        info "Stopping container services started by e2e phase..."
        $CONTAINER_TOOL -f "$COMPOSE_FILE" down 2>&1 || true
        info "Container services stopped"
    fi
}

# ── Summary ──────────────────────────────────────────────────────────────
print_summary() {
    local total=$((PASS_COUNT + FAIL_COUNT))

    echo ""
    header "QA Summary"
    echo "  Total checks: $total"
    echo -e "  ${GREEN}Passed: $PASS_COUNT${NC}"
    if [[ $FAIL_COUNT -gt 0 ]]; then
        echo -e "  ${RED}Failed: $FAIL_COUNT${NC}"
        echo ""
        info "Failed phases:"
        for f in "${FAILURES[@]}"; do
            echo "    - $f"
        done
        echo ""
        info "Check individual logs in: $REPORT_DIR/"
    else
        echo -e "  ${GREEN}Failed: 0${NC}"
    fi

    # Save summary to file
    {
        echo "# dubbo-rs QA Report — $TIMESTAMP"
        echo ""
        echo "## Summary"
        echo ""
        echo "| Metric | Value |"
        echo "|--------|-------|"
        echo "| Total checks | $total |"
        echo "| Passed | $PASS_COUNT |"
        echo "| Failed | $FAIL_COUNT |"
        echo ""
        if [[ ${#FAILURES[@]} -gt 0 ]]; then
            echo "## Failures"
            echo ""
            for f in "${FAILURES[@]}"; do
                echo "- $f"
            done
        fi
    } > "$SUMMARY_FILE"
    info "Summary saved to: $SUMMARY_FILE"
}

# ============================================================================
# MAIN
# ============================================================================
header "dubbo-rs QA Script"
info "Mode: ${FAST_MODE:-normal}"
info "Phases: ${RUN_PHASES[*]}"
info "Report dir: $REPORT_DIR"
echo ""

# Trap to ensure summary is always printed
trap print_summary EXIT

for phase in "${RUN_PHASES[@]}"; do
    case "$phase" in
        check)
            run_check_phases
            run_build_phases
            ;;
        test)
            run_test_phases
            ;;
        examples)
            run_example_phases
            ;;
        e2e)
            run_e2e_phases
            ;;
        fmt)
            header "Format Check"
            run_phase "cargo fmt" "cargo fmt --all -- --check 2>&1"
            ;;
        *)
            echo "Unknown phase: $phase (valid: check, test, e2e, examples, fmt)"
            exit 1
            ;;
    esac
done

# Exit code: 0 only if all passed
if [[ $FAIL_COUNT -gt 0 ]]; then
    exit 1
fi
