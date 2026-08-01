#!/usr/bin/env bash
# Build a relocatable Kokoro TTS pack for the macOS .app / .dmg.
# Output: src-tauri/resources/tts-kokoro.zip
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
STAGING="$ROOT/src-tauri/resources/tts-kokoro-staging"
ZIP_OUT="$ROOT/src-tauri/resources/tts-kokoro.zip"
MODEL_URL="https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0"

echo "Packing Kokoro TTS for macOS..."

if ! command -v uv >/dev/null 2>&1; then
  echo "Installing uv..."
  curl -LsSf https://astral.sh/uv/install.sh | sh
  export PATH="$HOME/.local/bin:$PATH"
fi
if ! command -v uv >/dev/null 2>&1; then
  echo "uv not found on PATH after install" >&2
  exit 1
fi

rm -rf "$STAGING"
mkdir -p "$STAGING/models" "$STAGING/python"

# Standalone CPython under the pack so the tree is relocatable after extract.
export UV_PYTHON_INSTALL_DIR="$STAGING/python"
uv python install 3.12

PYTHON="$(find "$STAGING/python" -type f -name 'python3.12' | head -n 1)"
if [[ -z "$PYTHON" || ! -x "$PYTHON" ]]; then
  echo "python3.12 not found under $STAGING/python" >&2
  exit 1
fi
echo "Using Python at $PYTHON"

# uv-managed CPython is marked EXTERNALLY-MANAGED; allow packing deps into it.
uv pip install --python "$PYTHON" --break-system-packages \
  kokoro-onnx soundfile numpy

echo "Verifying packaged imports..."
IMPORT_CHECK="$("$PYTHON" -c "import soundfile, numpy, kokoro_onnx, onnxruntime; print('imports-ok')")"
if [[ "$IMPORT_CHECK" != "imports-ok" ]]; then
  echo "import check failed (got: $IMPORT_CHECK)" >&2
  exit 1
fi

cp "$ROOT/tts-kokoro/speak.py" "$STAGING/speak.py"

ONNX="$STAGING/models/kokoro-v1.0.onnx"
VOICES="$STAGING/models/voices-v1.0.bin"
# Prefer already-downloaded models from the repo checkout when present.
if [[ ! -f "$ONNX" && -f "$ROOT/tts-kokoro/models/kokoro-v1.0.onnx" ]]; then
  cp "$ROOT/tts-kokoro/models/kokoro-v1.0.onnx" "$ONNX"
fi
if [[ ! -f "$VOICES" && -f "$ROOT/tts-kokoro/models/voices-v1.0.bin" ]]; then
  cp "$ROOT/tts-kokoro/models/voices-v1.0.bin" "$VOICES"
fi
if [[ ! -f "$ONNX" ]]; then
  echo "Downloading kokoro-v1.0.onnx..."
  curl -L --fail -o "$ONNX" "$MODEL_URL/kokoro-v1.0.onnx"
fi
if [[ ! -f "$VOICES" ]]; then
  echo "Downloading voices-v1.0.bin..."
  curl -L --fail -o "$VOICES" "$MODEL_URL/voices-v1.0.bin"
fi

# Relative path from pack root → python binary (for Rust helper_root).
REL="${PYTHON#"$STAGING"/}"
printf '%s' "$REL" > "$STAGING/python_relpath.txt"
printf '1' > "$STAGING/.deps-ok"

echo "Smoke-testing speak.py list-voices..."
VOICES_JSON="$("$PYTHON" "$STAGING/speak.py" list-voices)"
if [[ "$VOICES_JSON" != *bf_isabella* ]]; then
  echo "speak.py list-voices missing bf_isabella. Output: $VOICES_JSON" >&2
  exit 1
fi
echo "Smoke test ok."

mkdir -p "$(dirname "$ZIP_OUT")"
rm -f "$ZIP_OUT"

echo "Creating $ZIP_OUT ..."
(
  cd "$STAGING"
  # ditto makes a Finder-friendly zip; zip -r is fine for Rust's zip crate too.
  zip -rq "$ZIP_OUT" .
)

rm -rf "$STAGING"

SIZE_MB=$(( $(stat -f%z "$ZIP_OUT") / 1024 / 1024 ))
if [[ "$SIZE_MB" -lt 400 ]]; then
  echo "Kokoro zip looks too small (${SIZE_MB} MB) — packages may be missing" >&2
  exit 1
fi
echo "Kokoro pack ready: $ZIP_OUT (${SIZE_MB} MB)"
