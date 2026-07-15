# Build a relocatable Kokoro TTS pack for the Windows NSIS installer.
# Output: src-tauri/resources/tts-kokoro.zip
$ErrorActionPreference = "Stop"

$Root = Split-Path $PSScriptRoot -Parent
$Staging = Join-Path $Root "src-tauri\resources\tts-kokoro-staging"
$ZipOut = Join-Path $Root "src-tauri\resources\tts-kokoro.zip"
$ModelUrl = "https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0"

Write-Host "Packing Kokoro TTS for Windows..."

if (-not (Get-Command uv -ErrorAction SilentlyContinue)) {
  Write-Host "Installing uv..."
  irm https://astral.sh/uv/install.ps1 | iex
  $env:Path = "$env:USERPROFILE\.local\bin;$env:USERPROFILE\.cargo\bin;$env:Path"
}
if (-not (Get-Command uv -ErrorAction SilentlyContinue)) {
  throw "uv not found on PATH after install"
}

if (Test-Path $Staging) {
  Remove-Item -Recurse -Force $Staging
}
New-Item -ItemType Directory -Path (Join-Path $Staging "models") | Out-Null
New-Item -ItemType Directory -Path (Join-Path $Staging "python") | Out-Null

# Standalone CPython under the pack so the tree is relocatable after install.
$env:UV_PYTHON_INSTALL_DIR = Join-Path $Staging "python"
uv python install 3.12

$Python = Get-ChildItem -Path (Join-Path $Staging "python") -Recurse -Filter python.exe |
  Select-Object -First 1
if (-not $Python) {
  throw "python.exe not found under $($Staging)\python"
}

Write-Host "Using Python at $($Python.FullName)"
uv pip install --python $Python.FullName kokoro-onnx soundfile numpy

Copy-Item (Join-Path $Root "tts-kokoro\speak.py") (Join-Path $Staging "speak.py")

$Onnx = Join-Path $Staging "models\kokoro-v1.0.onnx"
$Voices = Join-Path $Staging "models\voices-v1.0.bin"
if (-not (Test-Path $Onnx)) {
  Write-Host "Downloading kokoro-v1.0.onnx..."
  curl.exe -L --fail -o $Onnx "$ModelUrl/kokoro-v1.0.onnx"
}
if (-not (Test-Path $Voices)) {
  Write-Host "Downloading voices-v1.0.bin..."
  curl.exe -L --fail -o $Voices "$ModelUrl/voices-v1.0.bin"
}

# Relative path from pack root → python.exe (for Rust helper_root).
$Rel = $Python.FullName.Substring($Staging.Length).TrimStart('\', '/')
Set-Content -Path (Join-Path $Staging "python_relpath.txt") -Value $Rel -NoNewline

Write-Host "Smoke-testing speak.py list-voices..."
& $Python.FullName (Join-Path $Staging "speak.py") list-voices | Select-Object -First 1

$ResDir = Split-Path $ZipOut -Parent
if (-not (Test-Path $ResDir)) {
  New-Item -ItemType Directory -Path $ResDir | Out-Null
}
if (Test-Path $ZipOut) {
  Remove-Item -Force $ZipOut
}

Write-Host "Creating $ZipOut ..."
Push-Location $Staging
try {
  tar.exe -a -c -f $ZipOut *
} finally {
  Pop-Location
}

if (-not (Test-Path $ZipOut)) {
  throw "Failed to create $ZipOut"
}

Remove-Item -Recurse -Force $Staging
$SizeMb = [math]::Round((Get-Item $ZipOut).Length / 1MB, 1)
Write-Host "Kokoro pack ready: $ZipOut ($SizeMb MB)"
