#!/usr/bin/env bash
set -euo pipefail

dotnet build UserDirectoryHarness.csproj --nologo --verbosity quiet -p:UseSharedCompilation=false -p:BuildInParallel=false -m:1 -nodeReuse:false
dotnet run --project UserDirectoryHarness.csproj --no-build --no-restore --nologo
