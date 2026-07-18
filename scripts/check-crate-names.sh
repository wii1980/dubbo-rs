#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# check-crate-names.sh
# Check whether workspace crate names are already taken on crates.io.
#
# Usage:
#   ./scripts/check-crate-names.sh          # check all workspace crates
#   ./scripts/check-crate-names.sh --json   # machine-readable JSON output
#
# Exit codes:
#   0  - all names are available
#   1  - one or more names are already taken
#   2  - usage / dependency error
# ---------------------------------------------------------------------------

set -euo pipefail

# ── colours ────────────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
RESET='\033[0m'

# ── config ─────────────────────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CARGO_TOML="$PROJECT_ROOT/Cargo.toml"
API="https://crates.io/api/v1/crates"
# crates.io asks for ≤1 req/s; add a small safety margin
DELAY=1.1
JSON_OUTPUT=false

# ── args ───────────────────────────────────────────────────────────────────
for arg in "$@"; do
    case "$arg" in
        --json)    JSON_OUTPUT=true ;;
        -h|--help)
            echo "Usage: $0 [--json]"
            echo "  --json   Output results as JSON"
            exit 0
            ;;
        *)
            echo "Unknown argument: $arg" >&2
            exit 2
            ;;
    esac
done

# ── deps ───────────────────────────────────────────────────────────────────
if ! command -v curl &>/dev/null; then
    echo -e "${RED}Error: curl is required but not found.${RESET}" >&2
    exit 2
fi

# ── helpers ────────────────────────────────────────────────────────────────
# Extract the `name` field from a Cargo.toml [package] section.
# Falls back to the directory name if not set.
crate_name() {
    local cargo_toml="$1"
    # grab the first `name = "..."` that appears (should be in [package])
    local name
    name="$(grep -m1 '^[[:space:]]*name[[:space:]]*=' "$cargo_toml" \
            | sed 's/^[[:space:]]*name[[:space:]]*=[[:space:]]*"\(.*\)"/\1/')"
    if [[ -z "$name" ]]; then
        name="$(basename "$(dirname "$cargo_toml")")"
    fi
    echo "$name"
}

# ── collect crate names ────────────────────────────────────────────────────
# Parse workspace members (handles both quoted and unquoted forms, and
# multiline arrays).
members="$(sed -n '/^members/,/\]/p' "$CARGO_TOML" \
    | grep -oP '"[^"]+"' \
    | tr -d '"')"

if [[ -z "$members" ]]; then
    echo -e "${RED}Error: no workspace members found in $CARGO_TOML${RESET}" >&2
    exit 2
fi

# Build the list of (name, Cargo.toml path) pairs, skipping examples & test crates.
# Only check crates starting with "dubbo-rs"
declare -a CRATE_NAMES=()
for rel_path in $members; do
    # skip example / cross-lang-test crates — they won't be published
    case "$rel_path" in
        examples/*|cross-lang-tests) continue ;;
    esac

    ct="$PROJECT_ROOT/$rel_path/Cargo.toml"
    if [[ -f "$ct" ]]; then
        name="$(crate_name "$ct")"
        # only check crates starting with "dubbo-rs"
        if [[ "$name" == dubbo-rs* ]]; then
            CRATE_NAMES+=("$name")
        fi
    else
        echo -e "${YELLOW}Warning: $ct not found, skipping${RESET}" >&2
    fi
done

if [[ ${#CRATE_NAMES[@]} -eq 0 ]]; then
    echo -e "${RED}Error: no crates to check${RESET}" >&2
    exit 2
fi

# ── query crates.io ────────────────────────────────────────────────────────
taken=0
available=0

# JSON accumulator
json_results="["
first=true

echo -e "${BOLD}Checking ${#CRATE_NAMES[@]} crate names on crates.io ...${RESET}"
echo ""

for name in "${CRATE_NAMES[@]}"; do
    http_code="$(curl --silent --write-out '%{http_code}' --output /dev/null \
                     -H 'User-Agent: dubbo-rs-check-crate-names (github.com/apache/dubbo-rs)' \
                     "$API/$name")"

    if [[ "$http_code" == "200" ]]; then
        if [[ "$JSON_OUTPUT" == false ]]; then
            echo -e "  ${RED}✗ TAKEN${RESET}  $name"
        fi
        taken=$((taken + 1))
        status="taken"
    elif [[ "$http_code" == "404" ]]; then
        if [[ "$JSON_OUTPUT" == false ]]; then
            echo -e "  ${GREEN}✓ FREE${RESET}   $name"
        fi
        available=$((available + 1))
        status="available"
    else
        if [[ "$JSON_OUTPUT" == false ]]; then
            echo -e "  ${YELLOW}? ERROR${RESET}  $name (HTTP $http_code)"
        fi
        status="error"
    fi

    # build JSON array
    if [[ "$first" == true ]]; then
        first=false
    else
        json_results+=","
    fi
    json_results+="{\"name\":\"$name\",\"status\":\"$status\",\"http_code\":$http_code}"

    # respect rate limit
    sleep "$DELAY"
done

json_results+="]"

# ── summary ────────────────────────────────────────────────────────────────
if [[ "$JSON_OUTPUT" == true ]]; then
    echo "$json_results" | python3 -m json.tool 2>/dev/null || echo "$json_results"
else
    echo ""
    echo -e "${BOLD}Summary:${RESET}  ${GREEN}${available} available${RESET}, ${RED}${taken} taken${RESET}  (total ${#CRATE_NAMES[@]})"
fi

if [[ "$taken" -gt 0 ]]; then
    exit 1
fi
exit 0
