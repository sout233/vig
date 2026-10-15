//! Own the native recognizer, keeping C++ exceptions on the C++ side of FFI.
use sherpa_rs::sherpa_rs_sys as sys;
use std::{
    ffi::{CStr, CString, c_char, c_void},
    path::Path,
    ptr::NonNull,
};

unsafe extern "C" {
    fn vig_create_recognizer(
        config: *const c_void,
        error: *mut *const c_char,
    ) -> *const sys::SherpaOnnxOfflineRecognizer;
    fn vig_decode(
        recognizer: *const sys::SherpaOnnxOfflineRecognizer,
        stream: *const sys::SherpaOnnxOfflineStream,
        error: *mut *const c_char,
    ) -> bool;
}

pub(super) struct Recognizer {
    recognizer: NonNull<sys::SherpaOnnxOfflineRecognizer>,
}
// The editor serializes all model access through its loaded-model Mutex.
unsafe impl Send for Recognizer {}

pub(super) fn validate_tokens(directory: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(directory.join("tokens.txt"))
        .map_err(|e| format!("无法读取 SenseVoice tokens.txt：{e}"))?;
    let mut tokens = std::collections::HashSet::new();
    let mut ids = std::collections::HashSet::new();
    for (line, entry) in text.lines().enumerate() {
        let mut parts = entry.split_whitespace();
        let token = parts
            .next()
            .ok_or_else(|| format!("tokens.txt 第 {} 行为空", line + 1))?;
        let id = parts
            .next()
            .and_then(|value| value.parse::<u32>().ok())
            .ok_or_else(|| format!("tokens.txt 第 {} 行缺少有效编号", line + 1))?;
        if parts.next().is_some() || !tokens.insert(token) || !ids.insert(id) {
            return Err(format!("tokens.txt 第 {} 行格式无效或存在重复项", line + 1));
        }
    }
    if ids.iter().any(|id| *id as usize >= ids.len()) {
        return Err("tokens.txt 的编号必须从 0 连续排列".into());
    }
    for token in ["<unk>", "<|zh|>", "<|en|>", "<|Speech|>", "<|withitn|>"] {
        if !tokens.contains(token) {
            return Err(format!("tokens.txt 缺少 SenseVoice 标记 {token}"));
        }
    }
    Ok(())
}

impl Recognizer {
    pub(super) fn new(directory: &Path) -> Result<Self, String> {
        // sherpa's symbol-table loader terminates the process on invalid token text.
        validate_tokens(directory)?;
        let model = directory.join("model.int8.onnx");
        let tokens = directory.join("tokens.txt");
        let model = CString::new(model.to_str().ok_or("模型路径不是有效 UTF-8")?)
            .map_err(|e| e.to_string())?;
        let tokens = CString::new(tokens.to_str().ok_or("分词器路径不是有效 UTF-8")?)
            .map_err(|e| e.to_string())?;
        let language = CString::new("auto").unwrap();
        let provider = CString::new("cpu").unwrap();
        let mut config: sys::SherpaOnnxOfflineRecognizerConfig = unsafe { std::mem::zeroed() };
        config.feat_config.sample_rate = 16000;
        config.feat_config.feature_dim = 80;
        config.model_config.tokens = tokens.as_ptr();
        config.model_config.provider = provider.as_ptr();
        config.model_config.num_threads = std::thread::available_parallelism()
            .map_err(|e| e.to_string())?
            .get()
            .min(4) as i32;
        config.model_config.sense_voice.model = model.as_ptr();
        config.model_config.sense_voice.language = language.as_ptr();
        config.model_config.sense_voice.use_itn = 1;
        let mut error = std::ptr::null();
        let pointer = unsafe {
            vig_create_recognizer(
                (&config as *const sys::SherpaOnnxOfflineRecognizerConfig).cast(),
                &mut error,
            )
        };
        let recognizer = NonNull::new(pointer.cast_mut())
            .ok_or_else(|| native_error("加载 SenseVoice 失败", error))?;
        Ok(Self { recognizer })
    }

    pub(super) fn transcribe(&mut self, samples: &[f32]) -> Result<String, String> {
        let count = samples
            .len()
            .try_into()
            .map_err(|_| "语音段过长，无法传入识别引擎")?;
        unsafe {
            let stream = sys::SherpaOnnxCreateOfflineStream(self.recognizer.as_ptr());
            let stream =
                Stream(NonNull::new(stream.cast_mut()).ok_or("无法创建 SenseVoice 音频流")?);
            sys::SherpaOnnxAcceptWaveformOffline(stream.0.as_ptr(), 16000, samples.as_ptr(), count);
            let mut error = std::ptr::null();
            if !vig_decode(self.recognizer.as_ptr(), stream.0.as_ptr(), &mut error) {
                return Err(native_error("SenseVoice 识别失败", error));
            }
            let result = sys::SherpaOnnxGetOfflineStreamResult(stream.0.as_ptr());
            let result = NonNull::new(result.cast_mut()).ok_or("SenseVoice 没有返回识别结果")?;
            let text = if result.as_ref().text.is_null() {
                Err("SenseVoice 返回了无效文本指针".into())
            } else {
                CStr::from_ptr(result.as_ref().text)
                    .to_str()
                    .map(str::to_owned)
                    .map_err(|e| format!("SenseVoice 返回的文本不是有效 UTF-8：{e}"))
            };
            sys::SherpaOnnxDestroyOfflineRecognizerResult(result.as_ptr());
            text
        }
    }
}

impl Drop for Recognizer {
    fn drop(&mut self) {
        unsafe { sys::SherpaOnnxDestroyOfflineRecognizer(self.recognizer.as_ptr()) }
    }
}

struct Stream(NonNull<sys::SherpaOnnxOfflineStream>);

impl Drop for Stream {
    fn drop(&mut self) {
        unsafe { sys::SherpaOnnxDestroyOfflineStream(self.0.as_ptr()) }
    }
}

pub(super) fn native_error(context: &str, error: *const c_char) -> String {
    if error.is_null() {
        format!("{context}，请检查模型文件和运行日志")
    } else {
        // C++ owns thread-local storage, copied immediately before another native call.
        format!(
            "{context}：{}",
            unsafe { CStr::from_ptr(error) }.to_string_lossy()
        )
    }
}
