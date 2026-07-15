# Build a relocatable Kokoro TTS pack for the Windows NSIS installer.
# Output: src-tauri/resources/tts-kokoro.zip
$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true

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
  Where-Object { $_.FullName -notmatch '\\Scripts\\' } |
  Select-Object -First 1
if (-not $Python) {
  throw "python.exe not found under $($Staging)\python"
}

Write-Host "Using Python at $($Python.FullName)"

# uv-managed CPython is marked EXTERNALLY-MANAGED; allow packing deps into it.
uv pip install --python $Python.FullName --break-system-packages `
  kokoro-onnx soundfile numpy
if ($LASTEXITCODE -ne 0) {
  throw "uv pip install failed with exit code $LASTEXITCODE"
}

Write-Host "Verifying packaged imports..."
$importCheck = & $Python.FullName -c "import soundfile, numpy, kokoro_onnx, onnxruntime; print('imports-ok')"
if ($LASTEXITCODE -ne 0) {
  throw "import check failed with exit code $LASTEXITCODE"
}
if ("$importCheck" -notmatch "imports-ok") {
  throw "import check did not print imports-ok (got: $importCheck)"
}

$Site = Join-Path $Python.DirectoryName "Lib\site-packages"
$KokoroDir = Join-Path $Site "kokoro_onnx"
if (-not (Test-Path $KokoroDir)) {
  throw "kokoro_onnx missing from site-packages at $Site"
}
Write-Host "site-packages ok at $Site"

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
# Marker so the app can reject incomplete packs from older broken builds.
Set-Content -Path (Join-Path $Staging ".deps-ok") -Value "1" -NoNewline

Write-Host "Smoke-testing speak.py list-voices..."
$voices = & $Python.FullName (Join-Path $Staging "speak.py") list-voices
if ($LASTEXITCODE -ne 0) {
  throw "speak.py list-voices failed with exit code $LASTEXITCODE"
}
if ("$voices" -notmatch "bf_isabella") {
  throw "speak.py list-voices missing bf_isabella. Output: $voices"
}
Write-Host "Smoke test ok."

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
if ($SizeMb -lt 400) {
  throw "Kokoro zip looks too small ($SizeMb MB) — packages may be missing"
}
Write-Host "Kokoro pack ready: $ZipOut ($SizeMb MB)"
