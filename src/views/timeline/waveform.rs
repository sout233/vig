use super::*;
use crate::editor::waveforms::WaveGeometry;
use std::sync::Arc;

impl Editor {
    pub(super) fn audio_waveform(&self, clip: &crate::model::Clip) -> AnyElement {
        let source = &self.project.media[clip.media].path;
        let waveform = self.waveforms.get(source).cloned();
        let cache = self.waveform_geometry.get(&clip.id).cloned();
        let clip = clip.clone();
        let zoom = self.zoom;
        canvas(
            |bounds, _, _| bounds,
            move |bounds, _, window, _| {
                let (Some(waveform), Some(cache)) = (&waveform, &cache) else {
                    return;
                };
                let mask = window.content_mask().bounds;
                let left = bounds.left().max(mask.left());
                let right = bounds.right().min(mask.right());
                if left >= right || bounds.bottom() <= mask.top() || bounds.top() >= mask.bottom() {
                    return;
                }
                let bits = |value: Pixels| u64::from((value / px(1.)).to_bits());
                let key = [
                    bits(bounds.left()),
                    bits(bounds.top()),
                    bits(bounds.size.width),
                    bits(bounds.size.height),
                    bits(left),
                    bits(right),
                    u64::from(zoom.to_bits()),
                    clip.source_in.to_bits(),
                    clip.source_out.to_bits(),
                    clip.speed.to_bits(),
                    clip.volume.to_bits(),
                    clip.fade_in.to_bits(),
                    clip.fade_out.to_bits(),
                    clip.fade_in_curve as u64,
                    clip.fade_out_curve as u64,
                    u64::from(clip.muted),
                    Arc::as_ptr(waveform) as usize as u64,
                ];
                let mut cache = cache.borrow_mut();
                if cache.as_ref().is_none_or(|geometry| geometry.key != key) {
                    let mut path = PathBuilder::stroke(px(1.));
                    let center = bounds.top() + bounds.size.height / 2.;
                    let scale = (bounds.size.height / 2. - px(2.)).max(px(0.));
                    let columns = ((right - left) / px(2.)).ceil() as usize;
                    for column in 0..columns {
                        let x = left + px(column as f32 * 2.);
                        let offset = f64::from((x - bounds.left()) / px(zoom));
                        let end = (clip.source_in + (offset + 2. / f64::from(zoom)) * clip.speed)
                            .min(clip.source_out);
                        let peak = waveform.peak(clip.source_in + offset * clip.speed, end);
                        let gain = if clip.muted {
                            0.
                        } else {
                            clip.volume * clip.brightness(offset)
                        } as f32;
                        let top = center - scale * (peak.max * gain).clamp(-1., 1.);
                        let bottom =
                            (center - scale * (peak.min * gain).clamp(-1., 1.)).max(top + px(0.6));
                        path.move_to(point(x, top));
                        path.line_to(point(x, bottom));
                    }
                    *cache = Some(WaveGeometry {
                        key,
                        path: path.build().expect("无法绘制音频波形"),
                    });
                }
                window.paint_path(cache.as_ref().unwrap().path.clone(), rgba(0xffffff99));
            },
        )
        .absolute()
        .left_0()
        .right_0()
        .top(px(22.))
        .bottom(px(3.))
        .into_any_element()
    }
}

#[cfg(all(test, feature = "ui-verify"))]
mod tests {
    use crate::{editor::Editor, media, theme};
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{
        AppContext, Bounds, TestAppContext, WindowBounds, WindowOptions, point, px, size,
    };

    #[gpui_kit::test]
    fn audio_waveform_loads_asynchronously_and_reuses_geometry(cx: &mut TestAppContext) {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("tone.wav");
        let result = media::command("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:sample_rate=48000:duration=30",
            ])
            .arg(&source)
            .output()
            .unwrap();
        assert!(result.status.success());
        let (handle, editor, id) = cx.update(|cx| {
            gpui_kit::init(cx);
            theme::apply(cx);
            let (handle, editor) = gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        point(px(0.), px(0.)),
                        size(px(1380.), px(900.)),
                    ))),
                    ..Default::default()
                },
                cx,
                |window, cx| cx.new(|cx| Editor::new(window, cx)),
            )
            .unwrap();
            let id = editor.update(cx, |this, cx| {
                this.project.media.push(media::probe(&source).unwrap());
                let id = this.project.append(0);
                this.project.duplicate(id);
                this.load_waveforms(cx);
                cx.notify();
                id
            });
            (handle, editor, id)
        });
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            assert!(editor.read(cx).waveforms.contains_key(&source));
            window.render_frame(cx);
            let cache = editor.read(cx).waveform_geometry[&id].clone();
            let (key, pointer) = {
                let geometry = cache.borrow();
                let geometry = geometry.as_ref().unwrap();
                assert!(!geometry.path.vertices.is_empty());
                (geometry.key, geometry.path.vertices.as_ptr())
            };
            window.render_frame(cx);
            assert_eq!(
                cache.borrow().as_ref().unwrap().path.vertices.as_ptr(),
                pointer,
                "重复重绘不能重新构建波形几何"
            );
            editor.update(cx, |this, cx| {
                this.zoom = 400.;
                cx.notify();
            });
            window.render_frame(cx);
            assert_ne!(
                cache.borrow().as_ref().unwrap().key,
                key,
                "缩放后必须重新采样可见范围"
            );
            assert!(
                cache.borrow().as_ref().unwrap().path.bounds.right()
                    <= editor.read(cx).timeline_scroll.bounds().right() + px(1.),
                "长片段必须只构建视口内的波形几何"
            );
            assert_eq!(editor.read(cx).waveforms.len(), 1, "缩放必须复用解码结果");
            editor.update(cx, |this, cx| {
                this.project
                    .tracks
                    .iter_mut()
                    .for_each(|track| track.clips.clear());
                this.load_waveforms(cx);
            });
            assert!(
                editor.read(cx).waveforms.is_empty(),
                "删除最后一个引用后释放峰值缓存"
            );
            assert!(editor.read(cx).waveform_geometry.is_empty());
        })
        .unwrap();
    }
}
