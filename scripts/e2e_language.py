"""End-to-end check of the fork's language routing, over the real worker
protocol: spawn the shipped binary as --transcribe-worker, feed it a clip the
way the microphone does, and read back the final transcript and language.

    python e2e_language.py <language_mode> <clip> [<clip> ...]
"""
import json
import struct
import subprocess
import sys
import time
import wave
import array

EXE = r"C:/Users/az051/.gemini/antigravity/scratch/voice-transcriber-handy/src-tauri/target/debug/handy.exe"
MODEL = r"C:/Users/az051/AppData/Roaming/il.voicetranscriber.desktop/models/encoder.int8.onnx"


def frame(obj, pcm=b""):
    body = json.dumps(obj).encode()
    return struct.pack("<I", len(body)) + body + struct.pack("<I", len(pcm)) + pcm


def read(p):
    n = struct.unpack("<I", p.stdout.read(4))[0]
    msg = json.loads(p.stdout.read(n))
    m = struct.unpack("<I", p.stdout.read(4))[0]
    if m:
        p.stdout.read(m)
    return msg


def pcm_of(path):
    w = wave.open(path, "rb")
    samples = array.array("h", w.readframes(w.getnframes()))
    if w.getnchannels() > 1:
        ch = w.getnchannels()
        samples = array.array("h", [
            sum(samples[i:i + ch]) // ch for i in range(0, len(samples), ch)
        ])
    return array.array("f", [s / 32768.0 for s in samples]).tobytes()


mode = sys.argv[1]
clips = sys.argv[2:]

p = subprocess.Popen(
    [EXE, "--transcribe-worker"],
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.DEVNULL,
)

p.stdin.write(frame({"Hello": {"list_devices": False}}))
p.stdin.flush()
read(p)

t0 = time.time()
p.stdin.write(frame({
    "Load": {
        "path": MODEL,
        "backend": "Auto",
        "device": "Auto",
        "language_mode": mode,
    }
}))
p.stdin.flush()
loaded = read(p)
print("loaded in %.0f ms: %s" % ((time.time() - t0) * 1000, json.dumps(loaded)[:90]))
if "Loaded" not in loaded:
    sys.exit("load failed: %s" % loaded)

for clip in clips:
    p.stdin.write(frame({"StreamBegin": {
        "run": {}, "stream": {}, "language_mode": mode}}))
    p.stdin.flush()
    read(p)

    pcm = pcm_of(clip)
    preview = None
    started = time.time()
    for i in range(0, len(pcm) // 4, 1600):        # 100 ms chunks, as the mic delivers
        chunk = pcm[i * 4:(i + 1600) * 4]
        p.stdin.write(frame("Feed", chunk))
        p.stdin.flush()
        msg = read(p)["Fed"]
        if msg.get("text"):
            preview = msg["text"]

    fed = time.time() - started
    t_fin = time.time()
    p.stdin.write(frame({"Finalize": {"want_language": True}}))
    p.stdin.flush()
    fin = read(p)["Finalized"]
    fin_ms = (time.time() - t_fin) * 1000
    elapsed = time.time() - started

    audio_s = (len(pcm) // 4) / 16000
    print("--- %-14s %4.2fs audio, %4.0f ms feeding, %4.0f ms from key release to text"
          % (clip.rsplit("/", 1)[-1], audio_s, fed * 1000, fin_ms))
    print("    preview : %r" % ((preview or {}).get("committed")))
    print("    language: %r" % fin.get("language"))
    print("    final   : %r" % fin["text"]["full"])

p.stdin.close()
p.wait(timeout=15)
print("worker exit:", p.returncode)
