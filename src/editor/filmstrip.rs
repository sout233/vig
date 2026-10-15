use super::*;

impl Editor {
    pub fn load_filmstrip(&mut self, cx: &mut Context<Self>) {
        self.load_waveforms(cx);
        let mut requests = Vec::new();
        for clip in self.project.video() {
            if self.project.media[clip.media].generator.is_some() {
                continue;
            }
            let path = &self.project.media[clip.media].path;
            for time in clip.preview_times() {
                let key = (path.clone(), time);
                if !self.filmstrip.contains_key(&key) && self.pending_frames.insert(key.clone()) {
                    requests.push(key);
                }
            }
        }
        if requests.is_empty() {
            return;
        }
        cx.spawn(async move |this, cx| {
            let results = cx
                .background_spawn(async move {
                    requests
                        .into_iter()
                        .map(|key| {
                            let result = (|| {
                                let bytes = media::frame(&key.0, key.1 as f64 / 1_000_000.)?;
                                let image = image::load_from_memory(&bytes)
                                    .map_err(|e| e.to_string())?
                                    .into_rgba8();
                                let image = image::imageops::resize(
                                    &image,
                                    160,
                                    90,
                                    image::imageops::FilterType::Triangle,
                                );
                                Ok::<_, String>(render_image(image))
                            })();
                            (key, result)
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                for (key, result) in results {
                    this.pending_frames.remove(&key);
                    if this.project.media.iter().any(|m| m.path == key.0) {
                        match result {
                            Ok(image) => {
                                this.filmstrip.insert(key, image);
                            }
                            Err(error) => {
                                this.error = Some(format!("时间线预览图生成失败：{error}"));
                            }
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}
