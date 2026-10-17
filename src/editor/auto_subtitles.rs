use super::*;
use crate::{
    asr::{self, ModelSize, Preferences},
    theme::{AnimatedButton as Button, MaterialStyle},
};
use gpui_kit::{
    component::{
        ActiveTheme, Disableable, IndexPath, WindowExt,
        button::ButtonVariants,
        select::{Select, SelectEvent, SelectItem, SelectState},
    },
    prelude::*,
};
mod download;
mod run;
#[cfg(all(test, feature = "ui-verify"))]
mod tests;

#[derive(Clone)]
struct Choice {
    id: u64,
    label: SharedString,
}

impl SelectItem for Choice {
    type Value = u64;

    fn title(&self) -> SharedString {
        self.label.clone()
    }

    fn value(&self) -> &u64 {
        &self.id
    }
}

struct AutoSubtitles {
    owner: WeakEntity<Editor>,
    tracks: Entity<SelectState<Vec<Choice>>>,
    models: Entity<SelectState<Vec<Choice>>>,
    sources: Entity<SelectState<Vec<Choice>>>,
    preferences: Preferences,
    loading: bool,
    running: bool,
    downloading: bool,
    download_progress: Option<f32>,
    help: bool,
    status: String,
    error: Option<String>,
    cancel: Arc<AtomicBool>,
    _subscriptions: Vec<Subscription>,
}

impl AutoSubtitles {
    fn copy_download_link(&mut self, auxiliary: bool, cx: &mut Context<Self>) {
        use asr::download::ModelDownload;
        let target = match (auxiliary, self.preferences.selected) {
            (true, ModelSize::SenseVoice) => ModelDownload::Vad,
            (true, _) => ModelDownload::Aligner,
            (false, model) => ModelDownload::Recognition(model),
        };
        let url = format!(
            "{}/{}/tree/main",
            self.preferences.source.endpoint(),
            target.repository()
        );
        cx.write_to_clipboard(ClipboardItem::new_string(url));
        self.status = "下载链接已复制".into();
        cx.notify();
    }

    fn import_model(&mut self, aligner: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.loading || self.running || self.downloading {
            return;
        }
        self.loading = true;
        self.error = None;
        let mut preferences = self.preferences.clone();
        let dialog = rfd::AsyncFileDialog::new().set_parent(window);
        cx.spawn_in(window, async move |this, cx| {
            let folder = dialog
                .set_title(if aligner {
                    if preferences.selected == ModelSize::SenseVoice {
                        "选择 VAD 模型文件夹"
                    } else {
                        "选择对齐模型文件夹"
                    }
                } else {
                    "选择识别模型文件夹"
                })
                .pick_folder()
                .await;
            let result = if let Some(folder) = folder {
                let path = folder.path().to_path_buf();
                Some(
                    cx.background_spawn(async move {
                        if aligner && preferences.selected == ModelSize::SenseVoice {
                            asr::validate_vad(&path)?;
                        } else {
                            asr::validate_model(&path, aligner, preferences.selected)?;
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
                        Ok::<_, String>(preferences)
                    })
                    .await,
                )
            } else {
                None
            };
            let _ = this.update(cx, |this, cx| {
                this.loading = false;
                if let Some(result) = result {
                    match result {
                        Ok(preferences) => {
                            this.preferences = preferences;
                            this.status = "模型目录已导入".into();
                        }
                        Err(error) => this.error = Some(error),
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.running || self.loading || self.downloading {
            return;
        }
        let Some(track) = self.tracks.read(cx).selected_value().copied() else {
            self.error = Some("请先在时间线上添加音频轨道并选择源轨道".into());
            cx.notify();
            return;
        };
        let auxiliary = if self.preferences.selected == ModelSize::SenseVoice {
            &self.preferences.vad
        } else {
            &self.preferences.aligner
        };
        if self.preferences.model().is_none() || auxiliary.is_none() {
            self.error = Some(
                "请先下载或导入识别模型及对应的 VAD / 对齐模型；展开“如何导入”查看步骤".into(),
            );
            cx.notify();
            return;
        }
        let form = cx.entity().downgrade();
        self.download_progress = None;
        self.cancel.store(false, Ordering::Relaxed);
        let result = self.owner.update(cx, |editor, cx| {
            if editor.busy || editor.job == Some(Job::Export) {
                return Err("编辑器正在处理其他任务".to_string());
            }
            asr::source_ranges(&editor.project, track)?;
            editor.begin_subtitles(
                track,
                self.preferences.clone(),
                form,
                self.cancel.clone(),
                window,
                cx,
            );
            Ok(())
        });
        match result {
            Ok(Ok(())) => {
                self.running = true;
                self.error = None;
                self.status = "正在准备字幕识别".into();
            }
            Ok(Err(error)) => self.error = Some(error),
            Err(error) => self.error = Some(error.to_string()),
        }
        cx.notify();
    }
}

impl Render for AutoSubtitles {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let disabled = self.loading || self.running || self.downloading;
        let sensevoice = self.preferences.selected == ModelSize::SenseVoice;
        let model_path = self
            .preferences
            .model()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "未导入识别模型".into());
        let aligner_path = (if sensevoice {
            &self.preferences.vad
        } else {
            &self.preferences.aligner
        })
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| {
            if sensevoice {
                "未导入 VAD 模型".into()
            } else {
                "未导入对齐模型".into()
            }
        });
        div()
            .id("auto-subtitles-form")
            .test_support()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div().flex().flex_col().gap_2().child("源音频轨道").child(
                    Select::new(&self.tracks)
                        .id("asr-track")
                        .accessibility_label("源音频轨道")
                        .placeholder("没有音频轨道")
                        .disabled(disabled)
                        .w_full(),
                ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child("模型下载源")
                    .child(
                        Select::new(&self.sources)
                            .id("asr-download-source")
                            .accessibility_label("模型下载源")
                            .disabled(disabled)
                            .w_full(),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("自动下载到 %LOCALAPPDATA%\\Vig\\models"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child("识别模型")
                    .child(
                        Select::new(&self.models)
                            .id("asr-model")
                            .accessibility_label("识别模型")
                            .disabled(disabled)
                            .w_full(),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                Button::new("download-asr-model")
                                    .label("下载模型")
                                    .disabled(disabled)
                                    .button_material(false)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.download_model(false, window, cx)
                                    })),
                            )
                            .child(
                                Button::new("import-asr-model")
                                    .label("导入识别模型…")
                                    .disabled(disabled)
                                    .button_material(false)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.import_model(false, window, cx)
                                    })),
                            )
                            .child(
                                Button::new("copy-asr-download-link")
                                    .label("复制下载链接")
                                    .accessibility_label("复制识别模型下载链接")
                                    .disabled(disabled)
                                    .button_material(false)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.copy_download_link(false, cx)
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(model_path),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(if sensevoice {
                        "语音分段模型 · Silero VAD"
                    } else {
                        "时间对齐模型 · Qwen3-ForcedAligner-0.6B"
                    })
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                Button::new("download-aligner-model")
                                    .label(if sensevoice {
                                        "下载 VAD 模型"
                                    } else {
                                        "下载对齐模型"
                                    })
                                    .disabled(disabled)
                                    .button_material(false)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.download_model(true, window, cx)
                                    })),
                            )
                            .child(
                                Button::new("import-aligner-model")
                                    .label(if sensevoice {
                                        "导入 VAD 模型…"
                                    } else {
                                        "导入对齐模型…"
                                    })
                                    .disabled(disabled)
                                    .button_material(false)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.import_model(true, window, cx)
                                    })),
                            )
                            .child(
                                Button::new("copy-aligner-download-link")
                                    .label("复制下载链接")
                                    .accessibility_label(if sensevoice {
                                        "复制 VAD 模型下载链接"
                                    } else {
                                        "复制对齐模型下载链接"
                                    })
                                    .disabled(disabled)
                                    .button_material(false)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.copy_download_link(true, cx)
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(aligner_path),
                    ),
            )
            .when(sensevoice, |view| {
                view.child(
                    div()
                        .id("asr-sensevoice-timing")
                        .test_support()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("SenseVoice INT8 约 239 MB，另需约 2.3 MB 的 VAD"),
                )
            })
            .child(
                Button::new("asr-import-help")
                    .label(if self.help {
                        "收起导入说明"
                    } else {
                        "如何导入"
                    })
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.help = !this.help;
                        cx.notify();
                    })),
            )
            .when(self.help, |view| {
                view.child(
                    div()
                        .id("asr-import-guide")
                        .test_support()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(asr::IMPORT_GUIDE),
                )
            })
    }
}

struct SubtitleActions {
    form: Entity<AutoSubtitles>,
    _subscription: Subscription,
}

impl Render for SubtitleActions {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.form.read(cx);
        let running = state.running;
        let downloading = state.downloading;
        let progress = state.download_progress;
        let disabled = running || state.loading || downloading;
        let status = state.status.clone();
        let error = state.error.clone();
        let close = self.form.clone();
        let generate = self.form.clone();
        let buttons = div()
            .flex()
            .justify_end()
            .gap_2()
            .child(
                Button::new("close-auto-subtitles")
                    .label(if running {
                        "停止生成"
                    } else if downloading {
                        "取消下载"
                    } else {
                        "关闭"
                    })
                    .button_material(false)
                    .on_click(move |_, window, cx| {
                        close.update(cx, |this, cx| {
                            if this.running || this.downloading {
                                this.cancel.store(true, Ordering::Relaxed);
                                this.status = if this.downloading {
                                    "正在取消下载，已下载的进度会保留".into()
                                } else {
                                    "正在停止，将在当前推理结束后取消".into()
                                };
                                cx.notify();
                            } else {
                                window.close_dialog(cx);
                            }
                        });
                    }),
            )
            .child(
                Button::new("generate-auto-subtitles")
                    .label("生成字幕")
                    .primary()
                    .button_material(true)
                    .disabled(disabled)
                    .on_click(move |_, window, cx| {
                        generate.update(cx, |form, cx| form.start(window, cx))
                    }),
            );
        div()
            .flex()
            .flex_col()
            .gap_2()
            .when_some(progress, |view, value| {
                view.child(
                    gpui_kit::component::progress::Progress::new("asr-download-progress")
                        .accessibility_label("模型下载进度")
                        .value(value),
                )
            })
            .when(!status.is_empty(), |view| {
                view.child(
                    div()
                        .id("asr-status")
                        .test_support()
                        .text_sm()
                        .child(status),
                )
            })
            .when_some(error, |view, error| {
                view.child(
                    div()
                        .id("asr-error")
                        .test_support()
                        .text_sm()
                        .text_color(cx.theme().danger)
                        .child(error),
                )
            })
            .child(buttons)
    }
}

impl Editor {
    pub fn auto_subtitles(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.job == Some(Job::Export) {
            return;
        }
        let tracks: Vec<_> = self
            .project
            .tracks
            .iter()
            .filter(|track| track.kind == TrackKind::Audio)
            .enumerate()
            .map(|(ix, track)| Choice {
                id: track.id,
                label: format!("A{} 音频 · {} 个片段", ix + 1, track.clips.len()).into(),
            })
            .collect();
        let selected = self
            .selected_track
            .and_then(|id| tracks.iter().position(|track| track.id == id))
            .or_else(|| (!tracks.is_empty()).then_some(0))
            .map(IndexPath::new);
        let tracks = cx.new(|cx| SelectState::new(tracks, selected, window, cx));
        let models = cx.new(|cx| {
            SelectState::new(
                vec![
                    Choice {
                        id: 0,
                        label: "Qwen3-ASR-0.6B · 较快".into(),
                    },
                    Choice {
                        id: 1,
                        label: "Qwen3-ASR-1.7B · 较高精度".into(),
                    },
                    Choice {
                        id: 2,
                        label: "SenseVoice Small INT8 · 轻量 · 约 239 MB".into(),
                    },
                ],
                Some(IndexPath::new(0)),
                window,
                cx,
            )
        });
        let owner = cx.entity().downgrade();
        let sources = cx.new(|cx| {
            SelectState::new(
                vec![
                    Choice {
                        id: 0,
                        label: "Hugging Face 源站 · huggingface.co".into(),
                    },
                    Choice {
                        id: 1,
                        label: "HF Mirror 镜像 · hf-mirror.com".into(),
                    },
                ],
                Some(IndexPath::new(0)),
                window,
                cx,
            )
        });
        let cancel = Arc::new(AtomicBool::new(false));
        let close_cancel = cancel.clone();
        let form = cx.new(|cx| {
            let subscription = cx.subscribe(
                &models,
                |this: &mut AutoSubtitles, _, event: &SelectEvent<Vec<Choice>>, cx| {
                    let SelectEvent::Confirm(Some(value)) = event else {
                        return;
                    };
                    this.preferences.selected = match *value {
                        0 => ModelSize::Small,
                        1 => ModelSize::Large,
                        2 => ModelSize::SenseVoice,
                        _ => unreachable!("model choices have fixed IDs"),
                    };
                    this.error = None;
                    cx.notify();
                },
            );
            let source_subscription = cx.subscribe(
                &sources,
                |this: &mut AutoSubtitles, _, event: &SelectEvent<Vec<Choice>>, cx| {
                    let SelectEvent::Confirm(Some(value)) = event else {
                        return;
                    };
                    this.preferences.source = if *value == 0 {
                        asr::download::DownloadSource::HuggingFace
                    } else {
                        asr::download::DownloadSource::Mirror
                    };
                    this.error = None;
                    cx.notify();
                },
            );
            AutoSubtitles {
                owner,
                tracks,
                models,
                sources,
                preferences: Default::default(),
                loading: true,
                running: false,
                downloading: false,
                download_progress: None,
                help: false,
                status: "正在读取模型设置".into(),
                error: None,
                cancel,
                _subscriptions: vec![subscription, source_subscription],
            }
        });
        form.update(cx, |_, cx| {
            cx.spawn_in(window, async move |this, cx| {
                let preferences = cx.background_spawn(async { Preferences::load() }).await;
                let _ = this.update_in(cx, |this, window, cx| {
                    this.loading = false;
                    this.status.clear();
                    match preferences {
                        Ok(preferences) => {
                            let id = match preferences.selected {
                                ModelSize::Small => 0,
                                ModelSize::Large => 1,
                                ModelSize::SenseVoice => 2,
                            };
                            this.models
                                .update(cx, |state, cx| state.set_selected_value(&id, window, cx));
                            let source = if preferences.source
                                == asr::download::DownloadSource::HuggingFace
                            {
                                0
                            } else {
                                1
                            };
                            this.sources.update(cx, |state, cx| {
                                state.set_selected_value(&source, window, cx)
                            });
                            this.preferences = preferences;
                        }
                        Err(error) => this.error = Some(error),
                    }
                    cx.notify();
                });
            })
            .detach();
        });
        let actions = cx.new(|cx| SubtitleActions {
            form: form.clone(),
            _subscription: cx.observe(&form, |_, _, cx| cx.notify()),
        });
        window.open_dialog(cx, move |dialog, window, _| {
            let confirm = form.clone();
            let cancel = close_cancel.clone();
            dialog
                .title("自动生成字幕")
                .width(window.rem_size() * 38.)
                .overlay_closable(false)
                .child(form.clone())
                .footer(actions.clone())
                .on_ok(move |_, window, cx| {
                    confirm.update(cx, |form, cx| form.start(window, cx));
                    false
                })
                .on_close(move |_, _, _| {
                    cancel.store(true, Ordering::Relaxed);
                })
        });
        cx.notify();
    }
}
