use crate::{
    media::{self, Event},
    model::{Project, TrackKind},
};
use gpui_kit::{
    component::{
        color_picker::{ColorPickerEvent, ColorPickerState},
        input::{InputState, TextareaState},
        resizable::ResizableState,
    },
    *,
};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    time::Duration,
};
mod auto_subtitles;
mod clipboard;
mod files;
mod filmstrip;
mod preview;
mod project_properties;
mod properties;
mod render_window;
mod selection;
mod timeline_motion;
mod tracks;
pub(crate) mod waveforms;
pub use properties::{ColorEdit, NumberDrag};
pub use timeline_motion::TimelineAnimation;

#[derive(Clone)]
pub enum TimelineContext {
    Clip(u64),
    Fades(Vec<(u64, crate::model::TrimEdge)>),
}

actions!(
    vig,
    [
        Import, Save, Open, NewProject, Play, Split, Delete, Undo, Redo, CopyClips, PasteClips
    ]
);

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-o", Import, Some("Vig && !Input")),
        KeyBinding::new("ctrl-n", NewProject, Some("Vig && !Input")),
        KeyBinding::new("ctrl-s", Save, Some("Vig && !Input")),
        KeyBinding::new("ctrl-shift-o", Open, Some("Vig && !Input")),
        KeyBinding::new("space", Play, Some("Vig && !Input")),
        KeyBinding::new("s", Split, Some("Vig && !Input")),
        KeyBinding::new("delete", Delete, Some("Vig && !Input")),
        KeyBinding::new("ctrl-z", Undo, Some("Vig && !Input")),
        KeyBinding::new("ctrl-shift-z", Redo, Some("Vig && !Input")),
        KeyBinding::new("ctrl-c", CopyClips, Some("Vig && !Input")),
        KeyBinding::new("ctrl-v", PasteClips, Some("Vig && !Input")),
    ]);
}

#[derive(Clone, Copy, PartialEq)]
pub enum Job {
    Export,
    Play,
}

#[derive(Clone, Copy)]
pub enum TimelineDrag {
    Scrub {
        origin: Pixels,
    },
    Pan {
        pointer: Pixels,
        offset: Point<Pixels>,
    },
}

pub struct ClipDrag {
    pub id: u64,
    pub pointer: f32,
    pub start: f64,
    pub original: Project,
    pub invalid: Option<String>,
    pub edge: Option<crate::model::TrimEdge>,
    pub fade_edge: Option<crate::model::TrimEdge>,
    pub copy: Option<crate::model::ClipClipboard>,
    pub copied_id: Option<u64>,
    pub members: Vec<u64>,
}

pub struct TrackResize {
    pub id: u64,
    pub pointer: Pixels,
    pub height: f32,
}

pub struct Editor {
    pub(crate) asr_engine: Arc<crate::asr::AsrEngine>,
    pub project: Project,
    pub selected: Option<u64>,
    pub selection: std::collections::BTreeSet<u64>,
    pub marquee: Option<selection::Marquee>,
    pub position: f64,
    pub zoom: f32,
    pub track_zoom: f32,
    pub status: String,
    pub error: Option<String>,
    pub dirty: bool,
    pub path: Option<PathBuf>,
    pub image: Option<Arc<RenderImage>>,
    pub thumbnails: std::collections::HashMap<PathBuf, Arc<RenderImage>>,
    pub filmstrip: std::collections::HashMap<(PathBuf, u64), Arc<RenderImage>>,
    pending_frames: std::collections::HashSet<(PathBuf, u64)>,
    pub waveforms: std::collections::HashMap<PathBuf, Arc<media::waveform::Waveform>>,
    pub waveform_geometry: std::collections::HashMap<u64, waveforms::WaveGeometryCache>,
    pending_waveforms: std::collections::HashSet<PathBuf>,
    waveform_queue: std::collections::VecDeque<PathBuf>,
    waveform_loading: bool,
    waveform_current: Option<PathBuf>,
    waveform_cancel: Arc<AtomicBool>,
    pub fields: Vec<Entity<InputState>>,
    pub subtitle_text: Entity<TextareaState>,
    pub generator_color: Entity<ColorPickerState>,
    pub color_edit: Option<ColorEdit>,
    pub number_fields: Vec<Entity<InputState>>,
    pub number_drag: Option<NumberDrag>,
    pub number_edit: Option<usize>,
    pub transform_tab: bool,
    pub focus: FocusHandle,
    pub undo: Vec<Project>,
    pub redo: Vec<Project>,
    pub job: Option<Job>,
    pub progress: f64,
    pub busy: bool,
    pub search: Entity<InputState>,
    pub query: String,
    pub snap: bool,
    pub drag: Option<ClipDrag>,
    pub selected_track: Option<u64>,
    pub track_drag: Option<u64>,
    pub track_drop_slot: Option<usize>,
    pub track_resize: Option<TrackResize>,
    pub track_heights: std::collections::HashMap<u64, f32>,
    pub timeline_drag: Option<TimelineDrag>,
    pub timeline_context: Option<TimelineContext>,
    pub timeline_menu: Option<WeakEntity<gpui_kit::component::menu::PopupMenu>>,
    pub timeline_scroll: ScrollHandle,
    pub timeline_viewport_width: f32,
    pub timeline_motion: Option<TimelineAnimation>,
    pub track_labels_scroll: ScrollHandle,
    pub track_label_width: f32,
    pub track_label_resize: Option<(Pixels, f32)>,
    pub workspace_split: Entity<ResizableState>,
    pub timeline_split: Entity<ResizableState>,
    pub cancel: Arc<AtomicBool>,
    receiver: Option<Receiver<Event>>,
    _temp: tempfile::TempDir,
    preview_cache: Arc<media::preview::PreviewCache>,
    preload_cancel: Arc<AtomicBool>,
    frame_generation: u64,
    pending_frame: Option<(u64, Project, f64, (u32, u32))>,
    pub frame_decoding: bool,
    close_pending: bool,
    _subscriptions: Vec<Subscription>,
}

impl Drop for Editor {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.preload_cancel.store(true, Ordering::Relaxed);
        self.waveform_cancel.store(true, Ordering::Relaxed);
    }
}

pub fn render_image(mut buffer: image::RgbaImage) -> Arc<RenderImage> {
    // GPUI's native renderer expects BGRA pixels.
    for p in buffer.pixels_mut() {
        p.0.swap(0, 2);
    }
    Arc::new(RenderImage::new(smallvec::smallvec![image::Frame::new(
        buffer
    )]))
}

impl Editor {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let weak = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            let Some(entity) = weak.upgrade() else {
                return true;
            };
            entity.update(cx, |this, cx| {
                if !this.dirty {
                    return true;
                }
                if this.close_pending {
                    return false;
                }
                this.close_pending = true;
                let dialog = rfd::AsyncMessageDialog::new().set_parent(window);
                cx.spawn_in(window, async move |this, cx| {
                    let discard = dialog
                        .set_title("关闭 Vig")
                        .set_description("当前项目有未保存的更改。是否丢弃更改并关闭？")
                        .set_buttons(rfd::MessageButtons::YesNo)
                        .show()
                        .await
                        == rfd::MessageDialogResult::Yes;
                    let _ = this.update_in(cx, |this, window, cx| {
                        this.close_pending = false;
                        if discard {
                            this.stop();
                            window.remove_window();
                        }
                        cx.notify();
                    });
                })
                .detach();
                false
            })
        });
        let fields: Vec<_> = (0..8)
            .map(|_| cx.new(|cx| InputState::new(window, cx)))
            .collect();
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("搜索素材…"));
        let subtitle_text = cx.new(|cx| TextareaState::new(window, cx));
        let generator_color = cx.new(|cx| ColorPickerState::new(window, cx));
        let number_fields: Vec<_> = (0..8)
            .map(|_| cx.new(|cx| InputState::new(window, cx)))
            .collect();
        let mut subscriptions = vec![cx.subscribe(
            &search,
            |this, state, _: &gpui_kit::component::input::InputEvent, cx| {
                this.query = state.read(cx).value().to_string();
                cx.notify();
            },
        )];
        for field in fields.iter().take(7) {
            subscriptions.push(cx.subscribe_in(
                field,
                window,
                |this, _, event: &gpui_kit::component::input::InputEvent, window, cx| {
                    use gpui_kit::component::input::InputEvent;
                    if matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                        this.apply_fields(window, cx);
                    }
                },
            ));
        }
        subscriptions.push(cx.subscribe_in(
            &subtitle_text,
            window,
            |this, _, event: &gpui_kit::component::input::InputEvent, window, cx| {
                if matches!(event, gpui_kit::component::input::InputEvent::Blur) {
                    this.apply_text(window, cx);
                }
            },
        ));
        subscriptions.push(cx.subscribe_in(
            &generator_color,
            window,
            |this, _, event: &ColorPickerEvent, window, cx| {
                let ColorPickerEvent::Change(color) = event;
                if let Some(color) = color {
                    this.apply_generator_color(*color, window, cx);
                } else {
                    this.error = Some("请选择颜色".into());
                    cx.notify();
                }
            },
        ));
        subscriptions.push(
            cx.observe_in(&generator_color, window, |this, state, _, cx| {
                if !state.read(cx).is_open() {
                    this.finish_color_edit(cx);
                }
            }),
        );
        for (index, field) in number_fields.iter().enumerate() {
            subscriptions.push(cx.subscribe_in(
                field,
                window,
                move |this, _, event: &gpui_kit::component::input::InputEvent, window, cx| {
                    use gpui_kit::component::input::InputEvent;
                    if matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                        if this.number_edit != Some(index) {
                            return;
                        }
                        this.apply_number(index, window, cx);
                        if this.error.is_none() {
                            this.number_edit = None;
                            if matches!(event, InputEvent::PressEnter { .. }) {
                                this.focus.focus(window, cx);
                            }
                            cx.notify();
                        }
                    }
                },
            ));
        }
        cx.spawn_in(window, async move |this, cx| {
            let mut interval = Duration::from_millis(30);
            loop {
                cx.background_executor().timer(interval).await;
                match this.update_in(cx, |this, window, cx| {
                    this.poll(window, cx);
                    if this.job == Some(Job::Play) {
                        Duration::from_secs_f64(
                            this.project.settings().frame_rate().frame_duration() / 2.,
                        )
                        .min(Duration::from_millis(30))
                    } else {
                        Duration::from_millis(30)
                    }
                }) {
                    Ok(next) => interval = next,
                    Err(_) => break,
                }
            }
        })
        .detach();
        let temp = tempfile::tempdir().expect("无法创建预览缓存目录");
        let preview_cache = Arc::new(media::preview::PreviewCache::new(
            temp.path().join("segments"),
            Default::default(),
            Default::default(),
        ));
        Self {
            asr_engine: Arc::new(crate::asr::AsrEngine::default()),
            project: Project::default(),
            selected: None,
            selection: Default::default(),
            marquee: None,
            position: 0.,
            zoom: 60.,
            track_zoom: 1.,
            status: "startup".into(),
            error: None,
            dirty: false,
            path: None,
            image: None,
            thumbnails: std::collections::HashMap::new(),
            filmstrip: std::collections::HashMap::new(),
            pending_frames: std::collections::HashSet::new(),
            waveforms: std::collections::HashMap::new(),
            waveform_geometry: std::collections::HashMap::new(),
            pending_waveforms: std::collections::HashSet::new(),
            waveform_queue: std::collections::VecDeque::new(),
            waveform_loading: false,
            waveform_current: None,
            waveform_cancel: Arc::new(AtomicBool::new(false)),
            fields,
            subtitle_text,
            generator_color,
            color_edit: None,
            number_fields,
            number_drag: None,
            number_edit: None,
            transform_tab: false,
            focus,
            undo: vec![],
            redo: vec![],
            job: None,
            progress: 0.,
            busy: false,
            search,
            query: String::new(),
            snap: true,
            drag: None,
            selected_track: None,
            track_drag: None,
            track_drop_slot: None,
            track_resize: None,
            track_heights: std::collections::HashMap::new(),
            timeline_drag: None,
            timeline_scroll: ScrollHandle::new(),
            timeline_viewport_width: 0.,
            timeline_motion: None,
            track_labels_scroll: ScrollHandle::new(),
            workspace_split: cx.new(|_| ResizableState::default()),
            timeline_split: cx.new(|_| ResizableState::default()),
            cancel: Arc::new(AtomicBool::new(false)),
            receiver: None,
            _temp: temp,
            preview_cache,
            preload_cancel: Arc::new(AtomicBool::new(false)),
            frame_generation: 0,
            track_label_width: 150.,
            track_label_resize: None,
            timeline_context: None,
            timeline_menu: None,
            pending_frame: None,
            frame_decoding: false,
            close_pending: false,
            _subscriptions: subscriptions,
        }
    }

    pub fn stop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.job = None;
        self.receiver = None;
    }

    pub fn checkpoint(&mut self) {
        self.stop();
        self.undo.push(self.project.clone());
        self.redo.clear();
    }

    pub fn changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.reconcile_selection();
        self.timeline_motion = None;
        self.selected_track = self
            .selected_track
            .filter(|id| self.project.tracks.iter().any(|t| t.id == *id));
        self.dirty = true;
        self.error = None;
        self.position = self.position.min(self.project.duration());
        self.refresh_fields(window, cx);
        self.seek(self.position, cx);
        self.load_filmstrip(cx);
        cx.notify();
    }

    pub fn select(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        self.close_color_editor(cx);
        self.timeline_motion = None;
        self.number_edit = None;
        self.selected_track = self.project.clip_track(id).map(|t| t.id);
        self.selected = Some(id);
        self.selection = self.project.linked_ids(id).into_iter().collect();
        self.refresh_fields(window, cx);
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub fn refresh_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.refresh_property_fields(window, cx);
        if let Some(c) = self.selected.and_then(|id| self.project.clip(id)) {
            let values = [
                c.start,
                c.source_in,
                c.source_out,
                c.speed,
                c.volume * 100.,
                c.fade_in,
                c.fade_out,
                c.duration(),
            ];
            for (field, v) in self.fields.iter().zip(values) {
                field.update(cx, |state, cx| {
                    state.set_value(format!("{v:.3}"), window, cx)
                });
            }
        }
    }

    pub fn apply_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.selected else {
            return;
        };
        let clip = self.project.clip(id).unwrap();
        if self.project.media[clip.media].generator.is_some() {
            return;
        }
        let previous = [
            clip.start,
            clip.source_in,
            clip.source_out,
            clip.speed,
            clip.volume * 100.,
            clip.fade_in,
            clip.fade_out,
        ];
        if self
            .fields
            .iter()
            .zip(previous)
            .all(|(field, value)| field.read(cx).value().as_ref() == format!("{value:.3}"))
        {
            self.error = None;
            return;
        }
        let values = self
            .fields
            .iter()
            .take(7)
            .zip(previous)
            .map(|(field, previous)| {
                let value = field.read(cx).value();
                if value.as_ref() == format!("{previous:.3}") {
                    Ok(previous)
                } else {
                    value.parse::<f64>()
                }
            })
            .collect::<Result<Vec<_>, _>>();
        let result = (|| {
            let v = values.map_err(|_| "请输入有效数字")?;
            let mut next = self.project.clone();
            let c = next.clip_mut(id).ok_or("片段不存在")?;
            c.start = v[0];
            c.source_in = v[1];
            c.source_out = v[2];
            c.speed = v[3];
            c.volume = v[4] / 100.;
            c.fade_in = v[5];
            c.fade_out = v[6];
            next.validate()?;
            next.sync_overlap_fades(&self.project);
            Ok::<_, String>(next)
        })();
        match result {
            Ok(next) => {
                self.checkpoint();
                self.project = next;
                self.changed(window, cx);
            }
            Err(e) => {
                self.error = Some(e);
                cx.notify();
            }
        }
    }

    pub fn split(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_color_editor(cx);
        if self.selected.is_none() {
            self.error = Some("请先选择时间线片段".into());
            cx.notify();
            return;
        }
        let mut next = self.project.clone();
        let members = self.selection_members();
        let first_id = next.next_id;
        let result = (|| {
            let mut visited = std::collections::BTreeSet::new();
            let mut count = 0;
            for member in &members {
                if visited.contains(member) {
                    continue;
                }
                let group = self.project.linked_ids(*member);
                visited.extend(group.iter().copied());
                if group.iter().all(|id| {
                    let clip = self.project.clip(*id).unwrap();
                    self.position > clip.start && self.position < clip.end()
                }) {
                    next.split_linked(*member, self.position)?;
                    count += 1;
                }
            }
            if count == 0 {
                return Err("播放头不在所选片段内部".into());
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.checkpoint();
                self.selection = members.into_iter().collect();
                self.selection.extend(
                    next.tracks
                        .iter()
                        .flat_map(|track| &track.clips)
                        .filter(|clip| clip.id >= first_id)
                        .map(|clip| clip.id),
                );
                self.project = next;
                self.changed(window, cx);
            }
            Err(e) => {
                self.error = Some(e);
                cx.notify();
            }
        }
    }

    pub fn delete(&mut self, ripple: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.close_color_editor(cx);
        if self.selected.is_some() {
            let mut next = self.project.clone();
            let members = self.selection_members();
            if let Err(error) = next.delete_selection(&members, ripple) {
                self.error = Some(error);
                cx.notify();
                return;
            }
            self.checkpoint();
            self.project = next;
            self.selected = None;
            self.selection.clear();
            self.changed(window, cx);
        }
    }

    pub fn history(&mut self, redo: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.close_color_editor(cx);
        let next = if redo {
            self.redo.pop()
        } else {
            self.undo.pop()
        };
        if let Some(next) = next {
            self.stop();
            if redo {
                self.undo.push(self.project.clone());
            } else {
                self.redo.push(self.project.clone());
            }
            self.project = next;
            self.selected = self.selected.filter(|id| self.project.clip(*id).is_some());
            self.changed(window, cx);
        }
    }

    pub fn add(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let mut next = self.project.clone();
        match next.insert_media(index, self.selected_track, None) {
            Ok(id) => {
                let match_video = next.first_video_match(&self.project);
                self.checkpoint();
                self.project = next;
                self.selected = Some(id);
                self.changed(window, cx);
                self.offer_first_video_settings(match_video, window, cx);
            }
            Err(error) => {
                self.error = Some(error);
                cx.notify();
            }
        }
    }

    pub fn seek(&mut self, time: f64, cx: &mut Context<Self>) {
        self.sync_preview_settings();
        if self.job == Some(Job::Play) {
            self.stop();
        }
        self.position = time.clamp(0., self.project.duration());
        if self.number_drag.is_none() {
            self.warm_preview(cx);
        }
        self.frame_generation += 1;
        let generation = self.frame_generation;
        let visible = self
            .project
            .video()
            .any(|c| self.position >= c.start && self.position < c.end());
        if visible {
            self.pending_frame = Some((
                generation,
                self.project.clone(),
                self.position,
                self.preview_cache.dimensions(),
            ));
            if !self.frame_decoding {
                self.frame_decoding = true;
                cx.spawn(async move |this, cx| {
                    loop {
                        let request = match this.update(cx, |this, _| {
                            let request = this.pending_frame.take();
                            if request.is_none() {
                                this.frame_decoding = false;
                            }
                            request
                        }) {
                            Ok(Some(request)) => request,
                            _ => break,
                        };
                        let (generation, project, time, dimensions) = request;
                        let result = cx
                            .background_spawn(async move {
                                let bytes = media::subtitles::snapshot(&project, time, dimensions)?;
                                let buffer = image::load_from_memory(&bytes)
                                    .map_err(|e| e.to_string())?
                                    .into_rgba8();
                                Ok::<_, String>(render_image(buffer))
                            })
                            .await;
                        let _ = this.update(cx, |this, cx| {
                            if this.frame_generation == generation {
                                match result {
                                    Ok(image) => this.image = Some(image),
                                    Err(e) => this.error = Some(e),
                                }
                                cx.notify();
                            }
                        });
                    }
                })
                .detach();
            }
        } else {
            self.pending_frame = None;
            self.image = None;
        }
        cx.notify();
    }

    pub fn play(&mut self, cx: &mut Context<Self>) {
        self.sync_preview_settings();
        if self.job.is_some() {
            self.stop();
            cx.notify();
            return;
        }
        if self.project.duration() == 0. {
            self.error = Some("请先将素材加入时间线".into());
            cx.notify();
            return;
        }
        if self.position >= self.project.duration() - 0.01 {
            self.position = 0.;
        }
        self.frame_generation += 1;
        self.preload_cancel.store(true, Ordering::Relaxed);
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.cancel.clone();
        let project = self.project.clone();
        let cache = self.preview_cache.clone();
        let start = self.position;
        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        self.job = Some(Job::Play);
        self.status = "正在启动播放…".into();
        std::thread::spawn(move || {
            let result = media::preview::playback(project, cache, start, cancel, tx.clone());
            let _ = tx.send(Event::Finished(result));
        });
        cx.notify();
    }

    pub fn start_render(
        &mut self,
        path: PathBuf,
        settings: media::render_settings::RenderSettings,
        cx: &mut Context<Self>,
    ) {
        self.stop();
        self.preload_cancel.store(true, Ordering::Relaxed);
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.cancel.clone();
        let project = self.project.clone();
        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        self.job = Some(Job::Export);
        self.progress = 0.;
        self.error = None;
        self.status = format!("正在渲染：{}", path.display());
        std::thread::spawn(move || {
            let result =
                media::render_settings::render_configured(&project, &path, &settings, cancel, &tx);
            let _ = tx.send(Event::Finished(result));
        });
        cx.notify();
    }

    fn poll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.number_drag.is_some() {
            if !window.is_window_active() {
                self.finish_number(window, cx);
            } else {
                #[cfg(windows)]
                {
                    self.move_number(crate::pointer_drag::shift_down(), window, cx);
                    if !crate::pointer_drag::left_button_down() {
                        self.finish_number(window, cx);
                    }
                }
                self.refresh_number_preview(cx);
            }
        }
        let mut events = Vec::new();
        let mut disconnected = false;
        if let Some(receiver) = &self.receiver {
            loop {
                match receiver.try_recv() {
                    Ok(event) => events.push(event),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }
        if events.is_empty() {
            if disconnected {
                self.stop();
                self.error = Some("后台媒体线程意外终止；请查看终端错误日志".into());
                cx.notify();
            }
            return;
        }
        let mut latest_frame = None;
        for event in events {
            match event {
                Event::Progress(p) => self.progress = p,
                Event::Frame(pixels, time) => latest_frame = Some((pixels, time)),
                Event::PlaybackPosition(time) => self.position = time.min(self.project.duration()),
                Event::Buffering(buffering) => {
                    self.status = if buffering {
                        "正在缓冲附近预览…"
                    } else {
                        "播放中"
                    }
                    .into();
                }
                Event::Finished(result) => {
                    let job = self.job.take();
                    self.receiver = None;
                    match result {
                        Ok(()) => match job {
                            Some(Job::Export) => {
                                self.progress = 1.;
                                self.status = "渲染完成".into();
                            }
                            Some(Job::Play) => {
                                self.position = self.project.duration();
                                self.status = "播放结束".into();
                            }
                            _ => self.status = "播放结束".into(),
                        },
                        Err(e) => self.error = Some(e),
                    }
                }
            }
        }
        if let Some((pixels, _)) = latest_frame.filter(|(_, time)| {
            self.job != Some(Job::Play)
                || self.position < *time + self.project.settings().frame_rate().frame_duration()
        }) {
            let (width, height) = self.preview_cache.dimensions();
            self.image = Some(render_image(
                image::RgbaImage::from_raw(width, height, pixels).expect("解码帧长度无效"),
            ));
        }
        cx.notify();
    }
}
