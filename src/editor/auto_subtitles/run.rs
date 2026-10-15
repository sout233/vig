use super::*;

enum Update {
    Status(String),
    Finished(Result<Vec<asr::Caption>, String>),
}

impl Editor {
    pub(super) fn begin_subtitles(
        &mut self,
        track: u64,
        preferences: Preferences,
        form: WeakEntity<AutoSubtitles>,
        cancel: Arc<AtomicBool>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.stop();
        self.busy = true;
        self.status = "正在准备字幕识别".into();
        let project = self.project.clone();
        let expected = serde_json::to_string(&project).expect("工程必须可序列化");
        let engine = self.asr_engine.clone();
        let worker_cancel = cancel.clone();
        let (sender, receiver) = mpsc::channel();
        cx.background_spawn(async move {
            let result = asr::generate(
                &engine,
                project,
                track,
                preferences,
                &worker_cancel,
                |status| {
                    // A closed editor no longer has a receiver for progress or completion.
                    let _ = sender.send(Update::Status(status));
                },
            );
            let _ = sender.send(Update::Finished(result));
        })
        .detach();
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                let updates: Vec<_> = receiver.try_iter().collect();
                let result = this.update_in(cx, |this, window, cx| {
                    if updates.is_empty() {
                        return false;
                    }
                    let mut finished = false;
                    for update in updates {
                        match update {
                            Update::Status(status) if !cancel.load(Ordering::Relaxed) => {
                                this.status = status.clone();
                                let _ = form.update(cx, |form, cx| {
                                    form.status = status;
                                    cx.notify();
                                });
                            }
                            Update::Status(_) => {}
                            Update::Finished(result) => {
                                finished = true;
                                this.busy = false;
                                let result = if cancel.load(Ordering::Relaxed) {
                                    Err("字幕生成已取消".into())
                                } else {
                                    result.and_then(|captions| {
                                        if serde_json::to_string(&this.project)
                                            .map_err(|e| e.to_string())?
                                            != expected
                                        {
                                            return Err(
                                                "识别期间工程已发生变化，字幕未写入，请重新生成"
                                                    .into(),
                                            );
                                        }
                                        let count = captions.len();
                                        let (next, track) =
                                            asr::insert_captions(&this.project, &captions)?;
                                        this.checkpoint();
                                        this.project = next;
                                        this.selected_track = Some(track);
                                        this.selected = this.project.tracks[0]
                                            .clips
                                            .first()
                                            .map(|clip| clip.id);
                                        this.selection = this.selected.into_iter().collect();
                                        this.changed(window, cx);
                                        Ok(count)
                                    })
                                };
                                let (status, error) = match result {
                                    Ok(count) => {
                                        (format!("已生成 {count} 条字幕，可使用 Ctrl+Z 撤销"), None)
                                    }
                                    Err(error) if cancel.load(Ordering::Relaxed) => (error, None),
                                    Err(error) => ("自动字幕生成失败".into(), Some(error)),
                                };
                                this.status = status.clone();
                                this.error = error.clone();
                                let _ = form.update(cx, |form, cx| {
                                    form.running = false;
                                    form.status = status;
                                    form.error = error;
                                    cx.notify();
                                });
                            }
                        }
                    }
                    cx.notify();
                    finished
                });
                match result {
                    Ok(true) => break,
                    Ok(false) => {}
                    Err(_) => {
                        cancel.store(true, Ordering::Relaxed);
                        break;
                    }
                }
            }
        })
        .detach();
        cx.notify();
    }
}
