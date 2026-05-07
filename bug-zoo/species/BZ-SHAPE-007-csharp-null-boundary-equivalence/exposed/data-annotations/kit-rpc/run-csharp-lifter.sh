#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SPECIMEN_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
REPO_ROOT="$(cd "$SPECIMEN_ROOT/../../.." && pwd)"
PROJECT="$REPO_ROOT/implementations/csharp/Provekit.BugZoo/Provekit.BugZoo.csproj"
dotnet build "$PROJECT" --nologo --verbosity quiet >/dev/null
exec dotnet run --project "$PROJECT" --no-build --no-restore -- lifter csharp-data-annotations
