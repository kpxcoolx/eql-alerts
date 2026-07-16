#!/usr/bin/env bash
# Compile the macOS AVFoundation helper used for chimes / AIFF playback.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="$ROOT/src-tauri/binaries/eql-speak.swift"
OUT="$ROOT/src-tauri/binaries/eql-speak"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "build_eql_speak.sh is macOS-only" >&2
  exit 1
fi

swiftc -O -o "$OUT" "$SRC" -framework AVFoundation -framework AppKit
echo "built $OUT"
