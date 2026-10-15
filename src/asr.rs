//! Offline subtitle generation; models are loaded only after an explicit request.
mod captions;
pub mod download;
mod inference;
mod recognizer;
mod sensevoice;
mod settings;
#[cfg(test)]
mod tests;
mod vad;
pub use captions::{Caption, insert_captions};
pub use inference::{AsrEngine, generate, source_ranges};
pub use sensevoice::validate_vad;
pub use settings::{ModelSize, Preferences, validate_model};

pub const IMPORT_GUIDE: &str = "自动下载：\n1. 选择 Hugging Face 源站或 HF Mirror 镜像\n2. SenseVoice Small INT8：点击“下载模型”和“下载 VAD 模型”，合计约 242 MB\n3. Qwen3-ASR：点击“下载模型”和“下载对齐模型”\n4. 文件自动存入 %LOCALAPPDATA%\\Vig\\models，校验后自动导入\n5. 全部就绪后选择音频轨道，点击“生成字幕”\n手动导入：\nSenseVoice：将 model.int8.onnx 和 tokens.txt 放在同一文件夹，点击“导入识别模型…”；将 silero_vad.onnx 放在另一文件夹，点击“导入 VAD 模型…”\nQwen3：将 config.json、全部 .safetensors 权重、权重索引（如有）、tokenizer_config.json，以及 tokenizer.json 或 vocab.json + merges.txt 放在同一文件夹，再点击对应“导入…”按钮\n识别在本机 CPU 上运行";
