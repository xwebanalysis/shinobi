#!/usr/bin/env bash
# shinobi — local-first launcher.
#
#   ./shinobi.sh                 # local: Rust backend :8060 + Python extractor :9090
#   ./shinobi.sh local           # same as above
#   ./shinobi.sh --build-frontend  # also build the Angular UI (npm) before starting
#   ./shinobi.sh docker          # docker compose up --build
#
# Legacy aliases kept: -f/--fast (backend only), -d/--deep (backend+extractor),
# -p/--python-only, -b/--build, -D/--docker.
set -euo pipefail

readonly GRN='\033[0;32m'
readonly BLU='\033[0;34m'
readonly YLW='\033[1;33m'
readonly RED='\033[0;31m'
readonly CYN='\033[0;36m'
readonly NC='\033[0m'

log()  { echo -e "${GRN}[shinobi]${NC} $1"; }
info() { echo -e "${BLU}[info]${NC} $1"; }
warn() { echo -e "${YLW}[warn]${NC} $1"; }
err()  { echo -e "${RED}[err]${NC} $1"; }

# Prefer the mise-managed Node 24 LTS for Angular tooling (system Node may be unsupported)
if [ -d "$HOME/.local/share/mise/installs/node/24/bin" ]; then
    case ":$PATH:" in
        *":$HOME/.local/share/mise/installs/node/24/bin:"*) ;;
        *) export PATH="$HOME/.local/share/mise/installs/node/24/bin:$PATH" ;;
    esac
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

PORT="${PORT:-8060}"
EXTRACTOR_PORT="${EXTRACTOR_PORT:-9090}"
export PORT
export SHINOBI_DB_PATH="${SHINOBI_DB_PATH:-$ROOT/shinobi.db}"
export DATA_DIR="${DATA_DIR:-$ROOT/downloads}"
export EXTRACTOR_URL="${EXTRACTOR_URL:-http://localhost:$EXTRACTOR_PORT}"
export RUST_LOG="${RUST_LOG:-shinobi=info,tower_http=info}"

UV="${UV:-$HOME/.local/bin/uv}"
EXTRACTOR_VENV="$ROOT/extractor/.venv"
EXTRACTOR_PY="$EXTRACTOR_VENV/bin/python"

RUST_PID=""
PY_PID=""

cleanup() {
    echo ""
    warn "Shutting down..."
    [ -n "$PY_PID" ] && kill "$PY_PID" 2>/dev/null || true
    [ -n "$RUST_PID" ] && kill "$RUST_PID" 2>/dev/null || true
    wait 2>/dev/null || true
    info "All processes stopped"
    exit 0
}
trap cleanup SIGINT SIGTERM

show_help() {
    cat <<EOF
${CYN}shinobi — launch control${NC}

Usage: ./shinobi.sh [command] [options]

Commands:
  local           SQLite nativo (DEFAULT): backend :$PORT + extractor :$EXTRACTOR_PORT
  docker          Levanta todo con docker compose

Options:
  --build-frontend   Construye la UI Angular antes de arrancar (usa npm)
  -f, --fast         Solo backend Rust (sin extractor)
  -d, --deep         Backend Rust + extractor Python (igual que local)
  -p, --python-only  Solo extractor Python (desarrollo)
  -b, --build        Alias legacy de --build-frontend
  -D, --docker       Alias legacy de 'docker'
  -h, --help         Muestra esta ayuda

Environment:
  PORT=$PORT  SHINOBI_DB_PATH=$SHINOBI_DB_PATH
  DATA_DIR=$DATA_DIR  EXTRACTOR_PORT=$EXTRACTOR_PORT

Examples:
  ./shinobi.sh                     # local, API + extractor, SQLite en el repo
  ./shinobi.sh --build-frontend    # además compila la UI Angular
  ./shinobi.sh docker              # todo vía compose (volumen shinobi-data)
  ./shinobi.sh --deep              # alias legacy
EOF
    exit 0
}

MODE="local"        # local | docker | python
FAST_ONLY=false
BUILD_FRONTEND=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        local)              MODE="local"; shift ;;
        docker)             MODE="docker"; shift ;;
        -f|--fast|--rust-only) MODE="local"; FAST_ONLY=true; shift ;;
        -d|--deep)          MODE="local"; FAST_ONLY=false; shift ;;
        -p|--python-only)   MODE="python"; shift ;;
        -b|--build|--build-frontend) BUILD_FRONTEND=true; shift ;;
        -D|--docker)        MODE="docker"; shift ;;
        -h|--help)          show_help ;;
        *) err "Unknown option: $1"; show_help ;;
    esac
done

if [ "$MODE" = "docker" ]; then
    if ! command -v docker >/dev/null 2>&1; then
        err "docker no encontrado"
        exit 1
    fi
    if [ ! -f docker-compose.yml ]; then
        err "docker-compose.yml not found"
        exit 1
    fi
    log "Launching via docker-compose (http://localhost:$PORT)..."
    exec docker compose up --build
fi

check_rust() {
    if ! command -v cargo >/dev/null 2>&1; then
        err "Rust (cargo) no encontrado. Instala: https://rustup.rs"
        exit 1
    fi
}

setup_python() {
    if [ ! -x "$EXTRACTOR_PY" ]; then
        log "Creando venv del extractor (Python 3.13)..."
        if [ -x "$UV" ]; then
            "$UV" venv --python 3.13 --seed "$EXTRACTOR_VENV"
        else
            python3 -m venv "$EXTRACTOR_VENV"
        fi
        info "Instalando dependencias del extractor..."
        "$EXTRACTOR_PY" -m pip install -q -r extractor/requirements.txt \
            || warn "pip install falló; el extractor se degradará a modo rule-based"
    fi
    if ! command -v httrack >/dev/null 2>&1; then
        warn "httrack no instalado: el modo /crawl del extractor fallará (instálalo con tu gestor de paquetes)"
    fi
}

build_frontend() {
    if ! command -v node >/dev/null 2>&1; then
        err "Node.js no encontrado; no se puede construir la UI"
        exit 1
    fi
    log "Construyendo UI Angular (SHINOBI_BUILD_FRONTEND=1)..."
    SHINOBI_BUILD_FRONTEND=1 cargo build --release
}

start_extractor() {
    setup_python
    log "Arrancando extractor Python en :$EXTRACTOR_PORT..."
    ( cd "$ROOT/extractor" && exec "$EXTRACTOR_PY" main.py "$EXTRACTOR_PORT" ) &
    PY_PID=$!
    sleep 1
    if kill -0 "$PY_PID" 2>/dev/null; then
        info "Extractor running (PID: $PY_PID) — http://localhost:$EXTRACTOR_PORT"
    else
        err "El extractor no arrancó; revisa los logs"
        exit 1
    fi
}

start_backend() {
    check_rust
    if [ "$BUILD_FRONTEND" = true ]; then
        build_frontend
    fi
    log "Arrancando backend Rust en :$PORT (DB: $SHINOBI_DB_PATH)..."
    cargo run --release &
    RUST_PID=$!
    for _ in $(seq 1 60); do
        if curl -fsS "http://localhost:$PORT/api/health" >/dev/null 2>&1; then
            info "Backend running (PID: $RUST_PID) — http://localhost:$PORT"
            return 0
        fi
        if ! kill -0 "$RUST_PID" 2>/dev/null; then
            err "El backend terminó antes de responder. Revisa los logs."
            exit 1
        fi
        sleep 1
    done
    err "Timeout esperando /api/health"
    exit 1
}

if [ "$MODE" = "python" ]; then
    start_extractor
    wait "$PY_PID"
    exit 0
fi

start_extractor
start_backend

echo ""
info "───────────────────────────────────────"
info " Shinobi is running"
info " API:       http://localhost:$PORT"
info " Extractor: http://localhost:$EXTRACTOR_PORT"
info " DB:        $SHINOBI_DB_PATH"
info " Data:      $DATA_DIR"
info " Press Ctrl+C to stop all services"
info "───────────────────────────────────────"
echo ""

wait
