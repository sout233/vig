use super::*;
use std::time::Instant;

#[derive(Default)]
struct DragPreview {
    pending: bool,
    last_request: Option<Instant>,
}

impl DragPreview {
    fn request_due(&mut self, now: Instant, decoding: bool) -> bool {
        if !self.pending
            || decoding
            || self
                .last_request
                .is_some_and(|last| now.duration_since(last) < Duration::from_millis(100))
        {
            return false;
        }
        self.pending = false;
        self.last_request = Some(now);
        true
    }
}

#[cfg(test)]
mod preview_tests {
    use super::DragPreview;
    use std::time::{Duration, Instant};

    #[test]
    fn drag_preview_throttles_continuous_changes_without_losing_the_latest() {
        let start = Instant::now();
        let mut preview = DragPreview::default();
        assert!(!preview.request_due(start, false));
        preview.pending = true;
        assert!(preview.request_due(start, false), "第一次修改立即请求预览");
        for milliseconds in 1..100 {
            preview.pending = true;
            assert!(!preview.request_due(start + Duration::from_millis(milliseconds), false));
        }
        assert!(preview.pending);
        assert!(preview.request_due(start + Duration::from_millis(100), false));
        assert!(!preview.pending);
        assert!(
            !preview.request_due(start + Duration::from_millis(200), false),
            "数值停止变化后不能重复生成同一预览"
        );
    }

    #[test]
    fn drag_preview_waits_for_decode_and_updates_even_after_pointer_stops() {
        let start = Instant::now();
        let mut preview = DragPreview {
            pending: true,
            last_request: None,
        };
        assert!(preview.request_due(start, false));
        preview.pending = true;
        for milliseconds in [100, 200, 300] {
            assert!(
                !preview.request_due(start + Duration::from_millis(milliseconds), true),
                "持续拖动不能替换正在生成的预览请求"
            );
            assert!(preview.pending, "后台繁忙时保留未呈现的参数变化");
        }
        assert!(
            preview.request_due(start + Duration::from_millis(330), false),
            "无需新的鼠标移动，生成完成后即可呈现最后一次修改"
        );
        assert!(!preview.pending);
    }
}

pub struct NumberDrag {
    id: u64,
    index: usize,
    pointer: crate::pointer_drag::RelativePointer,
    value: f64,
    initial_value: f64,
    original: Project,
    preview: DragPreview,
}

pub struct ColorEdit {
    id: u64,
    original: Project,
}

impl Editor {
    pub fn property_values(&self, id: u64) -> [f64; 8] {
        let clip = self.project.clip(id).unwrap();
        let font_size = self.project.media[clip.media]
            .generator
            .as_ref()
            .and_then(crate::model::Generator::as_text)
            .map(|text| text.font_size)
            .unwrap_or(0.);
        let [x, y, scale, rotation, opacity] = clip.transform.values();
        [
            clip.start,
            clip.duration(),
            font_size,
            x,
            y,
            scale,
            rotation,
            opacity,
        ]
    }

    pub fn refresh_property_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.selected else {
            return;
        };
        let values = self.property_values(id);
        for (field, value) in self.number_fields.iter().zip(values) {
            field.update(cx, |state, cx| {
                state.set_value(format!("{value:.3}"), window, cx)
            });
        }
        let clip = self.project.clip(id).unwrap();
        if let Some(generator) = &self.project.media[clip.media].generator {
            if let Some(text) = generator.as_text() {
                let text = text.text.clone();
                self.subtitle_text
                    .update(cx, |state, cx| state.set_value(text, window, cx));
            }
            if !self.generator_color.read(cx).is_open() {
                let color =
                    u32::from_str_radix(generator.color(), 16).expect("已验证的素材颜色无效");
                let color = rgb(color).opacity(clip.transform.opacity as f32);
                self.generator_color
                    .update(cx, |state, cx| state.set_value(color, window, cx));
            }
        }
    }

    fn set_property(
        project: &mut Project,
        id: u64,
        index: usize,
        value: f64,
    ) -> Result<(), String> {
        let previous = project.clone();
        if index == 2 {
            let media = project.clip(id).ok_or("片段不存在")?.media;
            project.media[media]
                .generator
                .as_mut()
                .and_then(crate::model::Generator::as_text_mut)
                .ok_or("当前片段不是文本素材")?
                .font_size = value;
        } else {
            let clip = project.clip_mut(id).ok_or("片段不存在")?;
            match index {
                0 => clip.start = value,
                1 => {
                    clip.source_out = clip.source_in + value * clip.speed;
                    clip.fit_fades();
                }
                3..=7 => {
                    let mut values = clip.transform.values();
                    values[index - 3] = value;
                    clip.transform = crate::model::Transform::from_values(values)?;
                }
                _ => unreachable!(),
            }
        }
        project.validate()?;
        if index <= 1 {
            project.sync_overlap_fades(&previous);
        }
        Ok(())
    }

    pub fn edit_number(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.number_edit = Some(index);
        self.number_fields[index].update(cx, |state, cx| {
            state.focus(window, cx);
            state.select_all(window, cx);
        });
        cx.notify();
    }

    pub fn begin_number(
        &mut self,
        index: usize,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy || self.job == Some(Job::Export) {
            return;
        }
        self.timeline_motion = None;
        if event.click_count == 2 {
            self.edit_number(index, window, cx);
            return;
        }
        self.focus.focus(window, cx);
        let id = self.selected.unwrap();
        let value = self.property_values(id)[index];
        self.stop();
        self.preload_cancel.store(true, Ordering::Relaxed);
        let viewport = window.viewport_size();
        let pointer = match crate::pointer_drag::RelativePointer::begin(
            (event.position.x / px(1.), event.position.y / px(1.)),
            (viewport.width / px(1.), viewport.height / px(1.)),
            window.scale_factor(),
        ) {
            Ok(pointer) => pointer,
            Err(error) => {
                self.error = Some(error);
                cx.notify();
                return;
            }
        };
        self.number_drag = Some(NumberDrag {
            id,
            index,
            pointer,
            value,
            initial_value: value,
            original: self.project.clone(),
            preview: DragPreview::default(),
        });
        cx.notify();
    }

    pub fn move_number(&mut self, fine: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(drag) = &mut self.number_drag else {
            return;
        };
        let movement = match drag.pointer.delta() {
            Ok(movement) => movement,
            Err(error) => {
                self.finish_number(window, cx);
                self.error = Some(error);
                cx.notify();
                return;
            }
        };
        let step = match drag.index {
            0 | 1 => 0.01,
            2 => 0.2,
            _ => 0.1,
        };
        let delta = f64::from(movement) * step * if fine { 0.1 } else { 1. };
        if delta == 0. {
            return;
        }
        let value = drag.value + delta;
        let value = match drag.index {
            0 => value.max(0.),
            1 | 2 | 5 => value.max(0.001),
            7 => value.clamp(0., 100.),
            _ => value,
        };
        if value == drag.value {
            return;
        }
        let mut next = self.project.clone();
        match Self::set_property(&mut next, drag.id, drag.index, value) {
            Ok(()) => {
                drag.value = value;
                self.project = next;
                self.error = None;
                drag.preview.pending = true;
                self.refresh_number_preview(cx);
                cx.notify();
            }
            Err(error) => {
                self.error = Some(error);
                cx.notify();
            }
        }
    }

    pub(super) fn refresh_number_preview(&mut self, cx: &mut Context<Self>) {
        let Some(drag) = &mut self.number_drag else {
            return;
        };
        // Let the in-flight frame finish instead of invalidating it on every mouse move.
        if drag
            .preview
            .request_due(Instant::now(), self.frame_decoding)
        {
            self.seek(self.position, cx);
        }
    }

    pub fn finish_number(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(drag) = self.number_drag.take() else {
            return;
        };
        if drag.value == drag.initial_value {
            if drag.preview.last_request.is_some() {
                self.seek(self.position, cx);
            }
            return;
        }
        let next = std::mem::replace(&mut self.project, drag.original);
        self.checkpoint();
        self.project = next;
        self.changed(window, cx);
    }

    pub fn apply_number(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.selected else {
            return;
        };
        let value = self.number_fields[index].read(cx).value();
        if value.as_ref() == format!("{:.3}", self.property_values(id)[index]) {
            self.error = None;
            return;
        }
        let mut next = self.project.clone();
        let result = value
            .parse::<f64>()
            .map_err(|_| "请输入有效数字".to_owned())
            .and_then(|value| Self::set_property(&mut next, id, index, value));
        match result {
            Ok(()) => {
                self.checkpoint();
                self.project = next;
                self.changed(window, cx);
            }
            Err(error) => {
                self.error = Some(error);
                cx.notify();
            }
        }
    }

    pub fn add_subtitle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.job == Some(Job::Export) {
            return;
        }
        self.close_color_editor(cx);
        let mut next = self.project.clone();
        match next.add_subtitle(self.position) {
            Ok(id) => {
                if let Some(track) = self.selected_track.filter(|id| {
                    next.tracks
                        .iter()
                        .any(|t| t.id == *id && t.kind == TrackKind::Video)
                }) {
                    if let Err(error) = next.move_clip(id, track, self.position) {
                        self.error = Some(error);
                        cx.notify();
                        return;
                    }
                }
                self.checkpoint();
                self.project = next;
                self.selected = Some(id);
                self.number_edit = None;
                self.selected_track = self.project.clip_track(id).map(|track| track.id);
                self.changed(window, cx);
            }
            Err(error) => {
                self.error = Some(error);
                cx.notify();
            }
        }
    }

    pub fn apply_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.selected else {
            return;
        };
        let media = self.project.clip(id).unwrap().media;
        let Some(original) = self.project.media[media]
            .generator
            .as_ref()
            .and_then(crate::model::Generator::as_text)
        else {
            return;
        };
        let mut text = original.clone();
        text.text = self.subtitle_text.read(cx).value().to_string();
        if &text == original {
            self.error = None;
            return;
        }
        match text.validate() {
            Ok(()) => {
                self.checkpoint();
                self.project.media[media].generator = Some(crate::model::Generator::Text(text));
                self.changed(window, cx);
            }
            Err(error) => {
                self.error = Some(error);
                cx.notify();
            }
        }
    }

    pub fn add_solid(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.job == Some(Job::Export) {
            return;
        }
        self.close_color_editor(cx);
        let mut next = self.project.clone();
        let generator = crate::model::Generator::Solid {
            color: "FFFFFF".into(),
        };
        match next.add_generator(generator, self.position, self.selected_track) {
            Ok(id) => {
                self.checkpoint();
                self.project = next;
                self.selected = Some(id);
                self.number_edit = None;
                self.selected_track = self.project.clip_track(id).map(|track| track.id);
                self.changed(window, cx);
            }
            Err(error) => {
                self.error = Some(error);
                cx.notify();
            }
        }
    }

    pub fn apply_generator_color(
        &mut self,
        color: Hsla,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy || self.job == Some(Job::Export) {
            return;
        }
        let Some(id) = self.selected else {
            return;
        };
        let media = self.project.clip(id).unwrap().media;
        let Some(generator) = &self.project.media[media].generator else {
            return;
        };
        let rgba = Rgba::from(color);
        let hex = format!(
            "{:02X}{:02X}{:02X}",
            (rgba.r * 255.).round() as u8,
            (rgba.g * 255.).round() as u8,
            (rgba.b * 255.).round() as u8
        );
        let opacity = f64::from(rgba.a);
        if generator.color() == hex && self.project.clip(id).unwrap().transform.opacity == opacity {
            return;
        }
        let mut next = self.project.clone();
        match next.media[media].generator.as_mut().unwrap() {
            crate::model::Generator::Text(text) => text.color = hex,
            crate::model::Generator::Solid { color } => *color = hex,
        }
        next.clip_mut(id).unwrap().transform.opacity = opacity;
        if let Err(error) = next.validate() {
            self.error = Some(error);
            cx.notify();
            return;
        }
        if self.color_edit.is_none() {
            self.color_edit = Some(ColorEdit {
                id,
                original: self.project.clone(),
            });
        }
        self.project = next;
        self.changed(window, cx);
        if !self.generator_color.read(cx).is_open() {
            self.finish_color_edit(cx);
        }
    }

    pub fn finish_color_edit(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = self.color_edit.take() else {
            return;
        };
        let media = edit.original.clip(edit.id).unwrap().media;
        if self.project.media[media].generator == edit.original.media[media].generator
            && self.project.clip(edit.id).unwrap().transform.opacity
                == edit.original.clip(edit.id).unwrap().transform.opacity
        {
            return;
        }
        let next = std::mem::replace(&mut self.project, edit.original);
        self.checkpoint();
        self.project = next;
        cx.notify();
    }

    pub fn close_color_editor(&mut self, cx: &mut Context<Self>) {
        self.finish_color_edit(cx);
        self.generator_color
            .update(cx, |state, cx| state.set_open(false, cx));
    }
}
