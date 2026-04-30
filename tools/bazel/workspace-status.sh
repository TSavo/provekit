#!/bin/sh
# Bazel workspace-status script: emits stable + volatile key/value
# pairs that bazel stamps into outputs (when --stamp is set).
#
# Stable keys persist across builds with the same source tree;
# volatile keys (like build timestamps) change every build.

# Stable: the git commit. Affects derived artifacts but not action keys.
echo "STABLE_GIT_COMMIT $(git rev-parse HEAD 2>/dev/null || echo unknown)"
echo "STABLE_GIT_BRANCH $(git branch --show-current 2>/dev/null || echo unknown)"
echo "STABLE_GIT_DIRTY  $(test -n "$(git status --porcelain 2>/dev/null)" && echo dirty || echo clean)"

# Volatile: build timestamp.
echo "BUILD_TIMESTAMP $(date -u +%s)"
