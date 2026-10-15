use super::*;

/// Decode ahead of presentation, so the next segment opens before its first frame is due.
pub(super) fn decode_video(
    segment: &mut Segment,
    sender: &std::sync::mpsc::SyncSender<Result<(Vec<u8>, f64), String>>,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let mut child = segment
        .command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut errors = child.stderr.take().unwrap();
    let error_reader = std::thread::spawn(move || {
        let mut text = String::new();
        errors
            .read_to_string(&mut text)
            .map(|_| text)
            .map_err(|e| e.to_string())
    });
    let mut frames = child.stdout.take().unwrap();
    let mut ended_at_boundary = false;
    let result: Result<(), String> = (|| {
        let mut index = 0;
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Ok(());
            }
            let position = segment.start + index as f64 * segment.frame_rate.frame_duration();
            if position >= segment.end {
                ended_at_boundary = true;
                break;
            }
            let mut pixels =
                vec![0; segment.dimensions.0 as usize * segment.dimensions.1 as usize * 4];
            match frames.read_exact(&mut pixels) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e.to_string()),
            }
            send_ahead(sender, Ok((pixels, position)), cancel)?;
            index += 1;
        }
        if index == 0 {
            return Err("预览区间没有可播放的视频帧".into());
        }
        Ok(())
    })();
    if (result.is_err() || cancel.load(Ordering::Relaxed) || ended_at_boundary)
        && child.try_wait().map_err(|e| e.to_string())?.is_none()
    {
        child.kill().map_err(|e| e.to_string())?;
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let errors = error_reader
        .join()
        .map_err(|_| "预览视频错误读取线程异常")??;
    result?;
    if !status.success() && !cancel.load(Ordering::Relaxed) && !ended_at_boundary {
        return Err(format!("预览视频解码失败：{errors}"));
    }
    Ok(())
}
