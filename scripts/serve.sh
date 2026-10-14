#!/usr/bin/env bash
# Serve web/ on http://localhost:${PORT:-8080}. wasm needs a real HTTP server (file:// will not work).
set -euo pipefail
cd "$(dirname "$0")/../web"
port="${PORT:-8080}"
echo "NtBA at http://localhost:${port}"
exec python3 -m http.server "$port" --bind 127.0.0.1
