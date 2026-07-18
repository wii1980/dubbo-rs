#!/bin/bash
# ============================================================================
# dubbo-rs 逐 crate 发布脚本
#
# 版本号由人工在 Cargo.toml 中管理，脚本仅负责比较和发布。
# 判断逻辑：本地版本 > crates.io 版本（或远程不存在）→ 发布，否则跳过。
#
# 用法：
#   ./scripts/release.sh                   # 检查哪些 crate 需要发布
#   ./scripts/release.sh -x                # 实际发布（需要 crates.io token）
#   ./scripts/release.sh -x --no-push      # 实际发布但不 push
#   ./scripts/release.sh -p dubbo-rs-common # 只检查/发布指定 crate
#   ./scripts/release.sh -x --retries 5      # 失败后最多重试 5 次（默认 3）
#   ./scripts/release.sh -x --retry-delay 60 # 已有 crate 重试间隔 60s（默认，crates.io 限速 1/min）
#                                              # 新 crate 自动使用 600s（crates.io 限速 1/10min）
# ============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

# Published in dependency topology order — every crate appears AFTER all its
# internal dependencies, so `cargo publish` always finds them on crates.io.
CRATES=(
	# ── Layer 0: no internal dependencies ──
	dubbo-rs-common
	dubbo-rs-tls
	dubbo-rs-codegen
	dubbo-rs-metadata

	# ── Layer 1: depend only on common ──
	dubbo-rs-protocol
	dubbo-rs-serialization
	dubbo-rs-registry
	dubbo-rs-configcenter

	# ── Layer 2: depend on Layer 0-1 ──
	dubbo-rs-macros
	dubbo-rs-serialization-protobuf
	dubbo-rs-serialization-hessian2
	dubbo-rs-serialization-json
	dubbo-rs-remoting
	dubbo-rs-filter
	dubbo-rs-loadbalance
	dubbo-rs-config

	# ── Layer 3: depend on Layer 0-2 ──
	dubbo-rs-protocol-triple
	dubbo-rs-protocol-dubbo
	dubbo-rs-protocol-grpc
	dubbo-rs-protocol-jsonrpc
	dubbo-rs-protocol-rest
	dubbo-rs-registry-zookeeper
	dubbo-rs-registry-nacos
	dubbo-rs-registry-etcd
	dubbo-rs-registry-redis
	dubbo-rs-logger
	dubbo-rs-metrics
	dubbo-rs-tracing
	dubbo-rs-configcenter-zookeeper
	dubbo-rs-configcenter-nacos
	dubbo-rs-configcenter-apollo
	dubbo-rs-cluster

	# ── Layer 4: depend on Layer 0-3 ──
	dubbo-rs-proxy

	# ── Layer 5: depend on Layer 0-4 ──
	dubbo-rs-client
	dubbo-rs-server

	# ── Layer 6: top-level façade, depends on everything ──
	dubbo-rs
)

# ── Parse args ──
TARGET_CRATE=""
EXECUTE=false
NO_PUSH=false
MAX_RETRIES=3
RETRY_DELAY=60

while [[ $# -gt 0 ]]; do
	case "$1" in
	-x | --execute) EXECUTE=true ;;
	-p | --package)
		shift
		TARGET_CRATE="$1"
		;;
	--no-push) NO_PUSH=true ;;
	--retries)
		shift
		MAX_RETRIES="$1"
		;;
	--retry-delay)
		shift
		RETRY_DELAY="$1"
		;;
	*) ;;
	esac
	shift
done

if [[ -n "$TARGET_CRATE" ]]; then
	CRATES=("$TARGET_CRATE")
fi

declare -A REMOTE_VERSIONS
load_remote_versions() {
	local total=${#CRATES[@]}
	local count=0
	for crate in "${CRATES[@]}"; do
		((count++)) || true
		printf "\r  Querying %d/%d: %-40s" "$count" "$total" "$crate"
		local ver
		ver=$(cargo search "$crate" --limit 1 --registry crates-io 2>/dev/null |
			grep "^$crate = " |
			sed 's/.*= "\([^"]*\)".*/\1/' ||
			true)
		[[ -n "$ver" ]] && REMOTE_VERSIONS["$crate"]="$ver"
	done
	echo ""
}

# ── Get local version ──
get_local_version() {
	local crate="$1"
	grep -m1 "^version" "$ROOT_DIR/${crate/dubbo-rs/dubbo}/Cargo.toml" 2>/dev/null |
		sed 's/version = "\(.*\)"/\1/' ||
		true
}

# ── Semver comparison: returns 0 if $1 > $2 ──
version_gt() {
	local a="$1" b="$2"
	[[ "$a" == "$b" ]] && return 1
	local highest
	highest=$(printf '%s\n%s\n' "$a" "$b" | sort -V | tail -1)
	[[ "$highest" == "$a" ]]
}

# ── Collect crate info ──
echo -e "${BOLD}=== dubbo-rs release check ===${NC}"
echo ""
echo "Querying remote versions..."
load_remote_versions
echo ""

NEEDS_RELEASE=()
SKIP_COUNT=0
NOT_FOUND_COUNT=0

for crate in "${CRATES[@]}"; do
	local_ver=$(get_local_version "$crate")

	if [[ -z "$local_ver" ]]; then
		echo -e "  ${YELLOW}---${NC}  $crate  (not found in workspace)"
		((NOT_FOUND_COUNT++)) || true
		continue
	fi

	remote_ver="${REMOTE_VERSIONS[$crate]:-}"

	if [[ -z "$remote_ver" ]]; then
		echo -e "  ${CYAN}NEW${NC}   $crate ${CYAN}v$local_ver${NC}  (not yet published)"
		NEEDS_RELEASE+=("$crate")
	elif version_gt "$local_ver" "$remote_ver"; then
		echo -e "  ${YELLOW}UPD${NC}   $crate ${YELLOW}v$local_ver${NC}  (remote: v$remote_ver)"
		NEEDS_RELEASE+=("$crate")
	else
		echo -e "  ${GREEN} OK${NC}   $crate v$local_ver  (remote: v$remote_ver)"
		((SKIP_COUNT++)) || true
	fi
done

echo ""
TOTAL=${#CRATES[@]}
echo -e "  Total: $TOTAL | ${GREEN}Up to date: $SKIP_COUNT${NC} | ${CYAN}Needs release: ${#NEEDS_RELEASE[@]}${NC} | ${YELLOW}Not found: $NOT_FOUND_COUNT${NC}"

if [[ ${#NEEDS_RELEASE[@]} -eq 0 ]]; then
	echo ""
	echo -e "${GREEN}All crates are up to date.${NC}"
	exit 0
fi

echo ""
echo -e "${BOLD}Crates to release (in order):${NC}"
for crate in "${NEEDS_RELEASE[@]}"; do
	echo "  - $crate"
done

# ── Execute if requested ──
if [[ "$EXECUTE" != "true" ]]; then
	echo ""
	echo -e "${YELLOW}Dry-run mode. Add ${BOLD}-x${NC}${YELLOW} to execute.${NC}"
	exit 0
fi

echo ""
echo -e "${BOLD}=== Publishing (rate limits: existing=1/min, new=1/10min) ===${NC}"
echo ""
PUBLISHED=0
FAILED=0

for crate in "${NEEDS_RELEASE[@]}"; do
	local_ver=$(get_local_version "$crate")
	is_new=false
	[[ -z "${REMOTE_VERSIONS[$crate]:-}" ]] && is_new=true

	published=false
	for ((attempt = 1; attempt <= MAX_RETRIES; attempt++)); do
		echo -e "${CYAN}> cargo publish -p $crate --registry crates-io${NC} (attempt $attempt/$MAX_RETRIES)"
		if cargo publish -p "$crate" --registry crates-io; then
			echo -e "${GREEN}  OK${NC} $crate v$local_ver"
			((PUBLISHED++)) || true
			if [[ "$NO_PUSH" != "true" ]]; then
				git tag "${crate}-v${local_ver}" 2>/dev/null || true
				git push origin "${crate}-v${local_ver}" 2>/dev/null || true
			fi
			published=true
			break
		else
			if ((attempt < MAX_RETRIES)); then
				delay=$RETRY_DELAY
				if $is_new; then
					delay=600
				fi
				echo -e "${YELLOW}  RETRY${NC} $crate failed (attempt $attempt/$MAX_RETRIES), waiting ${delay}s..."
				for ((remaining = delay; remaining > 0; remaining--)); do
					printf "\r  ⏳ Retrying in %3ds..." "$remaining"
					sleep 1
				done
				echo -e "\r  ⏳ Retrying now...          "
			else
				echo -e "${RED}  FAIL${NC} $crate (all $MAX_RETRIES attempts exhausted)"
			fi
		fi
	done
	if [[ "$published" != "true" ]]; then
		((FAILED++)) || true
	fi
done

echo ""
echo -e "  Published: ${GREEN}$PUBLISHED${NC} | Failed: ${RED}$FAILED${NC}"
[[ $FAILED -eq 0 ]]
