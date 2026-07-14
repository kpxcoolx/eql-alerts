#!/usr/bin/env python3
"""Local Kokoro TTS helper for EQL Alerts (macOS + Windows).

Commands:
  list-voices                 → JSON voice catalog on stdout
  speak --voice ID --text …   → write WAV to --out (or temp path printed)
  serve                       → long-lived HTTP server (keeps model loaded)
"""

from __future__ import annotations

import argparse
import json
import sys
import tempfile
import threading
import traceback
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlparse

import soundfile as sf

ROOT = Path(__file__).resolve().parent
MODEL = ROOT / "models" / "kokoro-v1.0.onnx"
VOICES_BIN = ROOT / "models" / "voices-v1.0.bin"

# Curated English voices for the picker (id, label, gender, locale).
VOICE_META = [
    ("af_heart", "Heart", "female", "en-US"),
    ("af_bella", "Bella", "female", "en-US"),
    ("af_nicole", "Nicole", "female", "en-US"),
    ("af_sarah", "Sarah", "female", "en-US"),
    ("af_kore", "Kore", "female", "en-US"),
    ("af_aoede", "Aoede", "female", "en-US"),
    ("af_nova", "Nova", "female", "en-US"),
    ("af_alloy", "Alloy", "female", "en-US"),
    ("bf_emma", "Emma (UK)", "female", "en-GB"),
    ("bf_isabella", "Isabella (UK)", "female", "en-GB"),
    ("am_michael", "Michael", "male", "en-US"),
    ("am_fenrir", "Fenrir", "male", "en-US"),
    ("am_puck", "Puck", "male", "en-US"),
    ("am_echo", "Echo", "male", "en-US"),
    ("am_liam", "Liam", "male", "en-US"),
    ("am_onyx", "Onyx", "male", "en-US"),
    ("bm_george", "George (UK)", "male", "en-GB"),
    ("bm_fable", "Fable (UK)", "male", "en-GB"),
]

_kokoro = None
_lock = threading.Lock()


def ensure_models() -> None:
    if not MODEL.exists() or not VOICES_BIN.exists():
        raise SystemExit(
            f"Kokoro models missing under {ROOT / 'models'}. "
            "Run scripts/setup_kokoro.sh (or .bat) first."
        )


def get_kokoro():
    global _kokoro
    with _lock:
        if _kokoro is None:
            from kokoro_onnx import Kokoro

            ensure_models()
            _kokoro = Kokoro(str(MODEL), str(VOICES_BIN))
        return _kokoro


def lang_for_voice(voice: str) -> str:
    if voice.startswith(("bf_", "bm_")):
        return "en-gb"
    return "en-us"


def synthesize(text: str, voice: str, speed: float, out: Path) -> Path:
    text = text.strip()
    if not text:
        raise ValueError("empty text")
    k = get_kokoro()
    samples, sample_rate = k.create(
        text,
        voice=voice,
        speed=speed,
        lang=lang_for_voice(voice),
    )
    # Match system alert sounds: 44.1k stereo PCM.
    # On macOS also write AIFF — same container as Ping (WAV stays silent for some users).
    try:
        import numpy as np

        arr = np.asarray(samples, dtype=np.float64)
        if arr.ndim == 1:
            mono = arr
        else:
            mono = arr.mean(axis=1)
        peak = float(np.max(np.abs(mono))) if mono.size else 0.0
        if peak > 1e-6:
            mono = mono * (0.95 / peak)
        target_rate = 44100
        if sample_rate != target_rate and mono.size > 1:
            x = np.linspace(0.0, 1.0, num=mono.shape[0], endpoint=False)
            n = max(1, int(round(mono.shape[0] * target_rate / float(sample_rate))))
            xi = np.linspace(0.0, 1.0, num=n, endpoint=False)
            mono = np.interp(xi, x, mono)
            sample_rate = target_rate
        samples = np.stack([mono, mono], axis=1)
    except Exception:
        pass
    out.parent.mkdir(parents=True, exist_ok=True)
    if sys.platform == "darwin":
        aiff = out.with_suffix(".aiff")
        sf.write(str(aiff), samples, sample_rate, format="AIFF", subtype="PCM_16")
        return aiff
    sf.write(str(out), samples, sample_rate, subtype="PCM_16")
    return out


def list_voices_json() -> str:
    ensure_models()
    available = set(get_kokoro().get_voices())
    out = []
    for vid, label, gender, locale in VOICE_META:
        if vid in available:
            out.append(
                {
                    "id": vid,
                    "label": label,
                    "gender": gender,
                    "locale": locale,
                }
            )
    # Include any other English voices from the model not in our curated list.
    known = {v["id"] for v in out}
    for vid in sorted(available):
        if vid in known:
            continue
        if not (vid.startswith("af_") or vid.startswith("am_") or vid.startswith("bf_") or vid.startswith("bm_")):
            continue
        gender = "female" if vid[1] == "f" else "male"
        locale = "en-GB" if vid.startswith(("bf_", "bm_")) else "en-US"
        out.append(
            {
                "id": vid,
                "label": vid,
                "gender": gender,
                "locale": locale,
            }
        )
    return json.dumps(out)


class Handler(BaseHTTPRequestHandler):
    def log_message(self, fmt: str, *args) -> None:
        sys.stderr.write("eql-kokoro: " + (fmt % args) + "\n")

    def _send(self, code: int, body: bytes, content_type: str = "application/json") -> None:
        self.send_response(code)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Connection", "close")
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self) -> None:
        parsed = urlparse(self.path)
        if parsed.path == "/voices":
            try:
                body = list_voices_json().encode("utf-8")
                self._send(200, body)
            except Exception as exc:
                self._send(500, json.dumps({"error": str(exc)}).encode("utf-8"))
            return
        if parsed.path == "/health":
            self._send(200, b'{"ok":true}')
            return
        if parsed.path == "/speak":
            qs = parse_qs(parsed.query)
            text = (qs.get("text") or ["Alert test"])[0]
            voice = (qs.get("voice") or ["bf_isabella"])[0]
            speed = float((qs.get("speed") or ["1.05"])[0])
            try:
                suffix = ".aiff" if sys.platform == "darwin" else ".wav"
                out = (
                    Path(tempfile.gettempdir())
                    / f"eql-kokoro-{abs(hash((voice, text, speed)))}{suffix}"
                )
                path = synthesize(text, voice, speed, out)
                payload = json.dumps(
                    {"ok": True, "path": str(path), "voice": voice}
                ).encode("utf-8")
                self._send(200, payload)
            except Exception as exc:
                traceback.print_exc()
                self._send(500, json.dumps({"error": str(exc)}).encode("utf-8"))
            return
        self._send(404, b'{"error":"not found"}')


def cmd_serve(host: str, port: int) -> None:
    # Warm model so first alert is fast.
    get_kokoro()
    server = ThreadingHTTPServer((host, port), Handler)
    sys.stderr.write(f"eql-kokoro listening on http://{host}:{port}\n")
    server.serve_forever()


def main() -> None:
    parser = argparse.ArgumentParser(description="EQL Alerts Kokoro TTS helper")
    sub = parser.add_subparsers(dest="cmd", required=True)

    sub.add_parser("list-voices")

    p_speak = sub.add_parser("speak")
    p_speak.add_argument("--text", required=True)
    p_speak.add_argument("--voice", default="bf_isabella")
    p_speak.add_argument("--speed", type=float, default=1.05)
    p_speak.add_argument("--out", default="")

    p_serve = sub.add_parser("serve")
    p_serve.add_argument("--host", default="127.0.0.1")
    p_serve.add_argument("--port", type=int, default=17423)

    args = parser.parse_args()
    if args.cmd == "list-voices":
        print(list_voices_json())
        return
    if args.cmd == "speak":
        out = Path(args.out) if args.out else Path(tempfile.mkstemp(suffix=".wav", prefix="eql-kokoro-")[1])
        path = synthesize(args.text, args.voice, args.speed, out)
        print(path)
        return
    if args.cmd == "serve":
        cmd_serve(args.host, args.port)


if __name__ == "__main__":
    main()
