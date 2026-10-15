use super::*;

impl Editor {
    pub(super) fn sync_preview_settings(&mut self) {
        if self.preview_cache.settings() != self.project.settings() {
            self.preload_cancel.store(true, Ordering::Relaxed);
            self.preview_cache = Arc::new(media::preview::PreviewCache::new(
                self._temp.path().join("segments"),
                self.preview_resolution(),
                self.project.settings().clone(),
            ));
        }
    }

    pub fn preview_resolution(&self) -> media::preview::PreviewResolution {
        self.preview_cache.resolution()
    }

    pub fn set_preview_resolution(
        &mut self,
        resolution: media::preview::PreviewResolution,
        cx: &mut Context<Self>,
    ) {
        if self.preview_resolution() == resolution {
            return;
        }
        let playing = self.job == Some(Job::Play);
        if playing {
            self.stop();
        }
        self.preload_cancel.store(true, Ordering::Relaxed);
        self.preview_cache = Arc::new(media::preview::PreviewCache::new(
            self._temp.path().join("segments"),
            resolution,
            self.project.settings().clone(),
        ));
        self.seek(self.position, cx);
        if playing {
            self.play(cx);
        }
    }

    /// Debounce scrubbing, then preload only the parked playhead's segment.
    pub(super) fn warm_preview(&mut self, cx: &mut Context<Self>) {
        self.preload_cancel.store(true, Ordering::Relaxed);
        self.preload_cancel = Arc::new(AtomicBool::new(false));
        if self.position >= self.project.duration() {
            return;
        }
        let cancel = self.preload_cancel.clone();
        let project = self.project.clone();
        let cache = self.preview_cache.clone();
        let index = (self.position / media::preview::SEGMENT_SECONDS).floor() as u64;
        cx.spawn(async move |this, cx| {
            let worker_cancel = cancel.clone();
            let result = cx
                .background_spawn(async move {
                    std::thread::sleep(Duration::from_millis(150));
                    if worker_cancel.load(Ordering::Relaxed) {
                        return Ok(());
                    }
                    cache.segment(&project, index, worker_cancel).map(|_| ())
                })
                .await;
            if !cancel.load(Ordering::Relaxed) {
                let _ = this.update(cx, |this, cx| {
                    if let Err(error) = result {
                        this.error = Some(format!("预览预加载失败：{error}"));
                        cx.notify();
                    }
                });
            }
        })
        .detach();
    }
}
