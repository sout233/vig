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
            assert_eq!(
                view.read(cx).project.linked_ids(9).len(),
                2,
                "拆分音频应默认绑定"
            );
            let clip = window.find(("clip", 9u64)).bounds();
            window.drag(clip.center(), clip.center() + point(px(60.), px(0.)), cx);
            assert!((view.read(cx).project.clip(9).unwrap().start - 9.).abs() < 0.02);
            assert!((view.read(cx).project.clip(10).unwrap().start - 9.).abs() < 0.02);
            window.render_frame(cx);
            let clip = window.find(("clip", 10u64)).bounds();
            window.drag(clip.center(), clip.center() + point(px(30.), px(0.)), cx);
            assert!(
                (view.read(cx).project.clip(9).unwrap().start - 9.5).abs() < 0.02,
                "拖音频也应移动视频"
            );
            assert!((view.read(cx).project.clip(10).unwrap().start - 9.5).abs() < 0.02);
            window.right_click(("clip", 9u64), cx);
        })
        .map_err(|e| e.to_string())?;
    screenshot(handle, dir.join("14-clip-context-menu.png"), cx)?;
    handle
        .update(cx, |_, window, cx| window.click(0usize, cx))
        .map_err(|e| e.to_string())?;
    wait(handle, view, cx, |e| {
        e.project.clip(9).unwrap().group.is_none()
    })
    .await?;
    handle
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            let clip = window.find(("clip", 9u64)).bounds();
            window.drag(clip.center(), clip.center() + point(px(30.), px(0.)), cx);
            assert!((view.read(cx).project.clip(9).unwrap().start - 10.).abs() < 0.02);
            assert!(
                (view.read(cx).project.clip(10).unwrap().start - 9.5).abs() < 0.02,
                "解绑后应独立拖动"
            );
            window.right_click(("clip", 9u64), cx);
        })
        .map_err(|e| e.to_string())?;
    handle
        .update(cx, |_, window, cx| window.click(1usize, cx))
        .map_err(|e| e.to_string())?;
    wait(handle, view, cx, |e| e.project.linked_ids(9).len() == 2).await?;
    let copied = handle
        .update(cx, |_, window, cx| {
            let copied = view.read(cx).project.next_id;
            window.right_click(("clip", 9u64), cx);
            copied
        })
        .map_err(|e| e.to_string())?;
    handle
        .update(cx, |_, window, cx| window.click(2usize, cx))
        .map_err(|e| e.to_string())?;
    wait(handle, view, cx, |e| e.project.clip(copied).is_some()).await?;
    handle
        .update(cx, |_, window, cx| {
            assert_eq!(
                view.read(cx).project.linked_ids(copied).len(),
                2,
                "复制应保留独立的新绑定组"
            );
            window.press("ctrl-z", cx);
            assert!(view.read(cx).project.clip(copied).is_none());
            window.right_click(("clip", 9u64), cx);
        })
        .map_err(|e| e.to_string())?;
    handle
        .update(cx, |_, window, cx| window.click(6usize, cx))
        .map_err(|e| e.to_string())?;
    wait(handle, view, cx, |e| e.project.clip(10).unwrap().muted).await?;
    handle
        .update(cx, |_, window, cx| {
            view.update(cx, |e, cx| {
                e.path = Some(dir.join("绑定项目.vig"));
                e.save(window, cx);
            });
        })
        .map_err(|e| e.to_string())?;
    wait(handle, view, cx, |e| !e.busy && !e.dirty).await?;
    let loaded = crate::model::Project::from_json(
        &std::fs::read_to_string(dir.join("绑定项目.vig")).map_err(|e| e.to_string())?,
    )?;
    assert_eq!(loaded.linked_ids(9).len(), 2);
    loaded.validate()?;
    Ok(())
}
