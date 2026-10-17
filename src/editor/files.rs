use super::*;

impl Editor {
    pub fn new_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.close_color_editor(cx);
        self.busy = true;
        let dirty = self.dirty;
        let dialog = rfd::AsyncMessageDialog::new().set_parent(window);
        cx.spawn_in(window, async move |this, cx| {
            let proceed = !dirty
                || dialog
                    .set_title("新建项目")
                    .set_description("新建项目将丢弃当前未保存的更改，是否继续？")
                    .set_buttons(rfd::MessageButtons::YesNo)
                    .show()
                    .await
                    == rfd::MessageDialogResult::Yes;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                if proceed {
                    this.stop();
                    this.project = Project::default();
                    this.selected = None;
                    this.selection.clear();
                    this.marquee = None;
                    this.number_edit = None;
                    this.number_drag = None;
                    this.selected_track = None;
                    this.drag = None;
                    this.track_drag = None;
                    this.track_drop_slot = None;
                    this.track_resize = None;
                    this.track_heights.clear();
                    this.position = 0.;
                    this.image = None;
                    this.thumbnails.clear();
                    this.filmstrip.clear();
                    this.waveforms.clear();
                    this.waveform_geometry.clear();
                    this.load_waveforms(cx);
                    this.undo.clear();
                    this.redo.clear();
                    this.path = None;
                    this.dirty = false;
                    this.frame_generation += 1;
                    this.pending_frame = None;
                    this.timeline_drag = None;
                    this.timeline_motion = None;
                    this.timeline_scroll.set_offset(point(px(0.), px(0.)));
                    this.track_labels_scroll.set_offset(point(px(0.), px(0.)));
                    this.query.clear();
                    this.search.update(cx, |s, cx| s.set_value("", window, cx));
                    this.error = None;
                    this.status = "新项目已创建".into();
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn import(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.import_media(None, false, None, window, cx);
    }

    pub fn import_media(
        &mut self,
        paths: Option<Vec<PathBuf>>,
        append: bool,
        placement: Option<(u64, f64)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy || self.job == Some(Job::Export) {
            self.error = Some("正在处理文件或导出，请完成后再导入素材".into());
            cx.notify();
            return;
        }
        self.busy = true;
        self.error = None;
        cx.notify();
        let dialog = rfd::AsyncFileDialog::new().set_parent(window);
        cx.spawn_in(window, async move |this, cx| {
            let paths = if let Some(paths) = paths {
                Some(paths)
            } else {
                dialog
                    .set_title("导入视频与音频")
                    .add_filter(
                        "视频 / 音频",
                        &[
                            "mp4", "mov", "mkv", "webm", "avi", "m4v", "mp3", "wav", "flac", "m4a",
                            "ogg",
                        ],
                    )
                    .pick_files()
                    .await
                    .map(|files| files.into_iter().map(|f| f.path().to_owned()).collect())
            };
            let result = if let Some(paths) = paths {
                Some(
                    cx.background_spawn(async move {
                        paths
                            .into_iter()
                            .map(|path| {
                                let media = media::probe(&path)
                                    .map_err(|e| format!("{}：{e}", path.display()))?;
                                let thumbnail = if media.width > 0 {
                                    Some(render_image(
                                        image::load_from_memory(&media::frame(&path, 0.)?)
                                            .map_err(|e| e.to_string())?
                                            .into_rgba8(),
                                    ))
                                } else {
                                    None
                                };
                                Ok::<_, String>((media, thumbnail))
                            })
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .await,
                )
            } else {
                None
            };
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Some(Ok(items)) => {
                        let mut next = this.project.clone();
                        let mut thumbs = Vec::new();
                        let mut selected = this.selected;
                        let mut placement = placement;
                        let mut failure = None;
                        let mut count = 0;
                        for (media, thumb) in items {
                            let index = if let Some(index) =
                                next.media.iter().position(|m| m.path == media.path)
                            {
                                index
                            } else {
                                if let Some(thumb) = thumb {
                                    thumbs.push((media.path.clone(), thumb));
                                }
                                next.media.push(media);
                                count += 1;
                                next.media.len() - 1
                            };
                            if append {
                                let id = match next.insert_media(
                                    index,
                                    placement.map(|p| p.0).or(this.selected_track),
                                    placement.map(|p| p.1),
                                ) {
                                    Ok(id) => id,
                                    Err(error) => {
                                        failure = Some(error);
                                        break;
                                    }
                                };
                                if let Some((track, _)) = placement {
                                    placement = Some((track, next.clip(id).unwrap().end()));
                                }
                                selected = Some(id);
                            }
                        }
                        if let Some(error) = failure {
                            this.error = Some(error);
                            cx.notify();
                            return;
                        }
                        let match_video = next.first_video_match(&this.project);
                        this.checkpoint();
                        this.project = next;
                        this.selected = selected;
                        if let Some((track, _)) = placement {
                            this.selected_track = Some(track);
                        }
                        this.thumbnails.extend(thumbs);
                        this.status = if append {
                            format!("拖入完成，新增 {count} 个素材")
                        } else {
                            format!("已导入 {count} 个素材")
                        };
                        this.changed(window, cx);
                        this.offer_first_video_settings(match_video, window, cx);
                    }
                    Some(Err(e)) => this.error = Some(e),
                    None => {}
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.close_color_editor(cx);
        self.busy = true;
        let existing = self.path.clone();
        let mut project = self.project.clone();
        let dialog = rfd::AsyncFileDialog::new().set_parent(window);
        cx.spawn_in(window, async move |this, cx| {
            let path = if let Some(path) = existing {
                Some(path)
            } else {
                dialog
                    .set_title("保存项目")
                    .set_file_name("我的剪辑.vig")
                    .add_filter("Vig 项目", &["vig"])
                    .save_file()
                    .await
                    .map(|f| f.path().to_owned())
            };
            let result = if let Some(path) = path {
                project.name = path.file_stem().unwrap().to_string_lossy().into_owned();
                Some(
                    cx.background_spawn(async move {
                        project.validate()?;
                        if project.media.iter().any(|m| m.path == path) {
                            return Err("项目文件不能覆盖源素材".into());
                        }
                        let bytes =
                            serde_json::to_vec_pretty(&project).map_err(|e| e.to_string())?;
                        use std::io::Write;
                        let mut temp =
                            tempfile::NamedTempFile::new_in(path.parent().ok_or("保存目录无效")?)
                                .map_err(|e| e.to_string())?;
                        temp.write_all(&bytes).map_err(|e| e.to_string())?;
                        temp.as_file().sync_all().map_err(|e| e.to_string())?;
                        temp.persist(&path).map_err(|e| e.to_string())?;
                        Ok::<_, String>((path, project.name))
                    })
                    .await,
                )
            } else {
                None
            };
            let _ = this.update_in(cx, |this, _, cx| {
                this.busy = false;
                match result {
                    Some(Ok((path, name))) => {
                        this.path = Some(path);
                        this.project.name = name;
                        this.dirty = false;
                        this.status = "项目已保存".into();
                    }
                    Some(Err(e)) => this.error = Some(e),
                    None => {}
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.close_color_editor(cx);
        self.busy = true;
        let dirty = self.dirty;
        self.stop();
        let confirm = rfd::AsyncMessageDialog::new().set_parent(window);
        let picker = rfd::AsyncFileDialog::new().set_parent(window);
        cx.spawn_in(window, async move |this, cx| {
            let proceed = !dirty
                || confirm
                    .set_title("尚有未保存的更改")
                    .set_description("打开其他项目将丢弃当前未保存的更改，是否继续？")
                    .set_buttons(rfd::MessageButtons::YesNo)
                    .show()
                    .await
                    == rfd::MessageDialogResult::Yes;
            let file = if proceed {
                picker.add_filter("Vig 项目", &["vig"]).pick_file().await
            } else {
                None
            };
            let result = if let Some(file) = file {
                Some(
                    cx.background_spawn(async move {
                        let path = file.path().to_owned();
                        let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
                        let project =
                            Project::from_json(&text).map_err(|e| format!("项目文件无效：{e}"))?;
                        project.validate()?;
                        let thumbs = project
                            .media
                            .iter()
                            .map(|m| {
                                if m.width > 0 && m.generator.is_none() {
                                    Ok(Some(render_image(
                                        image::load_from_memory(&media::frame(&m.path, 0.)?)
                                            .map_err(|e| e.to_string())?
                                            .into_rgba8(),
                                    )))
                                } else {
                                    Ok(None)
                                }
                            })
                            .collect::<Result<Vec<_>, String>>()?;
                        Ok::<_, String>((project, path, thumbs))
                    })
                    .await,
                )
            } else {
                None
            };
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Some(Ok((project, path, thumbs))) => {
                        this.thumbnails = project
                            .media
                            .iter()
                            .zip(thumbs)
                            .filter_map(|(m, t)| t.map(|t| (m.path.clone(), t)))
                            .collect();
                        this.project = project;
                        this.path = Some(path);
                        this.undo.clear();
                        this.redo.clear();
                        this.selected = None;
                        this.selection.clear();
                        this.marquee = None;
                        this.number_edit = None;
                        this.number_drag = None;
                        this.selected_track = None;
                        this.drag = None;
                        this.track_drag = None;
                        this.track_drop_slot = None;
                        this.track_resize = None;
                        this.track_heights.clear();
                        this.dirty = false;
                        this.position = 0.;
                        this.timeline_drag = None;
                        this.timeline_motion = None;
                        this.timeline_scroll.set_offset(point(px(0.), px(0.)));
                        this.track_labels_scroll.set_offset(point(px(0.), px(0.)));
                        this.refresh_fields(window, cx);
                        this.seek(0., cx);
                        this.load_filmstrip(cx);
                        this.status = "项目已打开".into();
                    }
                    Some(Err(e)) => this.error = Some(e),
                    None => {}
                }
                cx.notify();
            });
        })
        .detach();
    }
}
