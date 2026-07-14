#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)/tts-kokoro"
cd "$ROOT"
if ! command -v uv >/dev/null 2>&1; then
  echo "Install uv first: https://docs.astral.sh/uv/" >&2
  exit 1
fi
uv python install 3.12
uv venv -p 3.12
uv add kokoro-onnx soundfile numpy
mkdir -p models
cd models
if [[ ! -f kokoro-v1.0.onnx ]]; then
  curl -L -o kokoro-v1.0.onnx \
    "https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0/kokoro-v1.0.onnx"
fi
if [[ ! -f voices-v1.0.bin ]]; then
  curl -L -o voices-v1.0.bin \
    "https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0/voices-v1.0.bin"
fi
echo "Kokoro ready at $ROOT"
"$ROOT/.venv/bin/python" "$ROOT/speak.py" list-voices | head -c 200
echo
