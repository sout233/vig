use super::*;
use std::ops::{Deref, DerefMut};

pub(super) struct FilterCommand {
    command: Command,
    files: Vec<tempfile::NamedTempFile>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subtitles_render_chinese_multiline_and_literal_special_characters() {
        let mut project = Project::default();
        let id = project.add_subtitle(0.5).unwrap();
        let media = project.clip(id).unwrap().media;
        project.media[media]
            .generator
            .as_mut()
            .unwrap()
            .as_text_mut()
            .unwrap()
            .text = "你好，字幕 100% {测试}\n第二行：'quoted' \\literal".into();
        let bytes = snapshot(&project, 0.75, (640, 360)).unwrap();
        let decoded = image::load_from_memory(&bytes).unwrap().into_rgb8();
        assert_eq!(decoded.dimensions(), (640, 360));
        assert!(
            decoded
                .pixels()
                .filter(|p| p[0] > 180 && p[1] > 180 && p[2] > 180)
                .count()
                > 100
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("subtitle.mp4");
        let (tx, _) = std::sync::mpsc::channel();
        render_range(
            &project,
            &path,
            true,
            0.25,
            1.25,
            Arc::new(AtomicBool::new(false)),
            &tx,
        )
        .unwrap();
        let before = image::load_from_memory(&frame(&path, 0.1).unwrap())
            .unwrap()
            .into_rgb8();
        let during = image::load_from_memory(&frame(&path, 0.6).unwrap())
            .unwrap()
            .into_rgb8();
        assert!(before.pixels().all(|p| p[0] < 20 && p[1] < 20 && p[2] < 20));
        assert!(during.pixels().filter(|p| p[0] > 180).count() > 100);
    }

    #[test]
    fn generated_text_fades_and_transform_use_the_same_render_graph() {
        let mut project = Project::default();
        let id = project.add_subtitle(0.).unwrap();
        project.clip_mut(id).unwrap().fade_in = 1.;
        let energy = |project: &Project, time| {
            image::load_from_memory(&snapshot(project, time, (640, 360)).unwrap())
                .unwrap()
                .into_rgb8()
                .pixels()
                .map(|pixel| u64::from(pixel[0]))
                .sum::<u64>()
        };
        let early = energy(&project, 0.2);
        let late = energy(&project, 0.8);
        assert!(late > early * 2, "生成式文字应随片段淡入逐渐变亮");
        project.clip_mut(id).unwrap().transform.opacity = 0.5;
        assert!(energy(&project, 0.8) < late * 3 / 4);
        let clip = project.clip_mut(id).unwrap();
        clip.fade_in = 0.;
        clip.transform =
            crate::model::Transform::from_values([-20., -20., 80., 30., 100.]).unwrap();
        let transformed = image::load_from_memory(&snapshot(&project, 1., (640, 360)).unwrap())
            .unwrap()
            .into_rgb8();
        // Resampling rotated, scaled glyphs creates antialiased edge pixels.
        let points: Vec<_> = transformed
            .enumerate_pixels()
            .filter(|(_, _, pixel)| pixel[0] > 64)
            .collect();
        assert!(points.len() > 100);
        assert!(
            points.iter().map(|(x, _, _)| u64::from(*x)).sum::<u64>() / (points.len() as u64) < 320
        );
        assert!(
            points.iter().map(|(_, y, _)| u64::from(*y)).sum::<u64>() / (points.len() as u64) < 300
        );
    }
}

impl FilterCommand {
    pub(super) fn plain(command: Command) -> Self {
        Self {
            command,
            files: vec![],
        }
    }

    pub(super) fn keep(&mut self, file: tempfile::NamedTempFile) {
        self.files.push(file);
    }
}

impl Deref for FilterCommand {
    type Target = Command;

    fn deref(&self) -> &Command {
        &self.command
    }
}

impl DerefMut for FilterCommand {
    fn deref_mut(&mut self) -> &mut Command {
        &mut self.command
    }
}

fn filter_path(path: &Path) -> String {
    let path = path.to_string_lossy().replace('\\', "/");
    format!("'{}'", path.replace(':', "\\:").replace('\'', "'\\''"))
}

pub(super) fn append_text(
    text_source: &crate::model::TextGenerator,
    height: u32,
    alpha: &str,
    graph: &mut String,
    command: &mut FilterCommand,
) -> Result<(), String> {
    use std::io::Write;
    let mut text = tempfile::Builder::new()
        .suffix(".txt")
        .tempfile()
        .map_err(|e| e.to_string())?;
    text.write_all(text_source.text.as_bytes())
        .map_err(|e| e.to_string())?;
    text.flush().map_err(|e| e.to_string())?;
    let size = text_source.font_size * f64::from(height) / 1080.;
    let border = (size / 24.).round().max(1.);
    graph.push_str(&format!(",drawtext=fontfile={}:textfile={}:expansion=none:fontsize={size:.3}:fontcolor=0x{}:borderw={border:.0}:bordercolor=black:x=(w-text_w)/2:y=h*0.9-text_h:alpha='{alpha}'",
            filter_path(&text_source.font_file), filter_path(text.path()), text_source.color));
    command.keep(text);
    Ok(())
}

pub fn snapshot(project: &Project, time: f64, dimensions: (u32, u32)) -> Result<Vec<u8>, String> {
    let mut command = super::filter_command(
        project,
        time,
        (time + project.settings().frame_rate().frame_duration()).min(project.duration()),
        dimensions,
        Some(crate::model::TrackKind::Video),
    )?;
    let result = command
        .args([
            "-frames:v",
            "1",
            "-c:v",
            "png",
            "-f",
            "image2pipe",
            "pipe:1",
        ])
        .output()
        .map_err(|e| e.to_string())?;
    if !result.status.success() {
        return Err(format!(
            "预览失败：{}",
            String::from_utf8_lossy(&result.stderr)
        ));
    }
    Ok(result.stdout)
}
