use super::*;

pub(super) async fn check(
    handle: AnyWindowHandle,
    view: &Entity<Editor>,
    dir: &std::path::Path,
    cx: &mut AsyncApp,
) -> Result<(), String> {
    handle
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            let handle = window.find(("fade-in", 9u64)).bounds().center();
            window.drag(handle, handle + point(px(30.), px(0.)), cx);
            assert!((view.read(cx).project.clip(9).unwrap().fade_in - 0.5).abs() < 0.02);
            assert_eq!(view.read(cx).project.clip(9).unwrap().fade_out, 0.);
            window.render_frame(cx);
            let handle = window.find(("fade-out", 9u64)).bounds().center();
            window.drag(handle, handle - point(px(60.), px(0.)), cx);
            assert!((view.read(cx).project.clip(9).unwrap().fade_out - 1.).abs() < 0.02);
            assert!(
                (view.read(cx).project.clip(9).unwrap().start - 8.).abs() < 0.02,
                "角落拖拽不能移动片段"
            );
            assert!(
                (view.read(cx).project.clip(9).unwrap().duration() - 2.5).abs() < 0.02,
                "角落拖拽不能裁剪片段"
            );
            assert_eq!(view.read(cx).project.clip(10).unwrap().fade_in, 0.);
            assert_eq!(view.read(cx).project.clip(10).unwrap().fade_out, 0.);
            window.press("ctrl-z", cx);
            assert_eq!(view.read(cx).project.clip(9).unwrap().fade_out, 0.);
            window.click("redo", cx);
            assert!((view.read(cx).project.clip(9).unwrap().fade_out - 1.).abs() < 0.02);
            view.update(cx, |e, cx| {
                e.seek(8.25, cx);
                e.path = Some(dir.join("淡入淡出.vig"));
                e.save(window, cx);
            });
        })
        .map_err(|e| e.to_string())?;
    wait(handle, view, cx, |e| {
        !e.busy && !e.frame_decoding && !e.dirty
    })
    .await?;
    let project = crate::model::Project::from_json(
        &std::fs::read_to_string(dir.join("淡入淡出.vig")).map_err(|e| e.to_string())?,
    )?;
    project.validate()?;
    assert!((project.clip(9).unwrap().fade_in - 0.5).abs() < 0.02);
    assert!((project.clip(9).unwrap().fade_out - 1.).abs() < 0.02);
    screenshot(handle, dir.join("13-corner-fades.png"), cx)?;
    Ok(())
}
