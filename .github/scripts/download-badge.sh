#!/usr/bin/env bash
# Print a shields.io endpoint JSON with the total download count of every
# release's .dmg (the .sha256 files are left out).
#   GH_REPO=owner/repo .github/scripts/download-badge.sh > downloads.json
set -euo pipefail
: "${GH_REPO:?set GH_REPO=owner/repo}"

count=$(gh api "repos/$GH_REPO/releases" --paginate \
  --jq '.[].assets[] | select(.name | endswith(".dmg")) | .download_count' |
  awk '{ s += $1 } END { print s + 0 }')

# 950 → 950, 1234 → 1.2k, 1500000 → 1.5M
message=$(awk -v n="$count" 'BEGIN {
  if (n >= 1000000) s = sprintf("%.1fM", n / 1000000)
  else if (n >= 1000) s = sprintf("%.1fk", n / 1000)
  else s = n
  sub(/\.0/, "", s); print s
}')

printf '{"schemaVersion":1,"label":"downloads","message":"%s","color":"blue"}\n' "$message"
