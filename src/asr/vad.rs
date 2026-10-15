//! Minimal checked ownership of the sherpa-onnx VAD C API.
//! The upstream high-level constructor does not check its returned pointer.
use sherpa_rs::sherpa_rs_sys as sys;
use std::{
    ffi::{CString, c_char, c_void},
    path::Path,
    ptr::NonNull,
};

unsafe extern "C" {
    fn vig_create_vad(
        config: *const c_void,
        seconds: f32,
        error: *mut *const c_char,
    ) -> *const sys::SherpaOnnxVoiceActivityDetector;
}

pub(super) struct Vad {
    detector: NonNull<sys::SherpaOnnxVoiceActivityDetector>,
}

impl Vad {
    pub(super) fn new(directory: &Path) -> Result<Self, String> {
        let path = directory.join("silero_vad.onnx");
        let model = CString::new(path.to_str().ok_or("VAD 路径不是有效 UTF-8")?)
            .map_err(|e| e.to_string())?;
        let provider = CString::new("cpu").unwrap();
        // Zero initializes unused model slots in the upstream C configuration.
        let mut config: sys::SherpaOnnxVadModelConfig = unsafe { std::mem::zeroed() };
        config.silero_vad = sys::SherpaOnnxSileroVadModelConfig {
            model: model.as_ptr(),
            threshold: 0.5,
            min_silence_duration: 0.4,
            min_speech_duration: 0.25,
            window_size: 512,
            // Split continuous speech at the ASR processing window to bound memory.
            max_speech_duration: 30.,
        };
        config.sample_rate = 16000;
        config.num_threads = 1;
        config.provider = provider.as_ptr();
        let mut error = std::ptr::null();
        let detector = unsafe {
            vig_create_vad(
                (&config as *const sys::SherpaOnnxVadModelConfig).cast(),
                60.,
                &mut error,
            )
        };
        let detector = NonNull::new(detector.cast_mut())
            .ok_or_else(|| super::recognizer::native_error("无法加载 Silero VAD", error))?;
        Ok(Self { detector })
    }

    pub(super) fn accept(&mut self, samples: &[f32]) {
        // Callers feed small render blocks, so the C sample count fits i32.
        unsafe {
            sys::SherpaOnnxVoiceActivityDetectorAcceptWaveform(
                self.detector.as_ptr(),
                samples.as_ptr(),
                samples.len().try_into().unwrap(),
            );
        }
    }

    pub(super) fn flush(&mut self) {
        unsafe { sys::SherpaOnnxVoiceActivityDetectorFlush(self.detector.as_ptr()) }
    }

    pub(super) fn pop(&mut self) -> Result<Option<(usize, Vec<f32>)>, String> {
        unsafe {
            if sys::SherpaOnnxVoiceActivityDetectorEmpty(self.detector.as_ptr()) != 0 {
                return Ok(None);
            }
            let segment = sys::SherpaOnnxVoiceActivityDetectorFront(self.detector.as_ptr());
            let segment = NonNull::new(segment.cast_mut()).ok_or("VAD 没有返回语音区间")?;
            let raw = segment.as_ref();
            let result = if raw.start < 0 || raw.n <= 0 || raw.samples.is_null() {
                Err("VAD 返回了无效语音区间".into())
            } else {
                Ok(Some((
                    raw.start as usize,
                    std::slice::from_raw_parts(raw.samples, raw.n as usize).to_vec(),
                )))
            };
            sys::SherpaOnnxDestroySpeechSegment(segment.as_ptr());
            sys::SherpaOnnxVoiceActivityDetectorPop(self.detector.as_ptr());
            result
        }
    }
}

impl Drop for Vad {
    fn drop(&mut self) {
        unsafe { sys::SherpaOnnxDestroyVoiceActivityDetector(self.detector.as_ptr()) }
    }
}
