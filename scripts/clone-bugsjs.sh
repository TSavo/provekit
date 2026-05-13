#!/usr/bin/env bash
# scripts/clone-bugsjs.sh: shallow-clone the 10 BugsJS project forks.
#
# BugsJS publishes per-project forks of mature Node.js projects with
# `Bug-N-{original,fix,test,full}` tags. The harvest pipeline reads these
# tag pairs directly to build HarvestCandidates from real diffs.
#
# Idempotent: if a fork already exists at $BUGSJS_DIR/<project>, the script
# fetches new tags but does not re-clone. The shallow-clone uses
# --no-single-branch + --depth=1 so every Bug-* tag is reachable without
# pulling full project history (gigabytes saved).
#
# Usage:
#   bash scripts/clone-bugsjs.sh                 # clone into ~/bugsjs/
#   BUGSJS_DIR=/path/to/bugsjs bash scripts/clone-bugsjs.sh

set -euo pipefail

PROJECTS=(
  express
  mocha
  eslint
  karma
  bower
  hexo
  hessian.js
  node-redis
  pencilblue
  shields
)

BUGSJS_DIR="${BUGSJS_DIR:-$HOME/bugsjs}"

mkdir -p "$BUGSJS_DIR"

cloned=0
fetched=0
failed=0

for project in "${PROJECTS[@]}"; do
  dir="$BUGSJS_DIR/$project"
  url="https://github.com/BugsJS/$project.git"

  if [ -d "$dir/.git" ]; then
    echo "[fetch] $project: $dir"
    if git -C "$dir" fetch --tags --depth=1 origin 2>&1 | sed 's/^/  /'; then
      fetched=$((fetched + 1))
    else
      echo "  WARN: fetch failed for $project (continuing)"
      failed=$((failed + 1))
    fi
  else
    echo "[clone] $project: $url -> $dir"
    if git clone --depth=1 --no-single-branch "$url" "$dir" 2>&1 | sed 's/^/  /'; then
      cloned=$((cloned + 1))
    else
      echo "  WARN: clone failed for $project (continuing)"
      failed=$((failed + 1))
    fi
  fi
done

echo
echo "Done. cloned=$cloned fetched=$fetched failed=$failed"
echo "BUGSJS_DIR=$BUGSJS_DIR"
echo
echo "Tag counts (Bug-*):"
for project in "${PROJECTS[@]}"; do
  dir="$BUGSJS_DIR/$project"
  if [ -d "$dir/.git" ]; then
    count=$(git -C "$dir" tag -l "Bug-*" 2>/dev/null | wc -l | tr -d ' ')
    printf "  %-15s %6s\n" "$project" "$count"
  fi
done
