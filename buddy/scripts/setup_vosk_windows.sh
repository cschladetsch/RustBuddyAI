#!/usr/bin/env bash
set -euo pipefail

VOSK_VERSION="0.3.45"
VOSK_URL="https://github.com/alphacep/vosk-api/releases/download/v${VOSK_VERSION}/vosk-win64-${VOSK_VERSION}.zip"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
LIB_DIR="${ROOT_DIR}/vosk/lib"
TMP_DIR="$(mktemp -d)"
ZIP_PATH="${TMP_DIR}/vosk-win64.zip"

cleanup() {
    rm -rf "${TMP_DIR}"
}
trap cleanup EXIT

mkdir -p "${LIB_DIR}"

if ! command -v curl >/dev/null 2>&1; then
    echo "curl is required to download Vosk" >&2
    exit 1
fi
if ! command -v unzip >/dev/null 2>&1; then
    echo "unzip is required to extract Vosk" >&2
    exit 1
fi

echo "Downloading Vosk Windows package ${VOSK_VERSION}..."
curl -L -o "${ZIP_PATH}" "${VOSK_URL}"

echo "Extracting..."
unzip -q "${ZIP_PATH}" -d "${TMP_DIR}"
EXTRACTED_DIR="${TMP_DIR}/vosk-win64-${VOSK_VERSION}"

for file in libvosk.dll libvosk.dll.a; do
    if [[ ! -f "${EXTRACTED_DIR}/lib/${file}" ]]; then
        echo "Expected ${file} in ${EXTRACTED_DIR}/lib but it was not found." >&2
        exit 1
    fi
    cp "${EXTRACTED_DIR}/lib/${file}" "${LIB_DIR}/"
    echo "Copied ${file} -> ${LIB_DIR}"
done

echo "Vosk Windows libraries staged in ${LIB_DIR}."
