#!/bin/sh
# Prints the CHANGELOG.md entry for a version (without its heading), e.g.
#   scripts/changelog-section.sh 0.3.0 [CHANGELOG.md]
# Prints nothing when the version has no entry. Used for GitHub release notes.
awk -v v="$1" '
  /^## / {
    if (found) exit
    if (index($0, "## [" v "]") == 1) { found = 1; next }
  }
  found { print }
' "${2:-CHANGELOG.md}" | sed -e '/./,$!d'
