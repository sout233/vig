//! Managed, revision-pinned downloads. Source selection never changes implicitly.
mod manifest;
#[cfg(test)]
mod tests;
mod transfer;
use super::ModelSize;
use serde::{Deserialize, Serialize};
use std::{
    future::Future,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DownloadSource {
    #[default]
    HuggingFace,
    Mirror,
}

impl DownloadSource {
    pub fn endpoint(self) -> &'static str {
        match self {
            Self::HuggingFace => "https://huggingface.co",
            Self::Mirror => "https://hf-mirror.com",
        }
    }
}

#[derive(Clone, Copy)]
pub enum ModelDownload {
    Recognition(ModelSize),
    Aligner,
    Vad,
}

impl ModelDownload {
    pub fn repository(self) -> &'static str {
        match self {
            Self::Recognition(size) => size.repository(),
            Self::Aligner => "Qwen/Qwen3-ForcedAligner-0.6B",
            Self::Vad => "istupakov/silero-vad-onnx",
        }
    }

    fn files(self) -> Option<&'static [&'static str]> {
        match self {
            Self::Recognition(ModelSize::SenseVoice) => {
                Some(&["model.int8.onnx", "tokens.txt", "LICENSE"])
            }
            Self::Vad => Some(&["silero_vad.onnx", "LICENSE.txt"]),
            _ => None,
        }
    }
}

#[derive(Clone)]
pub struct DownloadProgress {
    completed: u64,
    total: u64,
    speed: f64,
    file: String,
    verifying: bool,
}

impl DownloadProgress {
    pub fn percentage(&self) -> f32 {
        (self.completed as f64 / self.total as f64 * 100.) as f32
    }

    pub fn description(&self) -> String {
        format!(
            "{} {:.1}% · {:.1} / {:.1} MiB · {:.1} MiB/s\n{}",
            if self.verifying {
                "正在校验"
            } else {
                "正在下载"
            },
            self.percentage(),
            self.completed as f64 / 1048576.,
            self.total as f64 / 1048576.,
            self.speed / 1048576.,
            self.file
        )
    }
}

struct Progress<'a> {
    completed: u64,
    total: u64,
    transferred: u64,
    started: Instant,
    last_report: Instant,
    callback: &'a mut dyn FnMut(DownloadProgress),
}

impl Progress<'_> {
    fn report(&mut self, file: &str, verifying: bool, force: bool) {
        if !force && self.last_report.elapsed() < Duration::from_millis(200) {
            return;
        }
        (self.callback)(DownloadProgress {
            completed: self.completed,
            total: self.total,
            speed: self.transferred as f64 / self.started.elapsed().as_secs_f64(),
            file: file.into(),
            verifying,
        });
        self.last_report = Instant::now();
    }
}

fn check_cancel(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err("模型下载已取消，已保留进度，下次点击下载可继续".into())
    } else {
        Ok(())
    }
}

async fn request<T>(
    future: impl Future<Output = Result<T, reqwest::Error>>,
    cancel: &AtomicBool,
) -> Result<T, String> {
    let mut future = Box::pin(future);
    loop {
        check_cancel(cancel)?;
        match tokio::time::timeout(Duration::from_millis(100), &mut future).await {
            Ok(result) => {
                check_cancel(cancel)?;
                return result.map_err(|e| format!("模型下载网络请求失败：{e}"));
            }
            Err(_) => continue,
        }
    }
}

pub fn directory() -> Result<PathBuf, String> {
    Ok(
        PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or("无法确定模型下载目录")?)
            .join("Vig/models"),
    )
}

pub fn download(
    target: ModelDownload,
    source: DownloadSource,
    root: &Path,
    cancel: &AtomicBool,
    mut progress: impl FnMut(DownloadProgress),
) -> Result<PathBuf, String> {
    download_from(target, source.endpoint(), root, cancel, &mut progress)
}

fn download_from(
    target: ModelDownload,
    endpoint: &str,
    root: &Path,
    cancel: &AtomicBool,
    callback: &mut dyn FnMut(DownloadProgress),
) -> Result<PathBuf, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    runtime.block_on(async {
        check_cancel(cancel)?;
        let client = reqwest::Client::builder()
            .use_rustls_tls()
            .connect_timeout(Duration::from_secs(30))
            .read_timeout(Duration::from_secs(30))
            .user_agent("Vig/0.1 model-downloader")
            .build()
            .map_err(|e| e.to_string())?;
        let repository = target.repository();
        let response = request(
            client
                .get(format!(
                    "{endpoint}/api/models/{repository}/revision/main?blobs=true"
                ))
                .send(),
            cancel,
        )
        .await?
        .error_for_status()
        .map_err(|e| format!("无法读取模型清单：{e}"))?;
        let metadata = request(response.json::<manifest::Repository>(), cancel).await?;
        let (revision, files) = metadata.selected_files(target.files())?;
        let destination = root.join(repository).join(&revision);
        std::fs::create_dir_all(&destination).map_err(|e| format!("无法创建下载目录：{e}"))?;
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(destination.join(".download.lock"))
            .map_err(|e| e.to_string())?;
        loop {
            check_cancel(cancel)?;
            match fs2::FileExt::try_lock_exclusive(&lock) {
                Ok(()) => break,
                Err(e) if e.raw_os_error() == fs2::lock_contended_error().raw_os_error() => {
                    tokio::time::sleep(Duration::from_millis(100)).await
                }
                Err(e) => return Err(format!("无法锁定模型下载目录：{e}")),
            }
        }
        let mut progress = Progress {
            completed: 0,
            total: files.iter().map(|file| file.size).sum(),
            transferred: 0,
            started: Instant::now(),
            last_report: Instant::now(),
            callback,
        };
        for file in files {
            transfer::download_file(
                &client,
                endpoint,
                repository,
                &revision,
                &destination,
                &file,
                cancel,
                &mut progress,
            )
            .await?;
        }
        check_cancel(cancel)?;
        match target {
            ModelDownload::Recognition(size) => super::validate_model(&destination, false, size)?,
            ModelDownload::Aligner => super::validate_model(&destination, true, ModelSize::Small)?,
            ModelDownload::Vad => super::sensevoice::validate_vad(&destination)?,
        }
        Ok(destination)
    })
}
