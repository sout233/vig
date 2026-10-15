use crate::{
    editor::{Editor, Job},
    media,
};
use gpui_kit::{test::TestWindowExt, *};
use std::{path::PathBuf, time::Duration};
mod fades;
mod links;
mod tracks;
mod trimming;

pub fn run(handle: AnyWindowHandle, view: Entity<Editor>, directory: PathBuf, cx: &mut App) {
    std::fs::create_dir_all(&directory).expect("无法创建验证目录");
    cx.spawn(async move |cx| {
        let result = Box::pin(check(handle, &view, &directory, cx)).await;
        let report = match result {
            Ok(()) => "PASS: multi-track add/delete/reorder, video/audio cross-track drag, library and external track drop, undo/redo, project save, timeline wheel/scrub/pan, filmstrip, menus and real playback".to_owned(),
            Err(e) => format!("FAIL: {e}"),
        };

        std::fs::write(directory.join("result.txt"), report.as_bytes())
            .expect("无法保存验证结果");
        cx.update(|cx| cx.quit());
    })
    .detach();
}

async fn wait(
    handle: AnyWindowHandle,
    view: &Entity<Editor>,
    cx: &mut AsyncApp,
    predicate: impl Fn(&Editor) -> bool,
) -> Result<(), String> {
    let start = std::time::Instant::now();
    loop {
        let ready = handle
            .update(cx, |_, _, cx| {
                let editor = view.read(cx);
                if let Some(error) = &editor.error {
                    return Err(error.clone());
                }
                Ok(predicate(editor))
            })
            .map_err(|e| e.to_string())??;
        if ready {
            return Ok(());
        }
        if start.elapsed() > Duration::from_secs(40) {
            return Err("原生界面验证超时".into());
        }
        cx.background_executor()
            .timer(Duration::from_millis(50))
            .await;
    }
}

fn screenshot(handle: AnyWindowHandle, path: PathBuf, cx: &mut AsyncApp) -> Result<(), String> {
    handle
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            window
                .render_to_image()
                .map_err(|e| e.to_string())?
                .save(path)
                .map_err(|e| e.to_string())
        })
        .map_err(|e| e.to_string())?
}

async fn check(
    handle: AnyWindowHandle,
    view: &Entity<Editor>,
    dir: &std::path::Path,
    cx: &mut AsyncApp,
) -> Result<(), String> {
    cx.background_executor()
        .timer(Duration::from_millis(500))
        .await;
    screenshot(handle, dir.join("01-empty.png"), cx)?;
    handle
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            let bounds = window.find("import").bounds();
            let scale = window.scale_factor();
            let pixels = window.render_to_image().map_err(|e| e.to_string())?;
            let crop = image::imageops::crop_imm(
                &pixels,
                ((bounds.left() / px(1.) - 4.) * scale) as u32,
                ((bounds.top() / px(1.) - 4.) * scale) as u32,
                ((bounds.size.width / px(1.) + 8.) * scale) as u32,
                ((bounds.size.height / px(1.) + 8.) * scale) as u32,
            )
            .to_image();
            image::imageops::resize(
                &crop,
                crop.width() * 2,
                crop.height() * 2,
                image::imageops::FilterType::Nearest,
            )
            .save(dir.join("00-button-material-detail.png"))
            .map_err(|e| e.to_string())
        })
        .map_err(|e| e.to_string())??;
    let source = dir.join("测试视频.mp4");
    let source_copy = source.clone();
    cx.background_spawn(async move {
        let output = media::command("ffmpeg")
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=s=640x360:r=30:d=4",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=330:duration=4",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-shortest",
            ])
            .arg(&source_copy)
            .output()
            .map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).into_owned());
        }
        Ok::<_, String>(())
    })
    .await?;
    handle
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            let position = window.find("editor-drop-zone").bounds().center();
            window.dispatch_event(
                FileDropEvent::Entered {
                    position,
                    paths: ExternalPaths([source.clone()].into_iter().collect()),
                }
                .to_platform_input(),
                cx,
            );
            window.dispatch_event(FileDropEvent::Submit { position }.to_platform_input(), cx);
            window.dispatch_event(FileDropEvent::Ended.to_platform_input(), cx);
        })
        .map_err(|e| e.to_string())?;
    wait(handle, view, cx, |e| {
        !e.busy && e.project.tracks[0].clips.len() == 1 && e.filmstrip.len() == 3
    })
    .await?;
    handle
        .update(cx, |_, window, cx| {
            assert_eq!(
                view.read(cx).project.tracks[0].clips.len(),
                1,
                "外部文件拖放必须导入并追加真实片段"
            );
            // This regression checks independent editing; linked editing has its own scenario.
            view.update(cx, |e, _| e.project.unlink(1));
            window.click(("clip", 1u64), cx);
            window.click_at("ruler", point(px(120.), px(10.)), cx);
            assert!((view.read(cx).position - 2.).abs() < 0.02);
            window.press("s", cx);
            assert_eq!(view.read(cx).project.tracks[0].clips.len(), 2);
            window.press("ctrl-z", cx);
            assert_eq!(view.read(cx).project.tracks[0].clips.len(), 1);
            window.click("redo", cx);
            assert_eq!(view.read(cx).project.tracks[0].clips.len(), 2);
            window.click(("clip", 1u64), cx);
            view.update(cx, |e, cx| {
                e.fields[2].update(cx, |f, cx| {
                    f.focus(window, cx);
                    f.set_value("1.5", window, cx);
                })
            });
            window.press("enter", cx);
            assert_eq!(view.read(cx).project.tracks[0].clips[0].source_out, 1.5);
            window.render_frame(cx);
            let clip = window.find(("clip", 3u64)).bounds();
            window.drag(clip.center(), clip.center() + point(px(60.), px(0.)), cx);
            assert!(
                (view.read(cx).project.tracks[0].clips[1].start - 3.).abs() < 0.02,
                "拖动应改变时间线起点"
            );
            view.update(cx, |e, cx| e.seek(0., cx));
        })
        .map_err(|e| e.to_string())?;
    wait(handle, view, cx, |e| {
        e.image.is_some()
            && e.project.tracks[0].clips.iter().all(|clip| {
                clip.preview_times().into_iter().all(|time| {
                    e.filmstrip
                        .contains_key(&(e.project.media[clip.media].path.clone(), time))
                })
            })
    })
    .await?;
    screenshot(handle, dir.join("02-editing.png"), cx)?;
    handle
        .update(cx, |_, window, cx| {
            let original = (view.read(cx).zoom, view.read(cx).track_zoom);
            for (zoom, height) in [(5., 1.), (240., 1.), (400., 1.), (400., 2.)] {
                view.update(cx, |e, cx| {
                    e.zoom = zoom;
                    e.track_zoom = height;
                    cx.notify();
                });
                window.render_frame(cx);
                let frame = window.find("preview-frame-1-0").bounds().size;
                assert!(
                    frame.width <= frame.height * 16. / 9. + px(1.),
                    "横向缩放不能把预览图变宽拉伸"
                );
                if zoom >= 240. && height == 1. {
                    assert!(
                        (frame.width / frame.height - 16. / 9.).abs() < 0.01,
                        "空间足够时缩略图应保持固定宽高比"
                    );
                }
            }
            view.update(cx, |e, cx| {
                e.zoom = original.0;
                e.track_zoom = original.1;
                cx.notify();
            });
        })
        .map_err(|e| e.to_string())?;
    handle
        .update(cx, |_, w, cx| w.click("play", cx))
        .map_err(|e| e.to_string())?;
    wait(handle, view, cx, |e| {
        e.job == Some(Job::Play) && e.position > 0.5
    })
    .await?;
    screenshot(handle, dir.join("03-playback.png"), cx)?;
    handle
        .update(cx, |_, w, cx| {
            w.click("play", cx);
            assert!(view.read(cx).job.is_none());
            view.update(cx, |e, cx| e.seek(3.2, cx));
            w.click("play", cx);
        })
        .map_err(|e| e.to_string())?;
    wait(handle, view, cx, |e| {
        e.job == Some(Job::Play) && e.position > 3.6
    })
    .await?;
    handle
        .update(cx, |_, w, cx| w.click("play", cx))
        .map_err(|e| e.to_string())?;
    handle
        .update(cx, |_, w, cx| {
            let position = w.find("editor-drop-zone").bounds().center();
            w.dispatch_event(
                FileDropEvent::Entered {
                    position,
                    paths: ExternalPaths([source.clone()].into_iter().collect()),
                }
                .to_platform_input(),
                cx,
            );
            w.dispatch_event(FileDropEvent::Submit { position }.to_platform_input(), cx);
            w.dispatch_event(FileDropEvent::Ended.to_platform_input(), cx);
        })
        .map_err(|e| e.to_string())?;
    wait(handle, view, cx, |e| {
        !e.busy && e.project.tracks[0].clips.len() == 3
    })
    .await?;
    handle
        .update(cx, |_, w, cx| {
            assert_eq!(
                view.read(cx).project.media.len(),
                1,
                "重复拖入应复用素材，同时创建新片段"
            );
            w.click("edit-menu", cx);
            w.click(0usize, cx);
        })
        .map_err(|e| e.to_string())?;
    wait(handle, view, cx, |e| e.project.tracks[0].clips.len() == 2).await?;
    handle
        .update(cx, |_, w, cx| {
            w.click("edit-menu", cx);
            w.click(1usize, cx);
        })
        .map_err(|e| e.to_string())?;
    wait(handle, view, cx, |e| e.project.tracks[0].clips.len() == 3).await?;
    handle
        .update(cx, |_, w, cx| {
            w.click("view-menu", cx);
            w.click(0usize, cx);
        })
        .map_err(|e| e.to_string())?;
    wait(handle, view, cx, |e| !e.snap).await?;
    handle
        .update(cx, |_, w, cx| {
            w.click("view-menu", cx);
            w.click(1usize, cx);
        })
        .map_err(|e| e.to_string())?;
    wait(handle, view, cx, |e| e.zoom > 60.).await?;
    let project_path = dir.join("菜单保存项目.vig");
    handle
        .update(cx, |_, w, cx| {
            view.update(cx, |e, _| e.path = Some(project_path.clone()));
            w.click("file-menu", cx);
        })
        .map_err(|e| e.to_string())?;
    screenshot(handle, dir.join("04-file-menu.png"), cx)?;
    handle
        .update(cx, |_, w, cx| w.click(3usize, cx))
        .map_err(|e| e.to_string())?;
    wait(handle, view, cx, |e| !e.busy && !e.dirty).await?;
    let saved: crate::model::Project =
        serde_json::from_str(&std::fs::read_to_string(project_path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    assert_eq!(saved.tracks[0].clips.len(), 3);
    saved.validate()?;
    screenshot(handle, dir.join("05-filmstrip.png"), cx)?;
    handle
        .update(cx, |_, window, cx| {
            for (id, horizontal, trailing, delta) in [
                ("library-panel", true, true, 64.),
                ("inspector", true, false, -64.),
                ("timeline-panel", false, false, -80.),
            ] {
                window.render_frame(cx);
                let bounds = window.find(id).bounds();
                let from = if horizontal {
                    point(
                        if trailing {
                            bounds.right()
                        } else {
                            bounds.left()
                        },
                        bounds.center().y,
                    )
                } else {
                    point(bounds.center().x, bounds.top())
                };
                let movement = if horizontal {
                    point(px(delta), px(0.))
                } else {
                    point(px(0.), px(delta))
                };
                window.drag(from, from + movement, cx);
                window.render_frame(cx);
                let after = window.find(id).bounds();
                let before_size = if horizontal {
                    bounds.size.width
                } else {
                    bounds.size.height
                };
                let after_size = if horizontal {
                    after.size.width
                } else {
                    after.size.height
                };
                let expected = if trailing { delta } else { -delta };
                assert!(
                    ((after_size - before_size) / px(1.) - expected).abs() < 1.,
                    "{id} 分隔线拖动后尺寸不正确：before={before_size:?}, after={after_size:?}"
                );
            }
            assert!(!view.read(cx).dirty, "调整面板不能修改项目状态");
        })
        .map_err(|e| e.to_string())?;
    screenshot(handle, dir.join("07-panel-resize.png"), cx)?;
    handle
        .update(cx, |_, window, cx| {
            window.click("zoom-in", cx);
            window.click("zoom-in", cx);
            window.click("zoom-in", cx);
            window.render_frame(cx);
            let viewport = window.find("timeline-scroll").bounds();
            let audio = window.find("track-2").bounds();
            let from = point(viewport.left() + px(320.), audio.top() + px(3.));
            let to = from - point(px(160.), px(0.));
            window.dispatch_event(
                MouseDownEvent {
                    position: from,
                    button: MouseButton::Middle,
                    modifiers: Default::default(),
                    click_count: 1,
                    first_mouse: false,
                }
                .to_platform_input(),
                cx,
            );
            window.render_frame(cx);
            window.dispatch_event(
                MouseMoveEvent {
                    position: to,
                    pressed_button: Some(MouseButton::Middle),
                    modifiers: Default::default(),
                }
                .to_platform_input(),
                cx,
            );
            window.dispatch_event(
                MouseUpEvent {
                    position: to,
                    button: MouseButton::Middle,
                    modifiers: Default::default(),
                    click_count: 1,
                }
                .to_platform_input(),
                cx,
            );
            window.render_frame(cx);
        })
        .map_err(|e| e.to_string())?;
    wait(handle, view, cx, |editor| {
        (editor.timeline_scroll.offset().x / px(1.) + 160.).abs() < 1.
    })
    .await?;
    handle
        .update(cx, |_, window, cx| {
            assert!(
                (view.read(cx).timeline_scroll.offset().x / px(1.) + 160.).abs() < 1.,
                "中键拖动应横向平移时间线"
            );
            let viewport = window.find("timeline-scroll").bounds();
            let ruler = window.find("ruler").bounds();
            let from = point(viewport.left() + px(100.), ruler.center().y);
            let to = point(viewport.left() + px(220.), ruler.center().y);
            window.drag(from, to, cx);
            let e = view.read(cx);
            let expected =
                (f64::from((to.x - viewport.left() - e.timeline_scroll.offset().x) / px(e.zoom))
                    * 30.)
                    .round()
                    / 30.;
            assert!(
                (e.position - expected).abs() < 0.001,
                "滚动后的刻度拖动必须正确定位"
            );
            window.render_frame(cx);
            let from = window.find("playhead").bounds().center();
            let position = view.read(cx).position;
            let zoom = view.read(cx).zoom;
            window.drag(from, from - point(px(60.), px(0.)), cx);
            let expected = ((position - 60. / f64::from(zoom)) * 30.).round() / 30.;
            assert!(
                (view.read(cx).position - expected).abs() < 0.02,
                "播放头必须可直接拖动"
            );
            assert!(
                view.read(cx).timeline_drag.is_none(),
                "释放鼠标后应结束时间线拖动"
            );
            assert!(!view.read(cx).dirty, "平移与定位不能改变项目编辑状态");
            assert_eq!(
                view.read(cx).project.tracks[0]
                    .clips
                    .iter()
                    .map(|c| c.start)
                    .collect::<Vec<_>>(),
                saved.tracks[0]
                    .clips
                    .iter()
                    .map(|c| c.start)
                    .collect::<Vec<_>>(),
                "时间线拖动不能移动片段"
            );
        })
        .map_err(|e| e.to_string())?;
    wait(handle, view, cx, |e| !e.frame_decoding && e.image.is_some()).await?;
    screenshot(handle, dir.join("06-timeline-drag.png"), cx)?;
    handle
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            let viewport = window.find("timeline-scroll").bounds();
            let position = point(viewport.left() + px(300.), viewport.top() + px(90.));
            let before = view.read(cx).timeline_scroll.offset();
            let zoom = view.read(cx).zoom;
            let time = (position.x - viewport.left() - before.x) / zoom;
            window.dispatch_event(
                ScrollWheelEvent {
                    position,
                    delta: ScrollDelta::Pixels(point(px(0.), px(40.))),
                    modifiers: Modifiers {
                        control: true,
                        ..Default::default()
                    },
                    ..Default::default()
                }
                .to_platform_input(),
                cx,
            );
            window.render_frame(cx);
            let e = view.read(cx);
            assert!(e.zoom > zoom, "Ctrl+滚轮应缩放横轴");
            assert_eq!(
                e.timeline_scroll.offset().y,
                before.y,
                "横轴缩放不能上下滚动"
            );
            let anchored = (position.x - viewport.left() - e.timeline_scroll.offset().x) / e.zoom;
            assert!(
                ((anchored - time) / px(1.)).abs() < 0.01,
                "横轴缩放应保留鼠标下的时间"
            );
            let before = e.timeline_scroll.offset();
            let zoom = e.zoom;
            window.dispatch_event(
                ScrollWheelEvent {
                    position,
                    delta: ScrollDelta::Pixels(point(px(0.), px(-48.))),
                    modifiers: Modifiers {
                        shift: true,
                        ..Default::default()
                    },
                    ..Default::default()
                }
                .to_platform_input(),
                cx,
            );
            window.render_frame(cx);
            let e = view.read(cx);
            assert!(
                ((e.timeline_scroll.offset().x - before.x) / px(1.) + 48.).abs() < 0.01,
                "Shift+滚轮应只横向滚动一次"
            );
            assert_eq!(e.zoom, zoom);
            assert_eq!(e.timeline_scroll.offset().y, before.y);
            let old_height = window.find("track-1").bounds().size.height;
            let old_zoom = e.track_zoom;
            window.dispatch_event(
                ScrollWheelEvent {
                    position,
                    delta: ScrollDelta::Pixels(point(px(0.), px(160.))),
                    modifiers: Modifiers {
                        alt: true,
                        ..Default::default()
                    },
                    ..Default::default()
                }
                .to_platform_input(),
                cx,
            );
            window.render_frame(cx);
            assert!(view.read(cx).track_zoom > old_zoom, "Alt+滚轮应缩放纵轴");
            assert!(
                window.find("track-1").bounds().size.height > old_height,
                "纵轴缩放必须改变轨道高度"
            );
            assert_eq!(view.read(cx).zoom, zoom, "纵轴缩放不能改变时间比例");
            let before = view.read(cx).timeline_scroll.offset();
            window.dispatch_event(
                ScrollWheelEvent {
                    position,
                    delta: ScrollDelta::Pixels(point(px(0.), px(-32.))),
                    ..Default::default()
                }
                .to_platform_input(),
                cx,
            );
            window.render_frame(cx);
            let e = view.read(cx);
            assert!(
                e.timeline_scroll.offset().y < before.y,
                "普通滚轮应上下滚动"
            );
            assert_eq!(
                e.timeline_scroll.offset().x,
                before.x,
                "普通滚轮不能横向滚动"
            );
            assert_eq!(
                e.track_labels_scroll.offset().y,
                e.timeline_scroll.offset().y,
                "轨道标签需要与内容同步上下滚动"
            );
            assert!(!e.dirty, "滚动与缩放不能修改项目保存状态");
        })
        .map_err(|e| e.to_string())?;
    screenshot(handle, dir.join("08-timeline-wheel.png"), cx)?;
    tracks::check(handle, view, dir, cx).await?;
    trimming::check(handle, view, dir, cx).await?;
    fades::check(handle, view, dir, cx).await?;
    links::check(handle, view, dir, cx).await?;
    Ok(())
}
