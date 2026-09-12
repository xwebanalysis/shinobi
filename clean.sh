#!/usr/bin/env bash
# clean.sh — removes build artifacts and runtime data for Shinobi.
#
#   ./clean.sh            # Rust target, static UI, DB, downloads, Python caches/venv
#   ./clean.sh --all      # also removes frontend/node_modules
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

readonly YLW='\033[1;33m'
readonly NC='\033[0m'
warn() { echo -e "${YLW}[clean]${NC} $1"; }

rm -rf target
rm -rf static
rm -rf frontend/.angular
rm -f shinobi.db shinobi.db-wal shinobi.db-shm
rm -rf downloads
rm -f /tmp/shinobi.db /tmp/shinobi.db-wal /tmp/shinobi.db-shm 2>/dev/null || true
find extractor -type d -name __pycache__ -prune -exec rm -rf {} + 2>/dev/null || true
rm -rf extractor/.venv extractor/venv extractor/.pytest_cache
rm -rf .pytest_cache

if [ "${1:-}" = "--all" ]; then
    warn "Removing frontend/node_modules (--all)"
    rm -rf frontend/node_modules
fi

warn "Clean complete"
