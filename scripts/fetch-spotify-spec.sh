#!/usr/bin/env bash
# Fetches the Spotify Web API OpenAPI spec.
#
# Requirements: Java (for openapi-generator-cli)
#
# Usage: ./scripts/fetch-spotify-spec.sh
#
# NOTE: Spotify's official spec (developer.spotify.com/reference/web-api/open-api-schema.yaml)
# has long-standing schema errors that break code generators. We are using a modified version
# of the community-maintained spec from https://github.com/sonallux/spotify-web-api instead.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

SPEC_URL="https://raw.githubusercontent.com/sonallux/spotify-web-api/39207c4ba600deb34b0913975d01983ba60be583/fixed-spotify-open-api.yml"
SPEC_SHA256="ddeb078a50208d94c4538bd891bec0c9e67c9f3406617a2d887f0a482a4501f4"
OUTPUT="spotify-openapi.yaml"

echo "=== Fetching Spotify OpenAPI spec ==="
echo "  Source: ${SPEC_URL} "
curl -fSL -o "$OUTPUT" "$SPEC_URL"

ACTUAL_SHA256=$(sha256sum "$OUTPUT" | cut -d' ' -f1)
if [[ "$ACTUAL_SHA256" != "$SPEC_SHA256" ]]; then
    echo "" >&2
    echo "  ERROR: checksum mismatch for $OUTPUT" >&2
    echo "    expected: $SPEC_SHA256" >&2
    echo "    actual:   $ACTUAL_SHA256" >&2
    echo "" >&2
    rm -f "$OUTPUT"
    exit 1
else
    echo "  Checksum verified: $ACTUAL_SHA256"
fi

ENDPOINTS=$(grep -c "operationId:" "$OUTPUT" || true)
echo "  Downloaded: $OUTPUT ($ENDPOINTS endpoints)"

