#!/usr/bin/env bash
set -euo pipefail

dotnet build ../library/UserDirectory.csproj --nologo --verbosity quiet
dotnet run --project UserDirectoryHarness.csproj --nologo
