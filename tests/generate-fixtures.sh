#!/bin/bash
# Generate Hessian2 binary fixture files from Java reference encoder.
# Output: tests/fixtures/hessian2/{primitives,strings,collections,maps,dates,objects}/*.bin
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
JAVA_DIR="$PROJECT_ROOT/tests/java-fixtures"
OUTPUT_DIR="$PROJECT_ROOT/tests/fixtures/hessian2"
TMP_DIR=$(mktemp -d)

log() { echo "[$(date '+%H:%M:%S')] $*"; }

trap 'rm -rf "$TMP_DIR"' EXIT

log "Building Java fixtures project..."
cd "$JAVA_DIR"
JAVA_HOME=/usr/lib/jvm/java-17-openjdk mvn compile -q 2>&1 || {
    log "Maven build failed"
    exit 1
}

log "Generating fixture files..."
# Run Java encoder with output to temp dir
JAVA_HOME=/usr/lib/jvm/java-17-openjdk mvn exec:java -q \
    -Dexec.mainClass="com.dubborst.fixtures.Hessian2ReferenceEncoder" \
    -Dexec.args="$TMP_DIR" 2>&1 || {
    log "Fixture generation failed"
    exit 1
}

# Copy generated files to output directory
log "Copying fixtures to $OUTPUT_DIR..."
mkdir -p "$OUTPUT_DIR"/{primitives,strings,collections,maps,dates,objects}
cp "$TMP_DIR"/bool_*.bin "$OUTPUT_DIR/primitives/" 2>/dev/null || true
cp "$TMP_DIR"/int_*.bin "$OUTPUT_DIR/primitives/" 2>/dev/null || true
cp "$TMP_DIR"/long_*.bin "$OUTPUT_DIR/primitives/" 2>/dev/null || true
cp "$TMP_DIR"/double_*.bin "$OUTPUT_DIR/primitives/" 2>/dev/null || true
cp "$TMP_DIR"/float_*.bin "$OUTPUT_DIR/primitives/" 2>/dev/null || true
cp "$TMP_DIR"/null.bin "$OUTPUT_DIR/primitives/" 2>/dev/null || true
cp "$TMP_DIR"/string_*.bin "$OUTPUT_DIR/strings/" 2>/dev/null || true
cp "$TMP_DIR"/list_*.bin "$OUTPUT_DIR/collections/" 2>/dev/null || true
cp "$TMP_DIR"/map_*.bin "$OUTPUT_DIR/maps/" 2>/dev/null || true
cp "$TMP_DIR"/date_*.bin "$OUTPUT_DIR/dates/" 2>/dev/null || true
cp "$TMP_DIR"/pojo_*.bin "$OUTPUT_DIR/objects/" 2>/dev/null || true

# Generate manifest with SHA-256 checksums
MANIFEST="$OUTPUT_DIR/manifest/manifest.csv"
log "Writing manifest to $MANIFEST..."
echo "filename,sha256,size_bytes" > "$MANIFEST"
find "$OUTPUT_DIR" -name '*.bin' -type f | sort | while read -r f; do
    rel="${f#$OUTPUT_DIR/}"
    sha=$(sha256sum "$f" | cut -d' ' -f1)
    size=$(stat -c%s "$f")
    echo "$rel,$sha,$size" >> "$MANIFEST"
done

# Count
COUNT=$(find "$OUTPUT_DIR" -name '*.bin' -type f | wc -l)
log "Generated $COUNT fixture files"
log "Done — fixtures in $OUTPUT_DIR"
