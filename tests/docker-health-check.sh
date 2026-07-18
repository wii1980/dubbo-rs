#!/bin/bash
# Health check for Docker services used in cross-language tests.
set -euo pipefail

TIMEOUT="${1:-60}"
INTERVAL=3
ELAPSED=0

log() { echo "[$(date '+%H:%M:%S')] $*"; }

check_zk() {
    if command -v nc &>/dev/null; then
        echo ruok | nc -w 2 localhost 2181 2>/dev/null | grep -q imok
    else
        # Fallback: check if port is open
        (echo >/dev/tcp/localhost/2181) 2>/dev/null
    fi
}

check_nacos() {
    curl -sf http://localhost:8848/nacos/actuator/health >/dev/null 2>&1
}

# Wait for both services
while [ $ELAPSED -lt $TIMEOUT ]; do
    zk_ok=false
    nacos_ok=false

    if check_zk; then
        zk_ok=true
    fi
    if check_nacos; then
        nacos_ok=true
    fi

    if $zk_ok && $nacos_ok; then
        log "All services healthy"
        exit 0
    fi

    if ! $zk_ok; then
        echo "  ⏳ ZooKeeper: waiting..."
    fi
    if ! $nacos_ok; then
        echo "  ⏳ Nacos: waiting..."
    fi

    sleep $INTERVAL
    ELAPSED=$((ELAPSED + INTERVAL))
done

# Timeout — report which ones failed
log "Timeout after ${TIMEOUT}s"
if ! check_zk; then
    echo "  ❌ ZooKeeper not healthy"
fi
if ! check_nacos; then
    echo "  ❌ Nacos not healthy"
fi
exit 1
