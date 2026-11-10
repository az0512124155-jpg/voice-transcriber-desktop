//! The worker process: the only place native speech code runs. Owns the
//! recognizer, its stream, and serves [`Request`]s until the parent closes
//! stdin.
//!
//! A dedicated thread reads stdin so the parent's writes never block, and
//! the worker exits the moment the parent goes away, even if hung.

use super::protocol::{
    read_message, write_message, DeviceSelector, LoadedInfo, Request, Response,
};
use super::sherpa::{Engine, Heard, OfflineEngine};
use super::LOG_LEVEL_ENV;
use crate::settings::LanguageMode;
use log::{debug, error, LevelFilter, Log, Metadata, Record};
use std::fs::File;
use std::io::{self, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use transcribe_cpp::{
    Backend, Capabilities, RunOptions, Segment, StreamText, StreamUpdate, TimestampKind, Timings,
    Transcript,
};

/// Prefix on worker log lines so the parent can tell them apart from raw
/// native output (e.g. an abort message right before a crash).
pub(super) const LOG_LINE_PREFIX: &str = "\u{1}";

/// How much audio one `Run` chunk holds. 3200 samples is 200 ms, the
/// granularity sherpa-onnx's own examples feed a streaming model.
const RUN_CHUNK: usize = 3200;

/// A request as handed from the stdin reader to the main loop.
struct Incoming {
    request: Request,
    pcm: Vec<f32>,
}

pub fn run() -> i32 {
    // The worker owns the per-run engine scratch, so it needs the same glibc
    // tuning as the app (#1792). Before anything allocates.
    crate::memory::init_allocator();
    #[cfg(unix)]
    ignore_app_signals();
    #[cfg(target_os = "linux")]
    set_process_name();
    // Take the real stdout for the protocol before any native code runs, and
    // point fd 1 at stderr: native code writes to stdout in places, and any
    // stray byte there would corrupt the protocol stream.
    let protocol_out = match take_stdout_for_protocol() {
        Ok(file) => file,
        Err(e) => {
            eprintln!("transcribe worker: failed to claim stdout: {e}");
            return 2;
        }
    };
    init_logger();

    let (requests_tx, requests) = mpsc::channel();
    if let Err(e) = thread::Builder::new()
        .name("transcribe-worker-in".into())
        .spawn(move || read_requests(requests_tx))
    {
        error!("Failed to start the request reader: {}", e);
        return 2;
    }

    let mut output = protocol_out;
    let mut session: Option<Session> = None;

    while let Ok(Incoming { request, pcm }) = requests.recv() {
        let finished_run = matches!(request, Request::Run { .. });
        let response = match request {
            Request::Hello { list_devices } => {
                // The fork has exactly one compute device (the CPU) and no
                // GPU path, so there is nothing to enumerate.
                Response::Hello {
                    devices: list_devices.then(Vec::new),
                }
            }
            Request::Load {
                path,
                backend,
                device,
                language_mode,
            } => {
                // Free any previous model first to avoid holding two at once.
                session = None;
                match load(&path, backend, device, language_mode) {
                    Ok(loaded) => {
                        let info = loaded.1.clone();
                        session = Some(loaded.0);
                        Response::Loaded(info)
                    }
                    Err(e) => Response::Error(e),
                }
            }
            Request::Run {
                options,
                language_mode,
            } => match session.as_mut() {
                Some(loaded) => match run_once(loaded, &pcm, language_mode, &options) {
                    Ok(transcript) => Response::Transcript(transcript),
                    Err(e) => Response::Error(e),
                },
                None => not_loaded(),
            },
            Request::StreamBegin {
                run,
                stream,
                language_mode,
            } => match session.as_mut() {
                Some(loaded) => {
                    let _ = (&run, &stream);
                    if write_message(&mut output, &Response::Ok, None).is_err() {
                        return 1;
                    }
                    if let Err(e) =
                        serve_stream(loaded, language_mode, &requests, &mut output)
                    {
                        error!("Protocol write failed during stream: {}", e);
                        return 1;
                    }
                    crate::memory::trim_freed_memory();
                    continue;
                }
                None => not_loaded(),
            },
            Request::Feed | Request::Finalize { .. } | Request::StreamReset => {
                Response::Error("no active stream".to_string())
            }
        };
        if let Err(e) = write_message(&mut output, &response, None) {
            error!("Protocol write failed: {}", e);
            return 1;
        }
        // After the reply, so it never delays the transcript.
        if finished_run {
            drop(pcm);
            crate::memory::trim_freed_memory();
        }
    }
    // Unreachable in practice: the reader exits the process on EOF.
    0
}

/// The stdin reader. Forwards requests to the main loop, and ends the
/// process when the parent closes stdin (unload, quit, or the parent died).
/// Cancels never reach the worker: the parent kills it instead. `_exit`
/// skips C++ static destructors, so a model still alive at that point can't
/// trip a teardown assert, and the OS reclaims its memory.
fn read_requests(requests: mpsc::Sender<Incoming>) -> ! {
    let mut input = BufReader::new(io::stdin().lock());
    loop {
        match read_message::<Request>(&mut input) {
            Ok(Some((request, pcm))) => {
                if requests.send(Incoming { request, pcm }).is_err() {
                    exit_now(1);
                }
            }
            Ok(None) => exit_now(0),
            Err(e) => {
                error!("Protocol read failed: {}", e);
                exit_now(1);
            }
        }
    }
}

fn exit_now(code: i32) -> ! {
    let _ = io::stderr().flush();
    // SAFETY: `_exit` only ends the process; nothing runs afterwards.
    unsafe { libc::_exit(code) }
}

/// Everything the UI needs from one in-flight dictation, plus the previous
/// hypothesis the commit split is computed against.
struct Live {
    full: String,
    committed: String,
    tentative: String,
    revision: i32,
    fed_samples: i64,
}

impl Live {
    fn new() -> Self {
        Self {
            full: String::new(),
            committed: String::new(),
            tentative: String::new(),
            revision: 0,
            fed_samples: 0,
        }
    }

    /// Fold a fresh hypothesis in. The streaming transducer only ever revises
    /// its tail, so the longest common prefix with the previous hypothesis is
    /// safe to commit: it cannot flicker, and the remainder stays tentative
    /// until the next frames agree on it.
    fn apply(&mut self, hypothesis: String, is_final: bool) -> (StreamUpdate, StreamText) {
        let (head, tail) = if is_final {
            (hypothesis.clone(), String::new())
        } else {
            split_stable(&self.full, &hypothesis)
        };
        let committed_changed = head != self.committed;
        let tentative_changed = tail != self.tentative;
        let result_changed = hypothesis != self.full || committed_changed || tentative_changed;

        self.committed = head;
        self.tentative = tail;
        self.full = hypothesis;
        self.revision += 1;

        let update = StreamUpdate {
            result_changed,
            is_final,
            revision: self.revision,
            input_received_ms: self.fed_samples * 1000 / 16_000,
            audio_committed_ms: 0,
            buffered_ms: 0,
            committed_changed,
            tentative_changed,
        };
        let text = StreamText {
            full: self.full.clone(),
            committed: self.committed.clone(),
            tentative: self.tentative.clone(),
        };
        (update, text)
    }
}

/// Byte length of the longest common prefix of two strings, snapped to a
/// character boundary so slicing the newer one is always valid.
fn split_stable(previous: &str, current: &str) -> (String, String) {
    let mut split = 0;
    for (a, b) in previous.chars().zip(current.chars()) {
        if a != b {
            break;
        }
        split += a.len_utf8();
    }
    let split = split.min(current.len());
    (current[..split].to_string(), current[split..].to_string())
}

/// Everything one loaded model session holds: the Hebrew streaming engine,
/// and — unless the language mode forbids it — the English engine that owns
/// every other language and names the one it heard.
struct Session {
    /// Shared rather than owned so a decoding stream can outlive the borrow
    /// of the session itself — the router needs `&mut` access to the English
    /// engine while the Hebrew stream is still open.
    he: std::sync::Arc<Engine>,
    whisper: Whisper,
}

/// The English engine, loaded off the request path.
///
/// Whisper takes a couple of seconds to load and the Hebrew model must be
/// usable before then, so the worker starts the load in the background the
/// moment a model is loaded and joins it the first time something needs it.
/// By then the person has usually started talking, so the join is free.
enum Whisper {
    /// Not started. Hebrew-only mode leaves it this way for good; switching
    /// to another mode mid-session starts it on first use instead of forcing
    /// a model reload.
    Idle(PathBuf),
    Loading(mpsc::Receiver<Result<OfflineEngine, String>>),
    Ready(OfflineEngine),
    Failed(String),
    /// Vacant slot, held only for the instant a variant is replaced in
    /// place. Never observed by callers.
    Off,
}

impl Whisper {
    /// Start loading in the background, so the Hebrew model is usable
    /// immediately.
    fn start(dir: PathBuf) -> Self {
        let (tx, rx) = mpsc::channel();
        match thread::Builder::new()
            .name("whisper-loader".into())
            .spawn(move || {
                let loaded = OfflineEngine::load(&dir);
                match &loaded {
                    Ok(_) => debug!("the English model finished loading"),
                    Err(e) => error!("the English model failed to load: {}", e),
                }
                let _ = tx.send(loaded);
            }) {
            Ok(_) => Whisper::Loading(rx),
            Err(e) => Whisper::Failed(format!("could not start the model loader: {e}")),
        }
    }

    /// The loaded engine, starting or waiting for the load if this is the
    /// first caller.
    fn ready(&mut self) -> Result<&OfflineEngine, String> {
        if let Whisper::Idle(_) = self {
            let Whisper::Idle(dir) = std::mem::replace(self, Whisper::Off) else {
                unreachable!("just matched")
            };
            *self = Whisper::start(dir);
        }
        let pending = match self {
            Whisper::Loading(_) => {
                let Whisper::Loading(rx) = std::mem::replace(self, Whisper::Off) else {
                    unreachable!("just matched")
                };
                Some(rx)
            }
            _ => None,
        };
        if let Some(rx) = pending {
            *self = match rx.recv() {
                Ok(Ok(engine)) => Whisper::Ready(engine),
                Ok(Err(e)) => Whisper::Failed(e),
                Err(_) => Whisper::Failed("the model loader stopped unexpectedly".to_string()),
            };
        }
        match self {
            Whisper::Ready(engine) => Ok(engine),
            Whisper::Failed(e) => Err(e.clone()),
            _ => Err("the English model is not loaded".to_string()),
        }
    }
}

/// Per-recording routing state: the mode, and the audio Whisper may need.
///
/// There is no early language probe any more. A probe hears a second and a
/// half of whatever came first — often the silence before the first word —
/// and Whisper's verdict on that is wrong often enough to matter: it named
/// Hebrew speech Arabic and the good Hebrew transcript was thrown away. The
/// Hebrew model's own output answers the same question for free, at the end,
/// with the whole recording behind it.
struct Router {
    mode: LanguageMode,
    /// The whole recording. Whisper decodes it in one go, and only when the
    /// Hebrew model heard no Hebrew at all.
    all: Vec<f32>,
}

impl Router {
    fn new(mode: LanguageMode, capacity_hint: usize) -> Self {
        Self {
            mode,
            all: Vec::with_capacity(capacity_hint),
        }
    }

    fn push(&mut self, pcm: &[f32]) {
        self.all.extend_from_slice(pcm);
    }
}

/// How many Hebrew and Latin letters a transcript contains.
///
/// This is the whole language decision. The Hebrew model writes any English it
/// hears in Latin script, in place, inside the Hebrew sentence — so the script
/// of its own output says exactly which language the recording was in. A
/// Hebrew sentence that contains English words still has Hebrew letters in it;
/// a sentence in another language has none.
fn script_of(text: &str) -> (usize, usize) {
    let mut hebrew = 0;
    let mut latin = 0;
    for c in text.chars() {
        if ('\u{0590}'..='\u{05FF}').contains(&c) {
            hebrew += 1;
        } else if c.is_ascii_alphabetic() {
            latin += 1;
        }
    }
    (hebrew, latin)
}

/// Decode the whole recording with the English engine, or `None` when it
/// isn't available (Hebrew-only mode, or a load that failed).
fn whisper_decode(session: &mut Session, samples: &[f32]) -> Option<Heard> {
    match session.whisper.ready() {
        Ok(engine) => match engine.decode(samples) {
            Ok(heard) => Some(heard),
            Err(e) => {
                error!("the English model could not decode this recording: {}", e);
                None
            }
        },
        Err(e) => {
            debug!("no English transcript for this recording: {}", e);
            None
        }
    }
}

/// Pick the transcript and the language for a finished recording.
///
/// Hebrew is the reason this product exists, so the Hebrew model keeps every
/// recording it heard Hebrew in — untouched, punctuation and all, including
/// the English words it wrote in Latin script. Whisper is consulted only when
/// the Hebrew model produced no Hebrew letter at all, which means the
/// recording was in another language and Whisper is the better transcriber
/// for it.
fn finalize_text(session: &mut Session, router: &Router, he_text: &str) -> (String, String) {
    let heard = he_text.trim().to_string();
    if router.mode == LanguageMode::Hebrew {
        return (heard, "he".to_string());
    }
    if heard.is_empty() {
        return (heard, "he".to_string());
    }

    let (hebrew, _latin) = script_of(&heard);
    if hebrew > 0 {
        return (heard, "he".to_string());
    }

    match whisper_decode(session, &router.all) {
        Some(other) if !other.text.trim().is_empty() => (
            other.text.trim().to_string(),
            other.lang.unwrap_or_else(|| "und".to_string()),
        ),
        // The English engine is unavailable or came up empty; the Hebrew
        // model's own Latin transcript is still a transcript.
        _ => (heard, "und".to_string()),
    }
}

/// Serve requests against an active stream until it is finalized or reset.
fn serve_stream(
    session: &mut Session,
    mode: LanguageMode,
    requests: &mpsc::Receiver<Incoming>,
    output: &mut impl Write,
) -> io::Result<()> {
    let he = std::sync::Arc::clone(&session.he);
    let stream = he.stream();
    let mut live = Live::new();
    // A dictation is a few seconds; 30 s of headroom avoids regrowing.
    let mut router = Router::new(mode, 30 * 16_000);

    while let Ok(Incoming { request, pcm }) = requests.recv() {
        let (response, done) = match request {
            Request::Feed => {
                live.fed_samples += pcm.len() as i64;
                router.push(&pcm);
                stream.accept(&pcm);
                // The Hebrew model drives the preview for every recording,
                // whatever language it turns out to be: it is the only engine
                // that streams, and on non-Hebrew audio it writes what it
                // hears in Latin script rather than inventing Hebrew.
                let hypothesis = stream.text();
                let (update, text) = live.apply(hypothesis, false);
                let text = (update.result_changed).then_some(text);
                (Response::Fed { update, text }, false)
            }
            Request::Finalize { want_language } => {
                // Flush the frames still inside the feature pipeline, then
                // read the settled hypothesis.
                stream.finish();
                let he_text = stream.text();
                let (text, language) = finalize_text(session, &router, &he_text);
                let (update, text) = live.apply(text, true);
                (
                    Response::Finalized {
                        update,
                        text,
                        language: want_language.then_some(language),
                    },
                    true,
                )
            }
            Request::StreamReset => {
                stream.reset();
                live = Live::new();
                router = Router::new(mode, 30 * 16_000);
                (Response::Ok, true)
            }
            _ => (
                Response::Error("a stream is active; finalize or reset it first".to_string()),
                false,
            ),
        };
        write_message(output, &response, None)?;
        if done {
            break;
        }
    }
    Ok(())
}

fn not_loaded() -> Response {
    Response::Error("no model loaded".to_string())
}

/// Transcribe a whole buffer in one go, for file/drop transcription.
fn run_once(
    session: &mut Session,
    pcm: &[f32],
    mode: LanguageMode,
    _options: &RunOptions,
) -> Result<Transcript, String> {
    let he = std::sync::Arc::clone(&session.he);
    let stream = he.stream();
    for chunk in pcm.chunks(RUN_CHUNK) {
        stream.accept(chunk);
    }
    stream.finish();
    let he_text = stream.text();

    // The audio is already complete, so the Hebrew model transcribes all of
    // it and `finalize_text` decides whether that transcript stands or
    // Whisper's is the better one.
    let mut router = Router::new(mode, pcm.len());
    router.push(pcm);
    let (text, language) = finalize_text(session, &router, &he_text);
    let text = text.trim().to_string();
    let duration_ms = (pcm.len() as i64) * 1000 / 16_000;
    Ok(Transcript {
        raw_text: text.clone(),
        text: text.clone(),
        language: Some(language),
        timestamp_kind: TimestampKind::None,
        segments: vec![Segment {
            t0_ms: 0,
            t1_ms: duration_ms,
            text,
            ..Segment::default()
        }],
        timings: Timings::default(),
        ..Transcript::default()
    })
}

fn load(
    path: &Path,
    backend: Backend,
    selector: DeviceSelector,
    mode: LanguageMode,
) -> Result<(Session, LoadedInfo), String> {
    // The parent names the file it wants loaded; every model in this build is
    // one folder, so the sibling files come from its parent directory.
    let _ = (backend, selector);
    let dir = path
        .parent()
        .ok_or_else(|| format!("the model path has no folder: {}", path.display()))?;

    inject_fault("load", false);
    let engine = Engine::load(dir).map_err(|e| format!("Failed to load model: {e}"))?;

    // The English engine sits beside the Hebrew one. Hebrew-only mode never
    // pays for it: Whisper costs ~78 MB of RAM and a couple of seconds.
    let whisper = if mode == LanguageMode::Hebrew {
        Whisper::Idle(dir.join("whisper"))
    } else {
        Whisper::start(dir.join("whisper"))
    };

    let info = LoadedInfo {
        arch: "zipformer2".to_string(),
        variant: "int8".to_string(),
        backend: "cpu".to_string(),
        device: "CPU".to_string(),
        on_gpu: false,
        capabilities: Capabilities {
            native_sample_rate: 16_000,
            languages: vec!["he".to_string()],
            translate_target_languages: Vec::new(),
            max_timestamp_kind: TimestampKind::None,
            supports_language_detect: true,
            supports_translate: false,
            supports_streaming: true,
            supports_spec_decode: false,
            max_audio_ms: 0,
        },
        supports_initial_prompt: false,
        supports_long_form: false,
        supports_vocabulary: false,
    };
    debug!(
        "loaded the Hebrew streaming model from {} (language mode {:?})",
        dir.display(),
        mode
    );
    Ok((
        Session {
            he: std::sync::Arc::new(engine),
            whisper,
        },
        info,
    ))
}

/// Debug-build fault injection for exercising crash/hang recovery:
/// `HANDY_WORKER_FAULT=<abort|segv|hang>@<init|list|load|run|feed|finalize>`
/// (`abort` is what an assert ends in, `segv` a real invalid memory write).
/// With `HANDY_WORKER_FAULT_ONCE=<marker path>` it fires only in the first
/// worker to reach that stage (the marker file records that it fired).
#[cfg(debug_assertions)]
fn inject_fault(stage: &str, on_gpu: bool) {
    let Ok(spec) = std::env::var("HANDY_WORKER_FAULT") else {
        return;
    };
    let Some((kind, at)) = spec.split_once('@') else {
        return;
    };
    if at != stage {
        return;
    }
    if !on_gpu && std::env::var_os("HANDY_WORKER_FAULT_GPU_ONLY").is_some() {
        return;
    }
    if let Some(marker) = std::env::var_os("HANDY_WORKER_FAULT_ONCE") {
        if std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&marker)
            .is_err()
        {
            return;
        }
    }
    eprintln!("injected fault: {kind} at {stage}");
    match kind {
        "abort" => std::process::abort(),
        // SAFETY: deliberately crash the worker with a real invalid memory
        // write, as a faulting driver does: SIGSEGV on Unix, an access
        // violation on Windows (where raising SIGSEGV only exits with code
        // 3). On Unix, restore the default disposition first so Rust's
        // stack-overflow handler doesn't get involved.
        "segv" => unsafe {
            #[cfg(unix)]
            libc::signal(libc::SIGSEGV, libc::SIG_DFL);
            std::ptr::write_volatile(std::ptr::null_mut::<u32>(), 1);
        },
        "hang" => loop {
            std::thread::sleep(std::time::Duration::from_secs(3600));
        },
        _ => {}
    }
}

#[cfg(not(debug_assertions))]
fn inject_fault(_stage: &str, _on_gpu: bool) {}

/// Logs go to stderr as `\x01LEVEL\ttarget\tmessage` lines; the parent
/// re-logs them at the same level.
struct StderrLogger;

impl Log for StderrLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let message = record.args().to_string();
        let mut stderr = io::stderr().lock();
        for line in message.lines() {
            let _ = writeln!(
                stderr,
                "{LOG_LINE_PREFIX}{}\t{}\t{}",
                record.level(),
                record.target(),
                line
            );
        }
    }

    fn flush(&self) {
        let _ = io::stderr().flush();
    }
}

fn init_logger() {
    static LOGGER: StderrLogger = StderrLogger;
    let level = std::env::var(LOG_LEVEL_ENV)
        .ok()
        .and_then(|v| v.parse::<LevelFilter>().ok())
        .unwrap_or(LevelFilter::Info);
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(level);
    }
}

/// SIGUSR2 toggles transcription in the app (and WebKitGTK uses SIGUSR1).
/// Aimed at Handy by name, one can reach this worker instead, and its
/// default action would kill it.
#[cfg(unix)]
fn ignore_app_signals() {
    // SAFETY: setting a signal's disposition to SIG_IGN has no preconditions.
    unsafe {
        libc::signal(libc::SIGUSR1, libc::SIG_IGN);
        libc::signal(libc::SIGUSR2, libc::SIG_IGN);
    }
}

/// Started from /proc/self/exe, the kernel names this process "exe". Give it
/// a distinct name so `pkill`/`killall` aimed at the app never reaches it.
#[cfg(target_os = "linux")]
fn set_process_name() {
    // SAFETY: PR_SET_NAME copies a NUL-terminated name of up to 16 bytes.
    unsafe {
        libc::prctl(libc::PR_SET_NAME, c"VoiceTranscriber-w".as_ptr());
    }
}

#[cfg(unix)]
fn take_stdout_for_protocol() -> io::Result<File> {
    use std::os::fd::FromRawFd;
    // SAFETY: plain fd duplication; the new fd is owned by the returned File.
    unsafe {
        let fd = libc::dup(libc::STDOUT_FILENO);
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        if libc::dup2(libc::STDERR_FILENO, libc::STDOUT_FILENO) < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(File::from_raw_fd(fd))
    }
}

#[cfg(windows)]
fn take_stdout_for_protocol() -> io::Result<File> {
    use std::os::windows::io::FromRawHandle;
    // Native code writes through the CRT's fd 1, so redirect that one. The
    // protocol uses the OS handle behind a CRT duplicate of the original.
    // SAFETY: CRT fd duplication; the duplicate's handle is owned by the File
    // for the rest of the process.
    unsafe {
        let fd = libc::dup(1);
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        if libc::dup2(2, 1) < 0 {
            return Err(io::Error::last_os_error());
        }
        let handle = libc::get_osfhandle(fd);
        if handle == -1 {
            return Err(io::Error::last_os_error());
        }
        Ok(File::from_raw_handle(handle as _))
    }
}

#[cfg(test)]
mod tests {
    use super::{script_of, split_stable};

    /// The whole language decision rests on this: a Hebrew sentence keeps
    /// Hebrew letters even when English words sit inside it, and a sentence
    /// in another language has none at all.
    #[test]
    fn a_hebrew_sentence_with_english_words_is_still_hebrew() {
        let (hebrew, latin) =
            script_of("עכשיו אתה יכול לזהות את הוורד שאני אומר לך? תנו לך rules מאוד פשוטים.");
        assert!(hebrew > 0);
        assert!(latin > 0, "the English word is written in Latin script");
        assert!(hebrew > latin);
    }

    #[test]
    fn an_english_sentence_has_no_hebrew_letters() {
        let (hebrew, latin) = script_of(
            "I need to pick up the kids from school at 4, and then we can talk.",
        );
        assert_eq!(hebrew, 0);
        assert!(latin > 0);
    }

    #[test]
    fn punctuation_and_digits_are_neither() {
        assert_eq!(script_of("?.,! 123 -"), (0, 0));
    }

    #[test]
    fn a_mixed_recording_keeps_both_scripts() {
        let (hebrew, latin) =
            script_of("אתה מבין? אין עכשיו את זה. I need to pick up the kids from school.");
        assert!(hebrew > 0);
        assert!(latin > 0);
    }

    #[test]
    fn commits_the_shared_prefix_only() {
        let (head, tail) = split_stable("שלום עו", "שלום עולם");
        assert_eq!(head, "שלום עו");
        assert_eq!(tail, "לם");
    }

    #[test]
    fn a_revision_pulls_the_commit_back() {
        let (head, tail) = split_stable("שלום עולם", "שלום עול");
        assert_eq!(head, "שלום עול");
        assert_eq!(tail, "");
    }

    #[test]
    fn nothing_in_common_leaves_everything_tentative() {
        let (head, tail) = split_stable("אבג", "דהו");
        assert_eq!(head, "");
        assert_eq!(tail, "דהו");
    }

    #[test]
    fn empty_previous_commits_nothing() {
        let (head, tail) = split_stable("", "טקסט");
        assert_eq!(head, "");
        assert_eq!(tail, "טקסט");
    }
}
