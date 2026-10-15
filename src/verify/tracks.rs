use super::*;

pub(super) async fn check(
    handle: AnyWindowHandle,
    view: &Entity<Editor>,
    dir: &std::path::Path,
    cx: &mut AsyncApp,
) -> Result<(), String> {
    handle
        .update(cx, |_, window, cx| {
            view.update(cx, |e, cx| {
                e.zoom = 60.;
                e.track_zoom = 1.;
                e.selected_track = Some(1);
                let ids = e.project.video().map(|c| c.id).collect::<Vec<_>>();
                for id in ids {
                    e.project.unlink(id);
                }
                e.timeline_scroll.set_offset(point(px(0.), px(0.)));
                e.track_labels_scroll.set_offset(point(px(0.), px(0.)));
                cx.notify();
            });
            window.render_frame(cx);
            window.click("add-video-track", cx);
            window.render_frame(cx);
            assert_eq!(
                view.read(cx)
                    .project
                    .tracks
                    .iter()
                    .map(|t| t.id)
                    .collect::<Vec<_>>(),
                vec![1, 3, 2]
            );
            let from = window.find(("clip", 1u64)).bounds().center();
            let target = window.find("track-3").bounds();
            window.drag(from, point(from.x, target.top() + px(40.)), cx);
            assert_eq!(
                view.read(cx).project.clip_track(1).unwrap().id,
                3,
                "片段应移入另一条视频轨"
            );
            assert!(view.read(cx).drag.is_none());
            window.press("ctrl-z", cx);
            assert_eq!(view.read(cx).project.clip_track(1).unwrap().id, 1);
            window.click("redo", cx);
            window.render_frame(cx);
            let from = window.find(("clip", 1u64)).bounds().center();
            let target = window.find("track-2").bounds();
            window.drag(from, point(from.x, target.top() + px(25.)), cx);
            assert_eq!(
                view.read(cx).project.clip_track(1).unwrap().id,
                3,
                "错误类型拖放不能移动片段"
            );
            assert!(view.read(cx).error.as_ref().unwrap().contains("视频只能"));
            view.update(cx, |e, cx| {
                e.error = None;
                cx.notify();
            });
            window.render_frame(cx);
            let from = window.find(("track-handle", 3u64)).bounds().center();
            let target = window.find(("track-handle", 1u64)).bounds().center();
            window.drag(from, target, cx);
            assert_eq!(
                view.read(cx).project.tracks[0].id,
                3,
                "拖动轨道标题应调整顺序"
            );
            window.click("add-audio-track", cx);
            window.render_frame(cx);
            let from = window.find(("asset", 0usize)).bounds().center();
            let target = window.find("track-3").bounds();
            let left = view.read(cx).timeline_scroll.bounds().left();
            window.drag(from, point(left + px(120.), target.top() + px(60.)), cx);
            assert_eq!(
                view.read(cx).project.clip_track(6).unwrap().id,
                3,
                "素材库拖放应创建片段"
            );
            assert!((view.read(cx).project.clip(6).unwrap().start - 2.).abs() < 0.02);
            assert_eq!(view.read(cx).project.clip_track(7).unwrap().id, 4);
            assert_eq!(view.read(cx).project.clip(7).unwrap().start, 2.);
            window.render_frame(cx);
            assert_eq!(
                view.read(cx)
                    .project
                    .tracks
                    .iter()
                    .filter(|t| t.kind == crate::model::TrackKind::Audio)
                    .count(),
                2
            );
        })
        .map_err(|e| e.to_string())?;
    wait(handle, view, cx, |e| !e.frame_decoding).await?;
    screenshot(handle, dir.join("09-multiple-tracks.png"), cx)?;
    let sound = dir.join("轨道配乐.wav");
    let path = sound.clone();
    let audio = cx
        .background_spawn(async move {
            let output = std::process::Command::new("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-y",
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=440:duration=1",
                ])
                .arg(&path)
                .output()
                .map_err(|e| e.to_string())?;
            if !output.status.success() {
                return Err(String::from_utf8_lossy(&output.stderr).into_owned());
            }
            media::probe(&path)
        })
        .await?;
    handle
        .update(cx, |_, window, cx| {
            view.update(cx, |e, cx| {
                e.project.media.push(audio);
                e.selected_track = Some(2);
                e.add(1, window, cx);
                e.timeline_scroll.set_offset(point(px(0.), px(-120.)));
                e.track_labels_scroll.set_offset(point(px(0.), px(-120.)));
                cx.notify();
            });
            window.render_frame(cx);
            let from = window.find(("clip", 8u64)).bounds().center();
            let target = window.find("track-4").bounds();
            window.drag(from, point(from.x, target.top() + px(30.)), cx);
            assert_eq!(
                view.read(cx).project.clip_track(8).unwrap().id,
                4,
                "音频应移入任意音频轨"
            );
            window.render_frame(cx);
            window.click(("track-delete", 4u64), cx);
            assert!(
                view.read(cx).project.clip(8).is_none(),
                "删除轨道应删除其中片段"
            );
            window.press("ctrl-z", cx);
            assert_eq!(
                view.read(cx).project.clip_track(8).unwrap().id,
                4,
                "撤销应恢复轨道与内容"
            );
            window.render_frame(cx);
            window.click(("track-up", 4u64), cx);
            assert_eq!(view.read(cx).project.tracks[0].id, 4);
            window.click(("track-down", 4u64), cx);
            assert_eq!(view.read(cx).project.tracks[1].id, 4);
            view.update(cx, |e, cx| {
                e.path = Some(dir.join("多轨项目.vig"));
                e.save(window, cx);
            });
        })
        .map_err(|e| e.to_string())?;
    wait(handle, view, cx, |e| !e.busy && !e.dirty).await?;
    let loaded = crate::model::Project::from_json(
        &std::fs::read_to_string(dir.join("多轨项目.vig")).map_err(|e| e.to_string())?,
    )?;
    loaded.validate()?;
    assert_eq!(loaded.tracks.len(), 4);
    assert_eq!(loaded.clip_track(8).unwrap().id, 4);
    screenshot(handle, dir.join("10-audio-tracks.png"), cx)?;
    handle
        .update(cx, |_, window, cx| {
            view.update(cx, |e, cx| {
                e.timeline_scroll.set_offset(point(px(0.), px(0.)));
                e.track_labels_scroll.set_offset(point(px(0.), px(0.)));
                cx.notify();
            });
            window.render_frame(cx);
            let position = point(
                view.read(cx).timeline_scroll.bounds().left() + px(420.),
                window.find("track-3").bounds().top() + px(50.),
            );
            let source = view.read(cx).project.media[0].path.clone();
            window.dispatch_event(
                FileDropEvent::Entered {
                    position,
                    paths: ExternalPaths([source].into_iter().collect()),
                }
                .to_platform_input(),
                cx,
            );
            window.dispatch_event(FileDropEvent::Submit { position }.to_platform_input(), cx);
            window.dispatch_event(FileDropEvent::Ended.to_platform_input(), cx);
        })
        .map_err(|e| e.to_string())?;
    wait(handle, view, cx, |e| !e.busy && e.project.clip(9).is_some()).await?;
    handle
        .update(cx, |_, _, cx| {
            let e = view.read(cx);
            assert_eq!(
                e.project.clip_track(9).unwrap().id,
                3,
                "外部文件应进入鼠标所在轨道"
            );
            assert!(
                (e.project.clip(9).unwrap().start - 7.).abs() < 0.02,
                "外部拖入应使用实际鼠标时间位置"
            );
            assert_eq!(e.project.media.len(), 2, "外部拖入仍应复用素材");
            let sound = e.project.clip(10).unwrap();
            assert_eq!(sound.start, 7.);
            assert_eq!(sound.source_out, e.project.clip(9).unwrap().source_out);
            let track = e.project.clip_track(10).unwrap();
            assert_eq!(track.kind, crate::model::TrackKind::Audio);
            assert_ne!(track.id, 2, "已有音频轨区间被占用时不能重叠放置");
            assert_ne!(track.id, 4);
            assert_eq!(e.project.tracks.len(), 5, "音频轨均不可用时应自动创建");
        })
        .map_err(|e| e.to_string())?;
    screenshot(handle, dir.join("11-auto-audio-track.png"), cx)?;
    Ok(())
}
