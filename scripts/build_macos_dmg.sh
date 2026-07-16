#!/usr/bin/env bash
# Build Apple Silicon .app + .dmg (and updater archive when a signing key is available).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

bash ./scripts/build_eql_speak.sh

if [[ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" && -f .tauri-keys/eql-alerts.key ]]; then
  export TAURI_SIGNING_PRIVATE_KEY
  TAURI_SIGNING_PRIVATE_KEY="$(cat .tauri-keys/eql-alerts.key)"
fi

npx tauri build --bundles dmg,app

echo "DMG: $ROOT/src-tauri/target/release/bundle/dmg/"
ls -lh "$ROOT/src-tauri/target/release/bundle/dmg/"*.dmg
