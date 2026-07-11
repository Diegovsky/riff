#!/usr/bin/env bash
set -euo pipefail

# Lint Blueprint UI files for patterns that cause GTK runtime warnings.
#
# Checks:
# 1. All .blp files compile without errors.
# 2. GtkStack does not set visible-child-name as a property (it fires before
#    children are added, causing "Child name 'X' not found in GtkStack").
# 3. Children of GtkBox/GtkBoxLayout do not use Grid layout properties
#    (causes "GtkBoxLayout does not create GtkLayoutChild instances").
#
# Usage: ./scripts/lint-blueprints.sh [path-to-search]

SEARCH_DIR="${1:-$(cd "$(dirname "$0")/.." && pwd)/src}"
FAILED=0

if ! command -v blueprint-compiler &>/dev/null; then
    echo "ERROR: blueprint-compiler not found. Install it to run this check."
    exit 1
fi

if ! command -v xmllint &>/dev/null; then
    echo "ERROR: xmllint not found. Install libxml2-utils (Debian) or libxml2 (Fedora/Arch)."
    exit 1
fi

BLP_FILES=$(find "$SEARCH_DIR" -type f -name "*.blp")

if [[ -z "$BLP_FILES" ]]; then
    echo "No .blp files found in $SEARCH_DIR"
    exit 0
fi

echo "--- Blueprint compilation ---"
for blp in $BLP_FILES; do
    if ! blueprint-compiler compile "$blp" > /dev/null 2>&1; then
        echo "FAIL: $blp does not compile"
        blueprint-compiler compile "$blp" 2>&1 || true
        FAILED=1
    fi
done
if [[ "$FAILED" -eq 0 ]]; then
    echo "PASS: all .blp files compile"
fi

echo ""
echo "--- GtkStack visible-child-name check ---"
# In compiled UI XML, a <property name="visible-child-name"> on a GtkStack
# is set before children are added, triggering runtime warnings.
for blp in $BLP_FILES; do
    XML=$(blueprint-compiler compile "$blp" 2>/dev/null) || continue

    # Find GtkStack objects that set visible-child-name as a direct property.
    # We use xmllint with xpath to find these.
    MATCHES=$(echo "$XML" | xmllint --xpath \
        '//object[@class="GtkStack"]/property[@name="visible-child-name"]' - 2>/dev/null) || MATCHES=""

    if [[ -n "$MATCHES" ]]; then
        echo "FAIL: $blp"
        echo "  GtkStack sets visible-child-name as a property (will warn at runtime)."
        echo "  Remove visible-child-name from the Stack; the first child is shown by default."
        echo "  Use a Breakpoint setter or code to switch pages after construction."
        FAILED=1
    fi
done
if [[ "$FAILED" -eq 0 ]]; then
    echo "PASS: no GtkStack visible-child-name property issues"
fi

echo ""
echo "--- Box child layout property check ---"
# A <layout> block inside a child of GtkBox can only use Box layout properties.
# Grid properties (column, row, column-span, row-span) inside a Box child cause
# "GtkBoxLayout does not create GtkLayoutChild instances".

for blp in $BLP_FILES; do
    XML=$(blueprint-compiler compile "$blp" 2>/dev/null) || continue

    # Use a Python one-liner to walk the XML tree and find layout blocks
    # under children of GtkBox that contain Grid properties.
    ISSUES=$(python3 -c "
import sys
import xml.etree.ElementTree as ET

xml_input = sys.stdin.read()
tree = ET.fromstring(xml_input)
issues = []

def check_box_children(box_elem, path=''):
    \"\"\"Check direct children of a GtkBox for invalid layout properties.\"\"\"
    for child in box_elem.findall('child'):
        obj = child.find('object')
        if obj is None:
            continue
        obj_id = obj.get('id', obj.get('class', 'unknown'))
        layout = obj.find('layout')
        if layout is not None:
            for prop in layout.findall('property'):
                name = prop.get('name', '')
                if name in ('column', 'row', 'column-span', 'row-span'):
                    issues.append(f'  {obj_id}: layout property \"{name}\" invalid inside Box')

def walk(elem):
    if elem.tag == 'object' and elem.get('class') in ('GtkBox',):
        check_box_children(elem)
    # Also check template if it extends GtkBox
    if elem.tag == 'template' and elem.get('parent') == 'GtkBox':
        check_box_children(elem)
    for child_elem in elem:
        walk(child_elem)

walk(tree)
for issue in issues:
    print(issue)
" <<< "$XML" 2>/dev/null) || ISSUES=""

    if [[ -n "$ISSUES" ]]; then
        echo "FAIL: $blp"
        echo "$ISSUES"
        FAILED=1
    fi
done
if [[ "$FAILED" -eq 0 ]]; then
    echo "PASS: no invalid layout properties in Box children"
fi

echo ""
if [[ "$FAILED" -ne 0 ]]; then
    echo "Blueprint lint checks FAILED."
    exit 1
else
    echo "All blueprint lint checks passed."
fi
