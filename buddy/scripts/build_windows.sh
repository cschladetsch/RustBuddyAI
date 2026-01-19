#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
LIB_DIR="${ROOT_DIR}/vosk/lib"
TARGET="x86_64-pc-windows-gnu"

if [[ ! -f "${LIB_DIR}/libvosk.dll.a" ]]; then
    echo "Missing ${LIB_DIR}/libvosk.dll.a. Run scripts/setup_vosk_windows.sh first." >&2
    exit 1
fi

export LIB="${LIB_DIR}:${LIB:-}"
cd "${ROOT_DIR}"

cargo run --release --target "${TARGET}" "$@"
