"""Phonon TTS as a small local server, shaped like `pocket-tts serve`.

POST /tts with form fields `text` and `voice_url` (a Phonon voice name, e.g. Easton). The reply is a
streamed 24 kHz mono 16-bit WAV, so any client that reads Pocket-TTS output can read it.
GET /health says it is up.

Phonon (`ptts`) is a uv tool with its own environment, so run this with that environment's Python:

    uv tool run --from ptts python resources/phonon/phonon_server.py

Settings (all optional): PHONON_HOST, PHONON_PORT, PHONON_MODEL, PHONON_QUANT, PHONON_THREADS,
PHONON_VOICE, PHONON_GAIN (volume, default 2.5).
"""
import os
os.environ.setdefault("OMP_NUM_THREADS", "1")
os.environ.setdefault("OPENBLAS_NUM_THREADS", "1")
os.environ.setdefault("MKL_NUM_THREADS", "1")
import struct
import threading
import time
import urllib.parse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import numpy as np
import ptts

HOST = os.environ.get("PHONON_HOST", "127.0.0.1")
PORT = int(os.environ.get("PHONON_PORT", "8001"))
MODEL = os.environ.get("PHONON_MODEL", "hf://gradium/phonon-31m-en")
QUANT = os.environ.get("PHONON_QUANT", "q8_0")
THREADS = int(os.environ.get("PHONON_THREADS", "2"))
DEFAULT_VOICE = os.environ.get("PHONON_VOICE", "Freya")
GAIN = float(os.environ.get("PHONON_GAIN", "2.5"))  # Phonon is quiet (peak 0.45); the soft limit below keeps louder peaks from clipping

_tts = None
_lock = threading.Lock()  # one synthesis at a time on this model


def _wav_header(sample_rate: int) -> bytes:
    """A WAV header for a stream of unknown length (sizes are the maximum)."""
    return (b"RIFF" + struct.pack("<I", 0xFFFFFFFF) + b"WAVEfmt " +
            struct.pack("<IHHIIHH", 16, 1, 1, sample_rate, sample_rate * 2, 2, 16) +
            b"data" + struct.pack("<I", 0xFFFFFFFF))


def _to_int16(chunk: np.ndarray) -> bytes:
    return (np.tanh(chunk * GAIN) * 32767.0).astype(np.int16).tobytes()


class Handler(BaseHTTPRequestHandler):
    def log_message(self, fmt, *args):
        pass

    def do_GET(self):
        body = b"ok" if self.path == "/health" else b"phonon-tts"
        self.send_response(200)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):
        if self.path != "/tts":
            self.send_error(404)
            return
        raw = self.rfile.read(int(self.headers.get("Content-Length", 0))).decode("utf-8", "replace")
        form = urllib.parse.parse_qs(raw)
        text = (form.get("text") or [""])[0].strip()
        voice = (form.get("voice_url") or [""])[0].strip() or DEFAULT_VOICE
        if not text:
            self.send_error(400, "text is empty")
            return
        t0 = time.perf_counter()
        t_first = None
        samples = 0
        with _lock:
            try:
                stream = _tts.stream(text, voice=voice)
            except Exception as ex:
                print(f"[phonon] voice/stream error for '{voice}': {ex}")
                self.send_error(400, str(ex)[:100])
                return
            self.send_response(200)
            self.send_header("Content-Type", "audio/wav")
            self.end_headers()
            try:
                self.wfile.write(_wav_header(_tts.sample_rate))
                with stream as audio:
                    for chunk in audio:
                        if t_first is None:
                            t_first = time.perf_counter() - t0
                        samples += len(chunk)
                        self.wfile.write(_to_int16(chunk))
                        self.wfile.flush()
            except (BrokenPipeError, ConnectionResetError):
                pass
        total = time.perf_counter() - t0
        secs = samples / _tts.sample_rate
        rtf = total / secs if secs else 0.0
        print(f"[phonon] {len(text)} chars, first chunk {(t_first or 0) * 1000:.0f} ms, "
              f"{secs:.2f} s of audio in {total:.2f} s (RTF {rtf:.2f})", flush=True)


def main():
    global _tts
    ptts.set_num_threads(THREADS)
    t0 = time.perf_counter()
    _tts = ptts.TTS(MODEL, lang="en", quant=QUANT, voice=DEFAULT_VOICE)
    print(f"[phonon] loaded {MODEL} ({_tts.quant}, {_tts.device}, {THREADS} threads) "
          f"in {time.perf_counter() - t0:.1f} s", flush=True)
    _tts.synth("Warm up.", voice=DEFAULT_VOICE)
    print(f"[phonon] listening on http://{HOST}:{PORT}/tts", flush=True)
    ThreadingHTTPServer((HOST, PORT), Handler).serve_forever()


if __name__ == "__main__":
    main()
