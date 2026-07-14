@echo off
setlocal
set ROOT=%~dp0..\tts-kokoro
cd /d "%ROOT%"
where uv >nul 2>&1
if errorlevel 1 (
  echo Install uv first: https://docs.astral.sh/uv/
  exit /b 1
)
uv python install 3.12
uv venv -p 3.12
uv add kokoro-onnx soundfile numpy
if not exist models mkdir models
cd models
if not exist kokoro-v1.0.onnx (
  curl -L -o kokoro-v1.0.onnx https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0/kokoro-v1.0.onnx
)
if not exist voices-v1.0.bin (
  curl -L -o voices-v1.0.bin https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0/voices-v1.0.bin
)
echo Kokoro ready at %ROOT%
"%ROOT%\.venv\Scripts\python.exe" "%ROOT%\speak.py" list-voices
