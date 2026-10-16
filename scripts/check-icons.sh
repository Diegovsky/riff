#!/usr/bin/env bash
set -euo pipefail

# Check that every symbolic icon referenced in the source tree is bundled in
# riff.gresource.xml

SRC_DIR="${1:-$(cd "$(dirname "$0")/.." && pwd)/src}"
GRESOURCE="$SRC_DIR/riff.gresource.xml"
FAILED=0

GTK_PROVIDED=(
    changes-allow-symbolic
    changes-prevent-symbolic
    dialog-error-symbolic
    dialog-warning-symbolic
    folder-publicshare-symbolic
    go-previous-symbolic
    list-add-symbolic
    media-playback-pause-symbolic
    media-playback-start-symbolic
    media-playlist-repeat-symbolic
    open-menu-symbolic
    system-search-symbolic
    user-trash-symbolic
    view-grid-symbolic
    view-list-symbolic
    view-more-symbolic
    zoom-in-symbolic
    zoom-out-symbolic
)

if [[ ! -f "$GRESOURCE" ]]; then
    echo "ERROR: $GRESOURCE not found"
    exit 1
fi

used=$(grep -rhoE "[\"'][a-z0-9-]+-symbolic[\"']" "$SRC_DIR" \
        --include='*.rs' --include='*.blp' --include='*.ui' --include='*.css' \
    | tr -d "\"'" | sort -u)

bundled=$(grep -oE 'alias="[a-z0-9-]+-symbolic\.svg"' "$GRESOURCE" \
    | sed -E 's/^alias="(.*)\.svg"$/\1/' | sort -u)

provided=$(printf '%s\n' "${GTK_PROVIDED[@]}" "$bundled" | sort -u)

echo "--- Referenced icons that are not bundled ---"
missing=$(comm -23 <(echo "$used") <(echo "$provided"))
if [[ -n "$missing" ]]; then
    while read -r icon; do
        echo "FAIL: $icon"
        grep -rnE "[\"']${icon}[\"']" "$SRC_DIR" \
            --include='*.rs' --include='*.blp' --include='*.ui' --include='*.css' \
            | sed 's/^/    /'
    done <<< "$missing"
    FAILED=1
fi

echo "--- Bundled icons that are never referenced ---"
unused=$(comm -13 <(echo "$used") <(echo "$bundled"))
if [[ -n "$unused" ]]; then
    while read -r icon; do
        echo "FAIL: $icon is in riff.gresource.xml but not used"
    done <<< "$unused"
    FAILED=1
fi

if [[ "$FAILED" -eq 1 ]]; then
    echo
    echo "Icon check failed."
    exit 1
fi

echo "All $(echo "$used" | wc -l) referenced icons are available."
