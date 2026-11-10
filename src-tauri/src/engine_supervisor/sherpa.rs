//! Runtime binding to the sherpa-onnx C API — the fork's ASR engine.
//!
//! Upstream Handy runs every model through transcribe.cpp (GGUF/ggml). Our
//! Hebrew model is a streaming Zipformer transducer exported as ONNX, which
//! transcribe.cpp cannot load at all, so the ASR path here is sherpa-onnx
//! instead. Only ASR moves: VAD and language ID still use transcribe.cpp in
//! the parent process, which is why the crate stays a dependency.
//!
//! Two recognizers are bound. [`Engine`] is the streaming Zipformer that
//! carries Hebrew, and [`OfflineEngine`] is the Whisper model that carries
//! every other language — and, because Whisper reports the language it heard,
//! the one that decides which of the two a recording belongs to.
//!
//! The library is bound with `LoadLibraryW`/`GetProcAddress` rather than at
//! link time. Two reasons: a missing or mismatched DLL becomes a readable
//! error in the worker log instead of a process that never starts, and the
//! DLLs can ship as Tauri resources instead of having to sit beside the
//! executable.
//!
//! The `#[repr(C)]` mirrors below copy field order and types from
//! `sherpa-onnx/c-api/c-api.h` (v2.0.1). The header requires every config
//! struct to be zero-initialised before use, which is what [`Engine::load`]
//! does. Because a silently wrong layout would read garbage pointers, the
//! sizes the C compiler produces are pinned in `sizes_match_the_c_header`.

use std::ffi::{c_char, c_void, CStr, CString};
use std::path::{Path, PathBuf};

/// The four files a streaming transducer needs, in one folder.
const ENCODER: &str = "encoder.int8.onnx";
const DECODER: &str = "decoder.int8.onnx";
const JOINER: &str = "joiner.int8.onnx";
const TOKENS: &str = "tokens.txt";

/// The three files Whisper needs, in the `whisper` folder beside the above.
const WHISPER_ENCODER: &str = "base-encoder.int8.onnx";
const WHISPER_DECODER: &str = "base-decoder.int8.onnx";
const WHISPER_TOKENS: &str = "base-tokens.txt";

/// Zipformer expects 16 kHz mono and 80-bin features; Whisper's encoder was
/// trained on the same features, so one feature config serves both.
const SAMPLE_RATE: i32 = 16_000;
const FEATURE_DIM: i32 = 80;
const NUM_THREADS: i32 = 2;

/// Whisper decodes in one go at key release, when the streaming model is
/// idle, so it can afford more threads than the model that has to keep up
/// with live audio.
const WHISPER_THREADS: i32 = 4;
const MAX_ACTIVE_PATHS: i32 = 4;

/// Overridable for development; otherwise the DLLs are looked up next to the
/// executable and in the Tauri resource folder.
const LIB_DIR_ENV: &str = "VT_SHERPA_DIR";

/// The name sherpa-onnx's C API ships under on this platform.
#[cfg(windows)]
const LIBRARY_FILE: &str = "sherpa-onnx-c-api.dll";
#[cfg(target_os = "macos")]
const LIBRARY_FILE: &str = "libsherpa-onnx-c-api.dylib";
#[cfg(all(unix, not(target_os = "macos")))]
const LIBRARY_FILE: &str = "libsherpa-onnx-c-api.so";

/* ── C ABI mirrors ─────────────────────────────────────────────────────── */

#[repr(C)]
struct FeatureConfig {
    sample_rate: i32,
    feature_dim: i32,
}

#[repr(C)]
struct TransducerModelConfig {
    encoder: *const c_char,
    decoder: *const c_char,
    joiner: *const c_char,
}

#[repr(C)]
struct ParaformerModelConfig {
    encoder: *const c_char,
    decoder: *const c_char,
}

#[repr(C)]
struct OneModelConfig {
    model: *const c_char,
}

#[repr(C)]
struct OnlineModelConfig {
    transducer: TransducerModelConfig,
    paraformer: ParaformerModelConfig,
    zipformer2_ctc: OneModelConfig,
    tokens: *const c_char,
    num_threads: i32,
    provider: *const c_char,
    debug: i32,
    model_type: *const c_char,
    modeling_unit: *const c_char,
    bpe_vocab: *const c_char,
    tokens_buf: *const c_char,
    tokens_buf_size: i32,
    nemo_ctc: OneModelConfig,
    t_one_ctc: OneModelConfig,
}

#[repr(C)]
struct CtcFstDecoderConfig {
    graph: *const c_char,
    max_active: i32,
}

#[repr(C)]
struct HomophoneReplacerConfig {
    dict_dir: *const c_char,
    lexicon: *const c_char,
    rule_fsts: *const c_char,
}

#[repr(C)]
struct OnlineRecognizerConfig {
    feat_config: FeatureConfig,
    model_config: OnlineModelConfig,
    decoding_method: *const c_char,
    max_active_paths: i32,
    enable_endpoint: i32,
    rule1_min_trailing_silence: f32,
    rule2_min_trailing_silence: f32,
    rule3_min_utterance_length: f32,
    hotwords_file: *const c_char,
    hotwords_score: f32,
    ctc_fst_decoder_config: CtcFstDecoderConfig,
    rule_fsts: *const c_char,
    rule_fars: *const c_char,
    blank_penalty: f32,
    hotwords_buf: *const c_char,
    hotwords_buf_size: i32,
    hr: HomophoneReplacerConfig,
}

#[repr(C)]
struct RecognizerResult {
    text: *const c_char,
    tokens: *const c_char,
    tokens_arr: *const *const c_char,
    timestamps: *mut f32,
    count: i32,
    json: *const c_char,
}

/* ── C ABI mirrors: offline (Whisper) ──────────────────────────────────── */

// `SherpaOnnxOfflineModelConfig` is a union-by-convention: one by-value slot
// per model family, only one of which is filled. Every slot therefore has to
// be mirrored, even the families this build never loads, or the offsets of
// everything after them drift.

#[repr(C)]
struct OfflinePtr3 {
    encoder: *const c_char,
    decoder: *const c_char,
    joiner: *const c_char,
}

#[repr(C)]
struct OfflinePtr2 {
    encoder: *const c_char,
    decoder: *const c_char,
}

#[repr(C)]
struct OfflinePtr1 {
    model: *const c_char,
}

#[repr(C)]
struct OfflineWhisperModelConfig {
    encoder: *const c_char,
    decoder: *const c_char,
    /// Empty (`NULL`) asks Whisper to detect the language and report it back.
    language: *const c_char,
    task: *const c_char,
    tail_paddings: i32,
    enable_token_timestamps: i32,
    enable_segment_timestamps: i32,
}

#[repr(C)]
struct OfflineSenseVoiceModelConfig {
    model: *const c_char,
    language: *const c_char,
    use_itn: i32,
}

#[repr(C)]
struct OfflineMoonshineModelConfig {
    preprocessor: *const c_char,
    encoder: *const c_char,
    uncached_decoder: *const c_char,
    cached_decoder: *const c_char,
    merged_decoder: *const c_char,
}

#[repr(C)]
struct OfflineCanaryModelConfig {
    encoder: *const c_char,
    decoder: *const c_char,
    src_lang: *const c_char,
    tgt_lang: *const c_char,
    use_pnc: i32,
}

#[repr(C)]
struct OfflineFunAsrNanoModelConfig {
    encoder_adaptor: *const c_char,
    llm: *const c_char,
    embedding: *const c_char,
    tokenizer: *const c_char,
    system_prompt: *const c_char,
    user_prompt: *const c_char,
    max_new_tokens: i32,
    temperature: f32,
    top_p: f32,
    seed: i32,
    language: *const c_char,
    itn: i32,
    hotwords: *const c_char,
}

#[repr(C)]
struct OfflineQwen3AsrModelConfig {
    conv_frontend: *const c_char,
    encoder: *const c_char,
    decoder: *const c_char,
    tokenizer: *const c_char,
    max_total_len: i32,
    max_new_tokens: i32,
    temperature: f32,
    top_p: f32,
    seed: i32,
    hotwords: *const c_char,
}

#[repr(C)]
struct OfflineCohereTranscribeModelConfig {
    encoder: *const c_char,
    decoder: *const c_char,
    language: *const c_char,
    use_punct: i32,
    use_itn: i32,
}

#[repr(C)]
struct OfflineModelConfig {
    transducer: OfflinePtr3,
    paraformer: OfflinePtr1,
    nemo_ctc: OfflinePtr1,
    whisper: OfflineWhisperModelConfig,
    tdnn: OfflinePtr1,
    tokens: *const c_char,
    num_threads: i32,
    debug: i32,
    provider: *const c_char,
    model_type: *const c_char,
    modeling_unit: *const c_char,
    bpe_vocab: *const c_char,
    telespeech_ctc: *const c_char,
    sense_voice: OfflineSenseVoiceModelConfig,
    moonshine: OfflineMoonshineModelConfig,
    fire_red_asr: OfflinePtr2,
    dolphin: OfflinePtr1,
    zipformer_ctc: OfflinePtr1,
    canary: OfflineCanaryModelConfig,
    wenet_ctc: OfflinePtr1,
    omnilingual: OfflinePtr1,
    medasr: OfflinePtr1,
    funasr_nano: OfflineFunAsrNanoModelConfig,
    fire_red_asr_ctc: OfflinePtr1,
    qwen3_asr: OfflineQwen3AsrModelConfig,
    cohere_transcribe: OfflineCohereTranscribeModelConfig,
}

#[repr(C)]
struct OfflineLmConfig {
    model: *const c_char,
    scale: f32,
}

#[repr(C)]
struct OfflineCtcFstDecoderConfig {
    graph: *const c_char,
    max_active: i32,
}

#[repr(C)]
struct OfflineRecognizerConfig {
    feat_config: FeatureConfig,
    model_config: OfflineModelConfig,
    lm_config: OfflineLmConfig,
    decoding_method: *const c_char,
    max_active_paths: i32,
    hotwords_file: *const c_char,
    hotwords_score: f32,
    rule_fsts: *const c_char,
    rule_fars: *const c_char,
    blank_penalty: f32,
    hr: HomophoneReplacerConfig,
    ctc_fst_decoder_config: OfflineCtcFstDecoderConfig,
}

#[repr(C)]
struct OfflineRecognizerResult {
    text: *const c_char,
    timestamps: *mut f32,
    count: i32,
    tokens: *const c_char,
    tokens_arr: *const *const c_char,
    json: *const c_char,
    /// The language Whisper heard. Empty when a language was forced.
    lang: *const c_char,
    emotion: *const c_char,
    event: *const c_char,
    durations: *mut f32,
    ys_log_probs: *mut f32,
    segment_timestamps: *const f32,
    segment_durations: *const f32,
    segment_texts: *const c_char,
    segment_texts_arr: *const *const c_char,
    segment_count: i32,
}

/* ── Function pointers ─────────────────────────────────────────────────── */

type CreateRecognizer = unsafe extern "C" fn(*const OnlineRecognizerConfig) -> *const c_void;
type DestroyRecognizer = unsafe extern "C" fn(*const c_void);
type CreateStream = unsafe extern "C" fn(*const c_void) -> *const c_void;
type DestroyStream = unsafe extern "C" fn(*const c_void);
type AcceptWaveform = unsafe extern "C" fn(*const c_void, i32, *const f32, i32);
type IsReady = unsafe extern "C" fn(*const c_void, *const c_void) -> i32;
type Decode = unsafe extern "C" fn(*const c_void, *const c_void);
type GetResult =
    unsafe extern "C" fn(*const c_void, *const c_void) -> *const RecognizerResult;
type DestroyResult = unsafe extern "C" fn(*const RecognizerResult);
type StreamReset = unsafe extern "C" fn(*const c_void, *const c_void);
type InputFinished = unsafe extern "C" fn(*const c_void);

type CreateOfflineRecognizer =
    unsafe extern "C" fn(*const OfflineRecognizerConfig) -> *const c_void;
type DestroyOfflineRecognizer = unsafe extern "C" fn(*const c_void);
type CreateOfflineStream = unsafe extern "C" fn(*const c_void) -> *const c_void;
type DestroyOfflineStream = unsafe extern "C" fn(*const c_void);
type AcceptWaveformOffline = unsafe extern "C" fn(*const c_void, i32, *const f32, i32);
type DecodeOfflineStream = unsafe extern "C" fn(*const c_void, *const c_void);
type GetOfflineResult =
    unsafe extern "C" fn(*const c_void) -> *const OfflineRecognizerResult;
type DestroyOfflineResult = unsafe extern "C" fn(*const OfflineRecognizerResult);

struct Api {
    create_recognizer: CreateRecognizer,
    destroy_recognizer: DestroyRecognizer,
    create_stream: CreateStream,
    destroy_stream: DestroyStream,
    accept_waveform: AcceptWaveform,
    is_ready: IsReady,
    decode: Decode,
    get_result: GetResult,
    destroy_result: DestroyResult,
    stream_reset: StreamReset,
    input_finished: InputFinished,
    create_offline_recognizer: CreateOfflineRecognizer,
    destroy_offline_recognizer: DestroyOfflineRecognizer,
    create_offline_stream: CreateOfflineStream,
    destroy_offline_stream: DestroyOfflineStream,
    accept_waveform_offline: AcceptWaveformOffline,
    decode_offline_stream: DecodeOfflineStream,
    get_offline_result: GetOfflineResult,
    destroy_offline_result: DestroyOfflineResult,
}

/* ── Platform loader ───────────────────────────────────────────────────── */

#[cfg(windows)]
mod loader {
    use std::ffi::c_void;

    // kernel32 is linked into every Rust binary on Windows, so these need no
    // extra crate or build-script plumbing.
    extern "system" {
        pub fn LoadLibraryW(name: *const u16) -> *mut c_void;
        pub fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
        pub fn SetDllDirectoryW(path: *const u16) -> i32;
    }

    fn wide(path: &std::path::Path) -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt;
        path.as_os_str().encode_wide().chain([0]).collect()
    }

    /// `onnxruntime.dll` is imported by name, and Windows resolves that
    /// against the *process* search path, not the importing DLL's folder.
    /// Pointing the DLL directory at ours is what makes the pair load.
    pub fn set_dll_directory(dir: &std::path::Path) -> bool {
        let w = wide(dir);
        unsafe { SetDllDirectoryW(w.as_ptr()) != 0 }
    }

    pub fn open(path: &std::path::Path) -> Result<*mut c_void, String> {
        let w = wide(path);
        let handle = unsafe { LoadLibraryW(w.as_ptr()) };
        if handle.is_null() {
            return Err(format!(
                "could not load {} ({})",
                path.display(),
                std::io::Error::last_os_error()
            ));
        }
        Ok(handle)
    }

    pub fn symbol(module: *mut c_void, name: &str) -> Result<*mut c_void, String> {
        let cname = std::ffi::CString::new(name).map_err(|e| e.to_string())?;
        let addr = unsafe { GetProcAddress(module, cname.as_ptr() as *const u8) };
        if addr.is_null() {
            return Err(format!("sherpa-onnx is missing the symbol {name}"));
        }
        Ok(addr)
    }
}

#[cfg(not(windows))]
mod loader {
    use std::ffi::{c_char, c_int, c_void, CString};

    // RTLD_NOW resolves every symbol up front, so a missing one is a readable
    // error here instead of a crash at the first decode. RTLD_GLOBAL lets the
    // runtime library's own dependencies resolve against this handle.
    const RTLD_NOW: c_int = 2;
    const RTLD_GLOBAL: c_int = 0x100;

    extern "C" {
        fn dlopen(filename: *const c_char, flag: c_int) -> *mut c_void;
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
        fn dlerror() -> *mut c_char;
    }

    fn last_error() -> String {
        unsafe {
            let raw = dlerror();
            if raw.is_null() {
                return "no detail from the loader".to_string();
            }
            std::ffi::CStr::from_ptr(raw).to_string_lossy().into_owned()
        }
    }

    /// macOS and Linux resolve a library's dependencies from its own install
    /// name and rpath, so there is no equivalent of `SetDllDirectoryW` to
    /// call. What we can do is prepend the folder to the dynamic loader's
    /// search path, which is what makes `libsherpa-onnx-c-api.dylib` find the
    /// `libonnxruntime.dylib` sitting beside it. Appending rather than
    /// replacing keeps anything the user already had set.
    pub fn set_dll_directory(dir: &std::path::Path) -> bool {
        let key = if cfg!(target_os = "macos") {
            "DYLD_LIBRARY_PATH"
        } else {
            "LD_LIBRARY_PATH"
        };
        let current = std::env::var_os(key)
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_default();
        if current
            .split(':')
            .any(|part| !part.is_empty() && std::path::Path::new(part) == dir)
        {
            return true;
        }
        let mut next = dir.to_string_lossy().into_owned();
        if !current.is_empty() {
            next.push(':');
            next.push_str(&current);
        }
        std::env::set_var(key, next);
        true
    }

    pub fn open(path: &std::path::Path) -> Result<*mut c_void, String> {
        let name = CString::new(path.to_string_lossy().as_bytes())
            .map_err(|e| format!("the library path is not valid: {e}"))?;
        let handle = unsafe { dlopen(name.as_ptr(), RTLD_NOW | RTLD_GLOBAL) };
        if handle.is_null() {
            return Err(format!("could not load {} ({})", path.display(), last_error()));
        }
        Ok(handle)
    }

    pub fn symbol(module: *mut c_void, name: &str) -> Result<*mut c_void, String> {
        // Mach-O keeps C symbols with a leading underscore in the symbol
        // table, and whether `dlsym` accepts the bare name differs between
        // Darwin releases. Trying both costs one failed lookup and removes
        // the guesswork.
        let mut candidates = vec![name.to_string()];
        candidates.push(format!("_{name}"));

        let mut detail = String::new();
        for candidate in candidates {
            let cname = CString::new(candidate).map_err(|e| e.to_string())?;
            unsafe { dlerror() };
            let addr = unsafe { dlsym(module, cname.as_ptr()) };
            if !addr.is_null() {
                return Ok(addr);
            }
            detail = last_error();
        }
        Err(format!("sherpa-onnx is missing {name} ({detail})"))
    }
}

/* ── Library discovery ─────────────────────────────────────────────────── */

/// Where the two DLLs may live, most specific first. The parent spawns the
/// worker as a copy of the installed executable, so `current_exe()` is the
/// install folder in production and `target/debug` during development (whose
/// grandparent is `src-tauri`, where the resource folder lives).
fn library_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(dir) = std::env::var_os(LIB_DIR_ENV) {
        dirs.push(PathBuf::from(dir));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            dirs.push(dir.to_path_buf());
            dirs.push(dir.join("resources").join("sherpa"));
            dirs.push(dir.join("resources"));
            // `cargo test` runs the binary from `target/<profile>/deps`.
            if let Some(up) = dir.parent() {
                dirs.push(up.join("resources").join("sherpa"));
                // macOS app bundle: Contents/MacOS/<exe> and the resources
                // land in Contents/Resources/resources/sherpa. The capital R
                // is how Tauri spells the folder; both spellings are pushed
                // because the default macOS filesystem is case-insensitive
                // but a case-sensitive volume is one checkbox away.
                dirs.push(up.join("Resources").join("resources").join("sherpa"));
                dirs.push(up.join("Resources").join("sherpa"));
                if let Some(root) = up.parent().and_then(|p| p.parent()) {
                    dirs.push(root.join("resources").join("sherpa"));
                }
            }
        }
    }
    // Development only: point straight at the source tree so `cargo test` and
    // `cargo run` work before Tauri has copied the resources next to the
    // binary. Never compiled into a release build.
    #[cfg(debug_assertions)]
    if let Some(root) = option_env!("CARGO_MANIFEST_DIR") {
        dirs.push(PathBuf::from(root).join("resources").join("sherpa"));
    }
    dirs
}

fn load_api() -> Result<Api, String> {
    let mut last = String::new();
    for dir in library_dirs() {
        let dll = dir.join(LIBRARY_FILE);
        if !dll.is_file() {
            continue;
        }
        loader::set_dll_directory(&dir);
        let module = match loader::open(&dll) {
            Ok(handle) => handle,
            Err(e) => {
                last = e;
                continue;
            }
        };
        // The module is deliberately never freed: it lives exactly as long
        // as the recognizer it owns, which is the life of the worker.
        return unsafe { bind(module) };
    }
    Err(if last.is_empty() {
        format!(
            "{} was not found (looked in {})",
            LIBRARY_FILE,
            library_dirs()
                .iter()
                .map(|d| d.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )
    } else {
        last
    })
}

unsafe fn bind(module: *mut c_void) -> Result<Api, String> {
    macro_rules! sym {
        ($name:literal, $ty:ty) => {{
            let raw = loader::symbol(module, $name)?;
            std::mem::transmute::<*mut c_void, $ty>(raw)
        }};
    }
    Ok(Api {
        create_recognizer: sym!("SherpaOnnxCreateOnlineRecognizer", CreateRecognizer),
        destroy_recognizer: sym!("SherpaOnnxDestroyOnlineRecognizer", DestroyRecognizer),
        create_stream: sym!("SherpaOnnxCreateOnlineStream", CreateStream),
        destroy_stream: sym!("SherpaOnnxDestroyOnlineStream", DestroyStream),
        accept_waveform: sym!("SherpaOnnxOnlineStreamAcceptWaveform", AcceptWaveform),
        is_ready: sym!("SherpaOnnxIsOnlineStreamReady", IsReady),
        decode: sym!("SherpaOnnxDecodeOnlineStream", Decode),
        get_result: sym!("SherpaOnnxGetOnlineStreamResult", GetResult),
        destroy_result: sym!("SherpaOnnxDestroyOnlineRecognizerResult", DestroyResult),
        stream_reset: sym!("SherpaOnnxOnlineStreamReset", StreamReset),
        input_finished: sym!("SherpaOnnxOnlineStreamInputFinished", InputFinished),
        create_offline_recognizer: sym!(
            "SherpaOnnxCreateOfflineRecognizer",
            CreateOfflineRecognizer
        ),
        destroy_offline_recognizer: sym!(
            "SherpaOnnxDestroyOfflineRecognizer",
            DestroyOfflineRecognizer
        ),
        create_offline_stream: sym!("SherpaOnnxCreateOfflineStream", CreateOfflineStream),
        destroy_offline_stream: sym!("SherpaOnnxDestroyOfflineStream", DestroyOfflineStream),
        accept_waveform_offline: sym!(
            "SherpaOnnxAcceptWaveformOffline",
            AcceptWaveformOffline
        ),
        decode_offline_stream: sym!("SherpaOnnxDecodeOfflineStream", DecodeOfflineStream),
        get_offline_result: sym!("SherpaOnnxGetOfflineStreamResult", GetOfflineResult),
        destroy_offline_result: sym!(
            "SherpaOnnxDestroyOfflineRecognizerResult",
            DestroyOfflineResult
        ),
    })
}

/* ── Public surface ────────────────────────────────────────────────────── */

/// A loaded recognizer. One per worker, reused by every stream.
pub struct Engine {
    api: Api,
    recognizer: *const c_void,
}

// SAFETY: sherpa-onnx recognizers are documented thread-safe for decoding;
// the worker only ever drives one stream at a time from its single loop.
unsafe impl Send for Engine {}

impl Engine {
    /// Load the streaming transducer found in `model_dir`. The folder is
    /// derived from the model path the parent asked for, so the four files
    /// must sit together.
    pub fn load(model_dir: &Path) -> Result<Self, String> {
        for name in [ENCODER, DECODER, JOINER, TOKENS] {
            if !model_dir.join(name).is_file() {
                return Err(format!(
                    "the speech model is incomplete: {} is missing",
                    model_dir.join(name).display()
                ));
            }
        }
        let api = load_api()?;

        // The config borrows these strings only for the duration of the call.
        let encoder = cstr(&model_dir.join(ENCODER))?;
        let decoder = cstr(&model_dir.join(DECODER))?;
        let joiner = cstr(&model_dir.join(JOINER))?;
        let tokens = cstr(&model_dir.join(TOKENS))?;
        let provider = CString::new("cpu").expect("static string");
        let method = CString::new("modified_beam_search").expect("static string");

        // SAFETY: the header requires a zeroed config; every field we leave
        // at zero is an optional feature we do not use.
        let mut config: OnlineRecognizerConfig = unsafe { std::mem::zeroed() };
        config.feat_config = FeatureConfig {
            sample_rate: SAMPLE_RATE,
            feature_dim: FEATURE_DIM,
        };
        config.model_config.transducer = TransducerModelConfig {
            encoder: encoder.as_ptr(),
            decoder: decoder.as_ptr(),
            joiner: joiner.as_ptr(),
        };
        config.model_config.tokens = tokens.as_ptr();
        config.model_config.num_threads = NUM_THREADS;
        config.model_config.provider = provider.as_ptr();
        config.decoding_method = method.as_ptr();
        config.max_active_paths = MAX_ACTIVE_PATHS;
        // Endpoints are not used: a dictation session is one continuous
        // stream, and the parent decides when it ends.
        config.enable_endpoint = 0;

        let recognizer = unsafe { (api.create_recognizer)(&config) };
        if recognizer.is_null() {
            return Err(
                "the speech model could not be loaded (sherpa-onnx rejected it)".to_string(),
            );
        }
        Ok(Self { api, recognizer })
    }

    /// Start a fresh decoding state.
    pub fn stream(&self) -> Stream<'_> {
        Stream {
            engine: self,
            ptr: unsafe { (self.api.create_stream)(self.recognizer) },
        }
    }

    /// Decode every frame that is ready. Must be called after each
    /// [`Stream::accept`].
    pub fn drain(&self, stream: &Stream) {
        while unsafe { (self.api.is_ready)(self.recognizer, stream.ptr) } != 0 {
            unsafe { (self.api.decode)(self.recognizer, stream.ptr) };
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        if !self.recognizer.is_null() {
            unsafe { (self.api.destroy_recognizer)(self.recognizer) };
        }
    }
}

/// One utterance's decoding state. Borrows the engine so it can never outlive
/// the recognizer it decodes against.
pub struct Stream<'a> {
    engine: &'a Engine,
    ptr: *const c_void,
}

impl Stream<'_> {
    /// Hand 16 kHz mono samples to the recognizer and decode what is ready.
    pub fn accept(&self, samples: &[f32]) {
        if samples.is_empty() || self.ptr.is_null() {
            return;
        }
        let n = i32::try_from(samples.len()).unwrap_or(i32::MAX);
        unsafe {
            (self.engine.api.accept_waveform)(self.ptr, SAMPLE_RATE, samples.as_ptr(), n);
        }
        self.engine.drain(self);
    }

    /// Tell the recognizer no more audio is coming, then drain the rest.
    ///
    /// A second of silence goes in first: the transducer only emits a token
    /// once it has seen the frames after it, so without the tail padding the
    /// last word or two of every dictation is lost.
    pub fn finish(&self) {
        if self.ptr.is_null() {
            return;
        }
        let silence = vec![0.0f32; SAMPLE_RATE as usize];
        for chunk in silence.chunks(3200) {
            self.accept(chunk);
        }
        unsafe { (self.engine.api.input_finished)(self.ptr) };
        self.engine.drain(self);
    }

    /// Clear the decoding state so the next utterance starts from silence.
    pub fn reset(&self) {
        if !self.ptr.is_null() {
            unsafe { (self.engine.api.stream_reset)(self.engine.recognizer, self.ptr) };
        }
    }

    /// The hypothesis so far. Empty when nothing was recognised yet.
    pub fn text(&self) -> String {
        if self.ptr.is_null() {
            return String::new();
        }
        unsafe {
            let result = (self.engine.api.get_result)(self.engine.recognizer, self.ptr);
            if result.is_null() {
                return String::new();
            }
            let text = CStr::from_ptr((*result).text)
                .to_string_lossy()
                .into_owned();
            (self.engine.api.destroy_result)(result);
            text
        }
    }
}

impl Drop for Stream<'_> {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe { (self.engine.api.destroy_stream)(self.ptr) };
        }
    }
}

fn cstr(path: &Path) -> Result<CString, String> {
    let text = path
        .to_str()
        .ok_or_else(|| format!("the model path is not valid UTF-8: {}", path.display()))?;
    CString::new(text).map_err(|e| format!("the model path contains a NUL byte: {e}"))
}

/* ── Offline recognizer (Whisper) ──────────────────────────────────────── */

/// What one Whisper decode produced: the text, and the language it heard.
#[derive(Debug, Clone, Default)]
pub struct Heard {
    pub text: String,
    /// Whisper's language verdict, e.g. `he` or `en`. `None` when the model
    /// reported nothing usable.
    pub lang: Option<String>,
}

/// A loaded Whisper recognizer. Unlike [`Engine`] it decodes a whole buffer
/// at once, and it is the only one of the two that can name the language.
pub struct OfflineEngine {
    api: Api,
    recognizer: *const c_void,
}

// SAFETY: the recognizer is only ever driven from the worker's single loop,
// and the binding holds no thread-affine state.
unsafe impl Send for OfflineEngine {}

impl OfflineEngine {
    /// Load the Whisper files in `dir`. Unlike the streaming engine this is
    /// not cheap — a couple of seconds for the bundled base model — so the
    /// worker loads it off the request path (see `worker::Session`).
    pub fn load(dir: &Path) -> Result<Self, String> {
        for name in [WHISPER_ENCODER, WHISPER_DECODER, WHISPER_TOKENS] {
            if !dir.join(name).is_file() {
                return Err(format!(
                    "the English speech model is incomplete: {} is missing",
                    dir.join(name).display()
                ));
            }
        }
        let api = load_api()?;

        let encoder = cstr(&dir.join(WHISPER_ENCODER))?;
        let decoder = cstr(&dir.join(WHISPER_DECODER))?;
        let tokens = cstr(&dir.join(WHISPER_TOKENS))?;
        let task = CString::new("transcribe").expect("static string");
        let provider = CString::new("cpu").expect("static string");
        let model_type = CString::new("whisper").expect("static string");

        // SAFETY: zeroed first, as the header requires; `language` stays NULL
        // so Whisper detects the language and reports it back in `lang`.
        let mut config: OfflineRecognizerConfig = unsafe { std::mem::zeroed() };
        config.feat_config = FeatureConfig {
            sample_rate: SAMPLE_RATE,
            feature_dim: FEATURE_DIM,
        };
        config.model_config.whisper = OfflineWhisperModelConfig {
            encoder: encoder.as_ptr(),
            decoder: decoder.as_ptr(),
            language: std::ptr::null(),
            task: task.as_ptr(),
            // -1: pad the clip to the 30 s window Whisper's encoder expects.
            // Without it the tail of a short recording is decoded weakly.
            tail_paddings: -1,
            enable_token_timestamps: 0,
            enable_segment_timestamps: 0,
        };
        config.model_config.tokens = tokens.as_ptr();
        config.model_config.num_threads = WHISPER_THREADS;
        config.model_config.provider = provider.as_ptr();
        config.model_config.model_type = model_type.as_ptr();

        let recognizer = unsafe { (api.create_offline_recognizer)(&config) };
        if recognizer.is_null() {
            return Err(
                "the English speech model could not be loaded (sherpa-onnx rejected it)"
                    .to_string(),
            );
        }
        Ok(Self { api, recognizer })
    }

    /// Decode one whole recording. `samples` is 16 kHz mono f32.
    pub fn decode(&self, samples: &[f32]) -> Result<Heard, String> {
        if samples.is_empty() {
            return Ok(Heard::default());
        }
        let n = i32::try_from(samples.len())
            .map_err(|_| "the recording is too long for one decode".to_string())?;
        unsafe {
            let stream = (self.api.create_offline_stream)(self.recognizer);
            if stream.is_null() {
                return Err("sherpa-onnx could not open a decoding stream".to_string());
            }
            (self.api.accept_waveform_offline)(stream, SAMPLE_RATE, samples.as_ptr(), n);
            (self.api.decode_offline_stream)(self.recognizer, stream);
            let result = (self.api.get_offline_result)(stream);
            let heard = if result.is_null() {
                Heard::default()
            } else {
                let text = cstr_field((*result).text);
                let lang = cstr_field((*result).lang);
                (self.api.destroy_offline_result)(result);
                Heard {
                    text,
                    lang: (!lang.is_empty()).then_some(lang),
                }
            };
            (self.api.destroy_offline_stream)(stream);
            Ok(heard)
        }
    }
}

impl Drop for OfflineEngine {
    fn drop(&mut self) {
        if !self.recognizer.is_null() {
            unsafe { (self.api.destroy_offline_recognizer)(self.recognizer) };
        }
    }
}

/// Read a `const char *` field that the API may leave NULL.
fn cstr_field(ptr: *const c_char) -> String {
    if ptr.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal 16-bit PCM WAV reader, so the test needs no audio crate.
    fn read_wav(path: &Path) -> Vec<f32> {
        let bytes = std::fs::read(path).expect("the test clip is readable");
        assert_eq!(&bytes[0..4], b"RIFF", "not a RIFF file");
        let mut channels = 1u16;
        let mut bits = 16u16;
        let mut data: &[u8] = &[];
        let mut pos = 12;
        while pos + 8 <= bytes.len() {
            let id = &bytes[pos..pos + 4];
            let size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;
            let end = (pos + 8 + size).min(bytes.len());
            let body = &bytes[pos + 8..end];
            match id {
                b"fmt " => {
                    channels = u16::from_le_bytes(body[2..4].try_into().unwrap());
                    bits = u16::from_le_bytes(body[14..16].try_into().unwrap());
                }
                b"data" => data = body,
                _ => {}
            }
            pos = end + (size & 1);
        }
        assert_eq!(bits, 16, "the test clip must be 16-bit PCM");
        let samples: Vec<f32> = data
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32_768.0)
            .collect();
        if channels <= 1 {
            samples
        } else {
            let n = channels as usize;
            samples
                .chunks(n)
                .map(|frame| frame.iter().sum::<f32>() / n as f32)
                .collect()
        }
    }

    /// End-to-end check of the swapped engine: loads the shipped Hebrew model
    /// through the real binding and transcribes a real clip.
    ///
    /// `VT_TEST_CLIP=<path to a 16 kHz wav> cargo test -- --ignored`
    #[test]
    #[ignore = "needs the bundled model, the sherpa DLLs and VT_TEST_CLIP"]
    fn transcribes_hebrew_audio() {
        let clip = std::env::var("VT_TEST_CLIP").expect("set VT_TEST_CLIP to a 16 kHz wav");
        let model_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("models");
        let engine = Engine::load(&model_dir).expect("the model loads");
        let samples = read_wav(Path::new(&clip));
        assert!(!samples.is_empty(), "the clip decoded to no samples");

        let stream = engine.stream();
        for chunk in samples.chunks(3200) {
            stream.accept(chunk);
        }
        stream.finish();
        let text = stream.text();
        println!("TRANSCRIPT: {text}");
        assert!(!text.trim().is_empty(), "the engine returned no text");
    }

    /// The C compiler's sizes for the three structs that carry pointers
    /// across the ABI, measured with `sizeof` in ctypes against the shipped
    /// DLL. A mismatch here means a field was added, removed or reordered in
    /// `c-api.h` and the mirrors above no longer match the library.
    #[test]
    fn sizes_match_the_c_header() {
        if std::mem::size_of::<usize>() != 8 {
            return;
        }
        assert_eq!(std::mem::size_of::<OfflineRecognizerConfig>(), 624);
        assert_eq!(std::mem::size_of::<OfflineModelConfig>(), 504);
        assert_eq!(std::mem::size_of::<OfflineRecognizerResult>(), 128);
    }

    /// Whisper's two jobs, on a real clip: transcribe non-Hebrew speech and
    /// say which language it heard.
    ///
    /// `VT_TEST_CLIP_EN=<path to a 16 kHz wav> cargo test -- --ignored`
    #[test]
    #[ignore = "needs the bundled Whisper model, the sherpa DLLs and VT_TEST_CLIP_EN"]
    fn whisper_names_the_language_it_heard() {
        let clip = std::env::var("VT_TEST_CLIP_EN").expect("set VT_TEST_CLIP_EN to a 16 kHz wav");
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("models")
            .join("whisper");
        let engine = OfflineEngine::load(&dir).expect("the Whisper model loads");
        let samples = read_wav(Path::new(&clip));
        assert!(!samples.is_empty(), "the clip decoded to no samples");

        let heard = engine.decode(&samples).expect("the decode runs");
        println!("WHISPER: lang={:?} text={:?}", heard.lang, heard.text);
        assert_eq!(heard.lang.as_deref(), Some("en"));
        assert!(
            heard.text.to_lowercase().contains("water"),
            "expected the English sentence, got {:?}",
            heard.text
        );
    }
}
