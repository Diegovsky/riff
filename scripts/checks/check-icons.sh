#!/usr/bin/env bash
set -euo pipefail

# Check that every symbolic icon referenced in the source tree is bundled in
# riff.gresource.xml

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
SRC_DIR="${1:-$ROOT_DIR/src}"
DATA_DIR="$(dirname "$SRC_DIR")/data"
GRESOURCE="$SRC_DIR/riff.gresource.xml"
ACTIONS_DIR="$DATA_DIR/hicolor/symbolic/actions"
FAILED=0

SOURCES=(--include='*.rs' --include='*.blp' --include='*.ui' --include='*.css')

fail() {
    echo "FAIL: $*"
    FAILED=1
}

show_references() {
    grep -rnF "$1" "$SRC_DIR" "${SOURCES[@]}" | sed 's/^/    /' || true
}

find_library() {
    local override="$1" soname="$2" lib="$1"
    if [[ -z "$lib" ]] && command -v ldconfig >/dev/null; then
        lib=$(ldconfig -p | awk -v so="$soname" '$1 == so { print $NF; exit }')
    fi
    if [[ -z "$lib" || ! -f "$lib" ]]; then
        echo "ERROR: $soname not found${override:+ at $override}, cannot tell which icons it ships" >&2
        exit 1
    fi
    echo "$lib"
}

# Icon names under a resource prefix the library registers as an icon path
builtin_icons() {
    gresource list "$1" | sed -nE "s|^$2/(.*/)?([^/]+)\.(svg\|png)$|\2|p" | sort -u
}

if [[ ! -f "$GRESOURCE" ]]; then
    echo "ERROR: $GRESOURCE not found"
    exit 1
fi
if ! command -v gresource >/dev/null; then
    echo "ERROR: the gresource tool is not installed (it ships with GLib's development files)"
    exit 1
fi

gtk_lib=$(find_library "${GTK_LIB:-}" libgtk-4.so.1)
adw_lib=$(find_library "${ADW_LIB:-}" libadwaita-1.so.0)

# --- What the source references ---

used=$(grep -rhoE "[\"'][A-Za-z0-9._-]+-symbolic[\"']" "$SRC_DIR" "${SOURCES[@]}" \
    | tr -d "\"'" | sort -u)

used_other=$(grep -rhoE "(icon-name|application-icon|icon_name)[:(][[:space:]]*(Some\()?\"[A-Za-z0-9._-]+\"" \
        "$SRC_DIR" "${SOURCES[@]}" \
    | grep -oE "\"[^\"]+\"" | tr -d '"' | grep -v -- '-symbolic$' | sort -u || true)

all_used=$(printf '%s\n' "$used" "$used_other" | sed '/^$/d' | sort -u)

# --- What is available ---

builtin=$(printf '%s\n' \
        "$(builtin_icons "$gtk_lib" /org/gtk/libgtk/icons)" \
        "$(builtin_icons "$adw_lib" /org/gnome/Adwaita/icons)" \
    | sed '/^$/d' | sort -u)

bundled=$(grep -oE 'alias="[A-Za-z0-9._-]+-symbolic\.svg"' "$GRESOURCE" \
    | sed -E 's/^alias="(.*)\.svg"$/\1/' | sort -u)

app_icons=$(find "$DATA_DIR/hicolor" -path '*/apps/*' -name '*.svg' -exec basename {} .svg \; \
    | sort -u)

needed=$(comm -23 <(echo "$all_used") <(echo "$builtin"))

echo "--- Referenced icons that are not bundled ---"
while read -r icon; do
    [[ -z "$icon" ]] && continue
    fail "$icon"
    show_references "\"$icon\""
done < <(comm -23 <(echo "$needed") <(printf '%s\n' "$bundled" "$app_icons" | sort -u))

echo "--- Icon names built at runtime (cannot be checked) ---"
while read -r line; do
    [[ -z "$line" ]] && continue
    fail "partial icon name, spell it out in full so it can be checked"
    echo "    $line"
done < <(grep -rnoE "[\"'][^\"']*symbolic[^\"']*[\"']" "$SRC_DIR" "${SOURCES[@]}" \
    | grep -vE ":[\"'][A-Za-z0-9._-]+-symbolic[\"']$" || true)

echo "--- Bundled icons that GTK or libadwaita already ship ---"
while read -r icon; do
    [[ -z "$icon" ]] && continue
    fail "$icon is built into GTK or libadwaita, remove it from riff.gresource.xml"
done < <(comm -12 <(echo "$bundled") <(echo "$builtin"))

echo "--- Bundled icons that are never referenced ---"
while read -r icon; do
    [[ -z "$icon" ]] && continue
    fail "$icon is in riff.gresource.xml but not used"
done < <(comm -13 <(echo "$used") <(echo "$bundled"))

echo "--- Bundled icon files ---"
while IFS='|' read -r alias path; do
    [[ -z "$alias" ]] && continue
    if [[ ! -f "$SRC_DIR/$path" ]]; then
        fail "$alias points at $path, which does not exist"
    elif [[ "$(basename "$path")" != "$alias" ]]; then
        fail "$alias points at $(basename "$path"), a differently named file"
    fi
done < <(grep -E '<file [^>]*alias="[^"]+-symbolic\.svg"' "$GRESOURCE" \
    | sed -E 's/.*alias="([^"]+)">([^<]+)<\/file>.*/\1|\2/')

if [[ -d "$ACTIONS_DIR" ]]; then
    while read -r file; do
        [[ -z "$file" ]] && continue
        fail "$file is in ${ACTIONS_DIR#"$ROOT_DIR"/} but not in riff.gresource.xml"
    done < <(comm -23 \
        <(find "$ACTIONS_DIR" -name '*-symbolic.svg' -exec basename {} .svg \; | sort -u) \
        <(echo "$bundled"))
fi

if [[ "$FAILED" -eq 1 ]]; then
    echo
    echo "Icon check failed."
    exit 1
fi

echo "All $(echo "$all_used" | wc -l) referenced icons are available:" \
    "$(echo "$needed" | wc -l) bundled with Riff," \
    "$(comm -12 <(echo "$all_used") <(echo "$builtin") | wc -l) built into GTK or libadwaita."
