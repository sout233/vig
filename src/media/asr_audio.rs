use super::*;

/// Render a short range through the same graph used for playback and export.
pub fn samples(project: &Project, start: f64, end: f64) -> Result<Vec<f32>, String> {
    let mut command = filter_command(
        project,
        start,
        end,
        project.settings().dimensions(),
        Some(crate::model::TrackKind::Audio),
    )?;
    let output = command
        .args(["-f", "f32le", "-ar", "16000", "-ac", "1", "pipe:1"])
        .output()
        .map_err(|e| format!("无法解码字幕源音频：{e}"))?;
    if !output.status.success() {
        return Err(format!(
            "字幕源音频解码失败：{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    if output.stdout.is_empty() || output.stdout.len() % 4 != 0 {
        return Err("字幕源音频数据不完整".into());
    }
    Ok(output
        .stdout
        .chunks_exact(4)
        .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::samples;
    use crate::{media, model::Project};

    #[test]
    fn subtitle_audio_uses_timeline_trim_speed_fades_and_silent_gaps() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("tone.wav");
        let output = media::command("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=4",
                "-y",
            ])
            .arg(&path)
            .output()
            .unwrap();
        assert!(output.status.success());
        let mut project = Project::default();
        project.media.push(media::probe(&path).unwrap());
        let id = project.insert_media(0, Some(2), Some(2.)).unwrap();
        let clip = project.clip_mut(id).unwrap();
        clip.source_in = 1.;
        clip.source_out = 3.;
        clip.speed = 2.;
        clip.fade_in = 0.5;
        let audio = samples(&project, 1.5, 3.).unwrap();
        assert_eq!(audio.len(), 24_000);
        assert!(audio[..8_000].iter().all(|sample| sample.abs() < 0.00001));
        let energy = |slice: &[f32]| {
            slice
                .iter()
                .map(|sample| f64::from(*sample).powi(2))
                .sum::<f64>()
                / slice.len() as f64
        };
        assert!(energy(&audio[17_000..23_000]) > energy(&audio[8_000..10_000]) * 4.);
        project.clip_mut(id).unwrap().muted = true;
        assert!(
            samples(&project, 2., 3.)
                .unwrap()
                .iter()
                .all(|sample| sample.abs() < 0.00001)
        );
    }
}
