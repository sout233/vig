use super::*;
use crate::asr::download::{DownloadProgress, ModelDownload};

enum Message {
    Progress(DownloadProgress),
    Finished(Result<Preferences, String>),
}

impl Drop for AutoSubtitles {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl AutoSubtitles {
    pub(super) fn download_model(
        &mut self,
        aligner: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.loading || self.running || self.downloading {
            return;
        }
        self.downloading = true;
        self.download_progress = None;
        self.error = None;
        self.cancel.store(false, Ordering::Relaxed);
        self.status = format!("正在读取模型清单 · {}", self.preferences.source.endpoint());
        let mut preferences = self.preferences.clone();
        let target = if aligner {
            if preferences.selected == ModelSize::SenseVoice {
                ModelDownload::Vad
            } else {
                ModelDownload::Aligner
            }
        } else {
            ModelDownload::Recognition(preferences.selected)
        };
        let cancel = self.cancel.clone();
        let (sender, receiver) = mpsc::channel();
        cx.background_spawn(async move {
            let result = (|| {
                preferences.save()?;
                let path = asr::download::download(
                    target,
                    preferences.source,
                    &asr::download::directory()?,
                    &cancel,
                    |progress| {
                        let _ = sender.send(Message::Progress(progress));
                    },
                )?;
                if cancel.load(Ordering::Relaxed) {
                    return Err("模型下载已取消，已保留进度".into());
                }
                if aligner {
                    if preferences.selected == ModelSize::SenseVoice {
                        preferences.vad = Some(path);
                    } else {
                        preferences.aligner = Some(path);
                    }
                } else {
                    match preferences.selected {
                        ModelSize::Small => preferences.small = Some(path),
                        ModelSize::Large => preferences.large = Some(path),
                        ModelSize::SenseVoice => preferences.sensevoice = Some(path),
                    }
                }
                preferences.save()?;
                Ok(preferences)
            })();
            // Closing the dialog drops the progress receiver and cancels the worker.
            let _ = sender.send(Message::Finished(result));
        })
        .detach();
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                let mut messages = Vec::new();
                loop {
                    match receiver.try_recv() {
                        Ok(message) => {
                            let done = matches!(&message, Message::Finished(_));
                            messages.push(message);
                            if done {
                                break;
                            }
                        }
                        Err(mpsc::TryRecvError::Empty) => break,
                        Err(mpsc::TryRecvError::Disconnected) => {
                            messages.push(Message::Finished(Err(
                                "模型下载任务意外结束，请检查运行日志".into(),
                            )));
                            break;
                        }
                    }
                }
                let result = this.update(cx, |this, cx| {
                    if messages.is_empty() {
                        return false;
                    }
                    let mut finished = false;
                    for message in messages {
                        match message {
                            Message::Progress(progress) if !this.cancel.load(Ordering::Relaxed) => {
                                this.status = progress.description();
                                this.download_progress = Some(progress.percentage());
                            }
                            Message::Progress(_) => {}
                            Message::Finished(result) => {
                                finished = true;
                                this.downloading = false;
                                match result {
                                    Ok(preferences) => {
                                        this.preferences = preferences;
                                        this.status = "模型已下载、校验并自动导入".into();
                                        this.download_progress = Some(100.);
                                    }
                                    Err(error) if this.cancel.load(Ordering::Relaxed) => {
                                        this.status = error
                                    }
                                    Err(error) => {
                                        this.status =
                                            "模型下载失败，可再次点击下载继续；不会自动切换下载源"
                                                .into();
                                        this.error = Some(error);
                                    }
                                }
                            }
                        }
                    }
                    cx.notify();
                    finished
                });
                match result {
                    Ok(true) | Err(_) => break,
                    Ok(false) => {}
                }
            }
        })
        .detach();
        cx.notify();
    }
}
