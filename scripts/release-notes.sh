#!/bin/sh
# Prints the CHANGELOG.md section of a version (without its heading),
# used as the description of the GitHub release: release-notes.sh 0.1.10
version="$1"
awk -v version="$version" '
    /^## \[/ { if (printing) exit; printing = index($0, "## [" version "]") == 1; next }
    /^\[[^]]+\]: / { next }
    printing { print }
' "$(dirname "$0")/../CHANGELOG.md"
