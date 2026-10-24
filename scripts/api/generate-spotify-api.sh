#!/usr/bin/env bash
# Generates the Rust Spotify API client from the checked-in OpenAPI spec.
#
# Requirements: Java (for openapi-generator-cli)
#
# Usage: ./scripts/api/generate-spotify-api.sh

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT"

SPEC_FILE="spotify-openapi.yaml"
GENERATOR_VERSION="7.12.0"
GENERATOR_JAR="${OPENAPI_GENERATOR_JAR:-/tmp/openapi-generator-cli-${GENERATOR_VERSION}.jar}"
OUTPUT_DIR="generated/spotify-api"

if [[ ! -f "$SPEC_FILE" ]]; then
    echo "Error: $SPEC_FILE not found in repository root." >&2
    echo "Run ./scripts/api/fetch-spotify-spec.sh to download the spec first." >&2
    exit 1
fi

echo "=== Downloading OpenAPI Generator (if needed) ==="
if [[ ! -f "$GENERATOR_JAR" ]]; then
    if [[ -n "${OPENAPI_GENERATOR_JAR:-}" ]]; then
        echo "Error: OPENAPI_GENERATOR_JAR points to a missing file: $GENERATOR_JAR" >&2
        exit 1
    fi
    curl -fSL -o "$GENERATOR_JAR" \
        "https://repo1.maven.org/maven2/org/openapitools/openapi-generator-cli/${GENERATOR_VERSION}/openapi-generator-cli-${GENERATOR_VERSION}.jar"
fi
echo "  Using: $GENERATOR_JAR"

echo ""
echo "=== Generating Rust client ==="
rm -rf "$OUTPUT_DIR"
java -jar "$GENERATOR_JAR" generate \
    -i "$SPEC_FILE" \
    -g rust \
    -o "$OUTPUT_DIR" \
    --package-name spotify-api \
    --additional-properties=library=reqwest,supportMiddleware=true \
    --type-mappings=file=String

# Clean up unnecessary generated files
rm -f "$OUTPUT_DIR/git_push.sh" "$OUTPUT_DIR/.travis.yml" "$OUTPUT_DIR/.gitignore"

echo ""
echo "=== Done ==="
echo "  Generated crate: $OUTPUT_DIR"
echo "  Verify: cd $OUTPUT_DIR && cargo check"
