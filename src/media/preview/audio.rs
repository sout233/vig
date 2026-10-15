use super::*;

/// Deliver 100 ms of PCM at a time instead of holding the first packet until a
/// whole two-second preview has been rendered and encoded.
const PACKET_SAMPLES: usize = 4800 * 2;

pub(super) fn load(
    project: &Project,
    start: f64,
    end: f64,
    cached: Option<(&Path, f64)>,
    sender: &std::sync::mpsc::SyncSender<Vec<f32>>,
    cancel: &AtomicBool,
) -> Result<(), String> {
    use rodio::Source;
    let expected = ((end - start) * 48000.).round() as usize * 2;
    if let Some((path, offset)) = cached {
        let mut decoder =
            rodio::Decoder::try_from(std::fs::File::open(path).map_err(|e| e.to_string())?)
                .map_err(|e| format!("预览音频解码失败：{e}"))?;
        if decoder.channels() != 2 || decoder.sample_rate() != 48000 {
            return Err("预览音频格式错误，预期 48000 Hz 立体声".into());
        }
        decoder
            .try_seek(Duration::from_secs_f64(offset))
            .map_err(|e| e.to_string())?;
        let mut remaining = expected;
        while remaining > 0 {
            let count = remaining.min(PACKET_SAMPLES);
            let samples: Vec<_> = decoder.by_ref().take(count).collect();
            if samples.len() != count {
                return Err("预览音频长度不足".into());
            }
            send_ahead(sender, samples, cancel)?;
            remaining -= count;
        }
        return Ok(());
    }
    let mut command = filter_command(
        project,
        start,
        end,
        project.settings().dimensions(),
        Some(crate::model::TrackKind::Audio),
    )?;
    let mut child = command
        .args(["-f", "f32le", "-ar", "48000", "-ac", "2", "pipe:1"])
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
    let mut pcm = child.stdout.take().unwrap();
    let result: Result<(), String> = (|| {
        let mut remaining = expected;
        while remaining > 0 {
            if cancel.load(Ordering::Relaxed) {
                return Err("操作已取消".into());
            }
            let count = remaining.min(PACKET_SAMPLES);
            let mut bytes = vec![0; count * 4];
            pcm.read_exact(&mut bytes)
                .map_err(|e| format!("预览音频读取失败：{e}"))?;
            let samples = bytes
                .chunks_exact(4)
                .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
                .collect();
            send_ahead(sender, samples, cancel)?;
            remaining -= count;
        }
        Ok(())
    })();
    if (result.is_err() || cancel.load(Ordering::Relaxed))
        && child.try_wait().map_err(|e| e.to_string())?.is_none()
    {
        child.kill().map_err(|e| e.to_string())?;
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let errors = error_reader
        .join()
        .map_err(|_| "预览音频错误读取线程异常")??;
    result?;
    if !status.success() {
        return Err(format!("预览音频解码失败：{errors}"));
    }
    Ok(())
}
