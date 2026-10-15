use super::*;
use std::{cell::RefCell, rc::Rc};

pub struct WaveGeometry {
    pub key: [u64; 17],
    pub path: gpui_kit::Path<Pixels>,
}

pub type WaveGeometryCache = Rc<RefCell<Option<WaveGeometry>>>;

impl Editor {
    pub fn load_waveforms(&mut self, cx: &mut Context<Self>) {
        let audio: Vec<_> = self
            .project
            .tracks
            .iter()
            .filter(|track| track.kind == TrackKind::Audio)
            .flat_map(|track| track.clips.iter())
            .map(|clip| (clip.id, self.project.media[clip.media].path.clone()))
            .collect();
        let ids: std::collections::HashSet<_> = audio.iter().map(|(id, _)| *id).collect();
        let sources: std::collections::HashSet<_> =
            audio.iter().map(|(_, path)| path.clone()).collect();
        self.waveform_geometry.retain(|id, _| ids.contains(id));
        self.waveforms.retain(|path, _| sources.contains(path));
        self.waveform_queue.retain(|path| sources.contains(path));
        self.pending_waveforms
            .retain(|path| sources.contains(path) || self.waveform_current.as_ref() == Some(path));
        if self
            .waveform_current
            .as_ref()
            .is_some_and(|path| !sources.contains(path))
        {
            self.waveform_cancel.store(true, Ordering::Relaxed);
        }
        for (id, path) in audio {
            self.waveform_geometry.entry(id).or_default();
            if !self.waveforms.contains_key(&path) && self.pending_waveforms.insert(path.clone()) {
                self.waveform_queue.push_back(path);
            }
        }
        if self.waveform_loading || self.waveform_queue.is_empty() {
            return;
        }
        self.waveform_loading = true;
        cx.spawn(async move |this, cx| {
            loop {
                let (path, cancel) = match this.update(cx, |this, _| {
                    let path = this.waveform_queue.pop_front();
                    this.waveform_current = path.clone();
                    if path.is_none() {
                        this.waveform_loading = false;
                    }
                    path.map(|path| {
                        this.waveform_cancel = Arc::new(AtomicBool::new(false));
                        (path, this.waveform_cancel.clone())
                    })
                }) {
                    Ok(Some(path)) => path,
                    _ => break,
                };
                let source = path.clone();
                let decoder_cancel = cancel.clone();
                let result = cx
                    .background_spawn(async move {
                        media::waveform::generate(&source, &decoder_cancel).map(Arc::new)
                    })
                    .await;
                if this
                    .update(cx, |this, cx| {
                        this.pending_waveforms.remove(&path);
                        this.waveform_current = None;
                        // Removing the last event cancels its analysis; re-importing can request it again.
                        if cancel.load(Ordering::Relaxed) {
                            this.load_waveforms(cx);
                            return;
                        }
                        if this
                            .project
                            .tracks
                            .iter()
                            .filter(|track| track.kind == TrackKind::Audio)
                            .flat_map(|track| &track.clips)
                            .any(|clip| this.project.media[clip.media].path == path)
                        {
                            match result {
                                Ok(waveform) => {
                                    this.waveforms.insert(path, waveform);
                                }
                                Err(error) => {
                                    this.error = Some(format!(
                                        "音频波形生成失败：{}：{error}",
                                        path.display()
                                    ));
                                }
                            }
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }
}
