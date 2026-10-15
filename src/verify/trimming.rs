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
                e.snap = false;
                cx.notify();
            });
            window.render_frame(cx);
            let head = window.find(("trim-in", 9u64)).bounds().center();
            window.drag(head, head + point(px(60.), px(0.)), cx);
            let e = view.read(cx);
            let video = e.project.clip(9).unwrap();
            assert!((video.start - 8.).abs() < 0.02);
            assert!((video.source_in - 1.).abs() < 0.02);
            assert!((video.end() - 11.).abs() < 0.02, "拖动头部应保持尾部不动");
            assert_eq!(
                e.project.clip(10).unwrap().start,
                video.start,
                "视频裁剪应同步绑定音频块"
            );
            assert_eq!(e.project.clip(10).unwrap().source_in, video.source_in);
            assert!(e.drag.is_none());
            window.press("ctrl-z", cx);
            assert_eq!(view.read(cx).project.clip(9).unwrap().start, 7.);
            window.click("redo", cx);
            assert!((view.read(cx).project.clip(9).unwrap().start - 8.).abs() < 0.02);
            window.render_frame(cx);
            let tail = window.find(("trim-out", 9u64)).bounds().center();
            window.drag(tail, tail - point(px(30.), px(0.)), cx);
            let e = view.read(cx);
            let video = e.project.clip(9).unwrap();
            assert!((video.end() - 10.5).abs() < 0.02);
            assert!((video.source_out - 3.5).abs() < 0.02);
            assert_eq!(e.project.clip(10).unwrap().source_out, video.source_out);
            assert!((video.start - 8.).abs() < 0.02, "拖动尾部应保持头部不动");
            let snapshot = serde_json::to_string(&e.project).unwrap();
            let history = e.undo.len();
            window.render_frame(cx);
            let tail = window.find(("trim-out", 9u64)).bounds().center();
            window.drag(tail, tail + point(px(120.), px(0.)), cx);
            let e = view.read(cx);
            assert!(e.error.is_none(), "拖过源素材边界应停住，不报裁剪失败");
            assert_eq!(
                e.project.clip(9).unwrap().source_out,
                e.project.media[e.project.clip(9).unwrap().media].duration
            );
            assert_ne!(snapshot, serde_json::to_string(&e.project).unwrap());
            assert_eq!(e.undo.len(), history + 1, "到达素材边界的裁剪可撤销");
            window.press("ctrl-z", cx);
            assert_eq!(
                snapshot,
                serde_json::to_string(&view.read(cx).project).unwrap()
            );
            view.update(cx, |e, cx| {
                e.error = None;
                cx.notify();
            });
        })
        .map_err(|e| e.to_string())?;
    wait(handle, view, cx, |e| {
        let clip = e.project.clip(9).unwrap();
        clip.preview_times().iter().all(|time| {
            e.filmstrip
                .contains_key(&(e.project.media[clip.media].path.clone(), *time))
        })
    })
    .await?;
    screenshot(handle, dir.join("12-edge-trimming.png"), cx)?;
    Ok(())
}
