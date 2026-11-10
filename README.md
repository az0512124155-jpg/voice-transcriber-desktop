# Voice Transcriber for Windows

The desktop companion to the Voice Transcriber Android app: press a shortcut,
speak, and the text is typed into whatever field has focus. Hebrew is
transcribed by the Hebrew model, English words inside a Hebrew sentence come
out in English, and a recording that is entirely in another language is
transcribed by the Whisper model that ships in the same installer. Everything
runs on your own computer — no audio ever leaves the machine.

This build is a fork of [Handy](https://github.com/cjpais/Handy) (MIT). The
interface, the shell and the shortcut handling are Handy's; the speech engine
and the visual identity are ours.

## What is different from upstream

| Area | Upstream Handy | This build |
| --- | --- | --- |
| Speech engine | transcribe.cpp (GGUF / ggml) | sherpa-onnx (ONNX) |
| Models | catalogue of ~73 downloadable models | two bundled models, already installed |
| Model management | download, delete, pin, rescan | none — the Models page is hidden |
| Language | ~100 languages, auto-detect | Hebrew + Whisper, routed automatically |
| Look | Handy pink | Midnight Ink, the phone app's palette |
| Updates | Handy's release channel | none; this build does not check for updates |

The name, the logo and every user-visible string now belong to Voice
Transcriber. The models page, the language preferences, the update checker and
the upstream donate/source links are gone, because none of them describe what
this build actually does.

## The engine swap

Handy runs every model in a child process, so a crash in native code cannot
take the window down. The parent talks to that worker over a framed JSON
protocol (`[u32 json_len][json][u32 pcm_len][pcm f32 LE]`) on stdin/stdout.

The fork keeps that boundary and only replaces what is behind it:

- `src-tauri/src/engine_supervisor/sherpa.rs` — a small runtime binding to the
  sherpa-onnx C API, loaded with `LoadLibraryW`/`GetProcAddress` so a missing
  DLL is a readable error rather than an app that refuses to start.
- `src-tauri/src/engine_supervisor/worker.rs` — serves the same `Request` /
  `Response` frames, now from sherpa-onnx instead of transcribe.cpp.

Streaming dictation is served by the same protocol: the worker folds each new
hypothesis into a committed prefix plus a tentative tail, so the text on screen
grows without flickering and the last word stays provisional until the next
frames agree on it.

Voice activity detection still uses transcribe.cpp, which is why that
dependency is still present.

## How a recording picks its language

Two recognizers live behind the same worker process:

- an online Zipformer transducer for Hebrew, which streams hypotheses while
  you speak, and
- an offline Whisper `base` model, which is the better transcriber for every
  other language.

**The Hebrew model owns every recording it can hear Hebrew in.** It is not
asked to justify itself and its text is never replaced. This is the whole
routing rule, and it exists because an earlier version of this fork did the
opposite: it fed the first 1.2 s of audio to Whisper as a language probe and
let the verdict decide. On real recordings that probe named Hebrew speech
Arabic, and the good Hebrew transcript was thrown away in favour of Arabic
gibberish — while the streaming preview was blanked for the rest of the
recording.

The signal that replaced it costs nothing and cannot be wrong: the Hebrew
model writes any English it hears in **Latin script, in place, inside the
Hebrew sentence**. So the script of its own output says which language the
recording was in. A Hebrew sentence that contains English words still has
Hebrew letters in it; a sentence in another language has none.

```
he_text contains at least one Hebrew letter  ->  Hebrew, kept as-is
he_text contains no Hebrew letter at all     ->  other language, ask Whisper
he_text is empty                             ->  nothing was said, keep it empty
```

That gives two modes, chosen in **Settings → General → Recording Languages**:

| Mode | Behaviour | Key-release latency |
| --- | --- | --- |
| Hebrew only | Whisper is never loaded | ~35 ms |
| Hebrew + other languages | Hebrew kept; a recording with no Hebrew goes to Whisper | ~35 ms Hebrew, ~2.4 s other |

There is no separate "mixed" mode because there is nothing to mix: a Hebrew
recording that contains `ctrl`, `space`, `mixer` or `sub-text` already comes
back with those words in Latin script, in the right place, at streaming speed.
Measured on a spliced Hebrew/English/Hebrew clip:

```
אתה מבין? אין עכשיו את זה. I need a pick up the kids from school at for, and
then we can talk about the true. עכשיו אתה יכול לזהות את הוורד שאני אומר לך?
תנו לך rules מאוד פשוטים.
```

The English inside a Hebrew sentence is the Hebrew model's own, so it is a
little rougher than Whisper would be — Whisper writes `from` and `four` where
the Hebrew model writes `rom` and `for`. That is the price of 38 ms instead of
two seconds, and of never touching the Hebrew.

A recording that is entirely in another language gets Whisper's version, which
is the more accurate of the two. Whisper's encoder always runs its fixed 30 s
window, so that costs about 2.4 s at key release however short the recording
is; the preview still fills in instantly with the Hebrew model's own Latin
transcript while you speak.

The Whisper model is ~80 MB and takes ~3 s to load, so it is loaded on a
background thread when the app starts, and not at all in Hebrew-only mode.

## Recordings on disk

Every dictation is written to a WAV file under the app data directory, named
`voice-transcriber-<timestamp>.wav`, and **Settings → Advanced → Auto-Delete
Recordings** decides when they go:

- a fixed window (3 days, 2 weeks, 3 months), or
- **a number of days you type** (1–3650), or
- **a size budget in KB / MB / GB / TB** — the oldest recordings are deleted
  until the folder fits.

The size budget is enforced after every new recording, so the folder never
grows past it.

## The shortcut

**Settings → General → Shortcut Behavior** defaults to *Toggle*: tap the
shortcut to start recording and tap it again to stop. Push-to-talk (hold) and
hold-or-toggle are still available.

## Building

Requirements: Node 22+, Rust (MSVC toolchain), CMake, and the WebView2 runtime
(present on Windows 11).

```bash
npm install
npm run tauri build          # NSIS installer in src-tauri/target/release/bundle
```

During development:

```bash
npm run dev                  # Vite on :1420
cd src-tauri && cargo build  # debug binary loads the dev server
```

### Building the Mac app

A `.dmg` can only be produced on a Mac, so the build runs on GitHub's macOS
runners: `.github/workflows/macos.yml`. Run it from the Actions tab (or push a
`v*` tag) and it uploads `VoiceTranscriber-macos-arm64` — a `.dmg` for Apple
Silicon.

The workflow does two things a local `tauri build` on macOS would not do for
you:

- It downloads `libsherpa-onnx-c-api.dylib` and `libonnxruntime.dylib` from
  the sherpa-onnx `v2.0.1` universal2 release into `src-tauri/resources/sherpa/`.
  These are 60 MB of binaries and are deliberately not in git. The two models
  are, so the bundle works the moment it is installed.
- It passes `bundle.macOS.hardenedRuntime: false` and signs ad-hoc. That is
  the combination that runs without a Developer ID certificate. With one, drop
  the flag and set `APPLE_SIGNING_IDENTITY` — nothing else changes.

Opening an unsigned build on another Mac shows *"Apple could not verify…"*.
Right-click the app → **Open** → **Open** clears it permanently for that
machine. The same class of warning as Windows SmartScreen, and the same fix:
a paid certificate.

### Assets that ship with the app

- `src-tauri/resources/models/` — the Hebrew streaming Zipformer
  (`encoder.int8.onnx`, `decoder.int8.onnx`, `joiner.int8.onnx`, `tokens.txt`),
  the Whisper base model (`base-encoder.int8.onnx`, `base-decoder.int8.onnx`,
  `base-tokens.txt`) and the Silero VAD model.
- `src-tauri/resources/sherpa/` — `sherpa-onnx-c-api.dll` and
  `onnxruntime.dll`.
- `src-tauri/resources/THIRD_PARTY_NOTICES.txt` — every third-party licence
  and attribution.

## Testing the engine

The binding has an ignored test that loads the real model and transcribes a
real clip:

```bash
cd src-tauri
VT_TEST_CLIP=/path/to/16k-mono.wav cargo test --lib sherpa -- --ignored --nocapture
VT_TEST_CLIP_EN=/path/to/english.wav cargo test --lib whisper_names -- --ignored --nocapture
```

The second one proves the Whisper path reports `en` and produces English text.

The whole application path can be exercised without a microphone:

```bash
handy.exe --transcribe-file /path/to/16k-mono.wav
handy.exe --list-models
```

## Licensing

The application code in this fork is proprietary. It is built on Handy, which
is MIT: that notice is retained in `THIRD_PARTY_NOTICES.txt` and in `LICENSE`.

The bundled Hebrew speech model is the ivrit.ai streaming Zipformer, licensed
CC BY 4.0, which permits commercial use provided the attribution is kept. The
second model is OpenAI's Whisper `base`, MIT. The engine is sherpa-onnx
(Apache-2.0) with ONNX Runtime (MIT). Full texts and the required attribution
statements are in `src-tauri/resources/THIRD_PARTY_NOTICES.txt`.

None of these licences reach the application's own source code, and none of
them grant rights over the audio a user dictates.
