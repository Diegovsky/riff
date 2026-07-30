#!/usr/bin/env bash

projdir="$(dirname $(dirname $(realpath $0)))"
cd "$projdir"

potfiles="po/POTFILES"
if [[ ! -f "$potfiles" ]]; then
    echo "$projdir/$potfiles not found."
    exit 1
fi

(
    echo '# rust code'
    rg -l gettext -g '*.rs'
    echo
    echo '# blueprint files'
    find src -name "*.blp" -print
) | tee "$potfiles"
