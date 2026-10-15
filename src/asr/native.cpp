// sherpa-onnx's C exports can throw while loading malformed ONNX graphs.
// Catch in C++ before an exception reaches Rust; never fabricate a result.
#include <exception>
#include <string>

extern "C" {
const void* SherpaOnnxCreateVoiceActivityDetector(const void*, float);
const void* SherpaOnnxCreateOfflineRecognizer(const void*);
void SherpaOnnxDecodeOfflineStream(const void*, const void*);
}

static thread_local std::string last_error;

extern "C" const void* vig_create_vad(const void* config, float seconds, const char** error) noexcept {
    try {
        return SherpaOnnxCreateVoiceActivityDetector(config, seconds);
    } catch (const std::exception& exception) {
        last_error = exception.what();
    } catch (...) {
        last_error = "Unknown native VAD exception";
    }
    *error = last_error.c_str();
    return nullptr;
}

extern "C" const void* vig_create_recognizer(const void* config, const char** error) noexcept {
    try {
        return SherpaOnnxCreateOfflineRecognizer(config);
    } catch (const std::exception& exception) {
        last_error = exception.what();
    } catch (...) {
        last_error = "Unknown native SenseVoice exception";
    }
    *error = last_error.c_str();
    return nullptr;
}

extern "C" bool vig_decode(const void* recognizer, const void* stream, const char** error) noexcept {
    try {
        SherpaOnnxDecodeOfflineStream(recognizer, stream);
        return true;
    } catch (const std::exception& exception) {
        last_error = exception.what();
    } catch (...) {
        last_error = "Unknown native SenseVoice decoding exception";
    }
    *error = last_error.c_str();
    return false;
}
