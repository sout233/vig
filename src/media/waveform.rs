use super::*;

pub const SAMPLE_RATE: f64 = 48_000.;

const BUCKET_FRAMES: u64 = 256;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Peak {
    pub min: f32,
    pub max: f32,
}

impl Peak {
    fn merge(self, other: Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }
}

pub struct Waveform {
    pub frames: u64,
    levels: Vec<Vec<Peak>>,
}

impl Waveform {
    fn from_peaks(frames: u64, mut base: Vec<Peak>) -> Self {
        base.shrink_to_fit();
        let mut levels = vec![base];
        while levels.last().unwrap().len() > 1 {
            let next = levels
                .last()
                .unwrap()
                .chunks(2)
                .map(|pair| {
                    if pair.len() == 2 {
                        pair[0].merge(pair[1])
                    } else {
                        pair[0]
                    }
                })
                .collect();
            levels.push(next);
        }
        Self { frames, levels }
    }

    pub fn peak(&self, start: f64, end: f64) -> Peak {
        let start = (start.max(0.) * SAMPLE_RATE).floor() as u64;
        let end = ((end.max(0.) * SAMPLE_RATE).ceil() as u64).min(self.frames);
        if start >= end {
            return Peak::default();
        }
        let mut level = 0;
        let mut bucket = BUCKET_FRAMES;
        while level + 1 < self.levels.len() && bucket * 2 <= end - start {
            bucket *= 2;
            level += 1;
        }
        let peaks = &self.levels[level];
        let first = (start / bucket) as usize;
        let last = end.div_ceil(bucket) as usize;
        peaks[first..last]
            .iter()
            .copied()
            .reduce(Peak::merge)
            .unwrap()
    }
}

pub fn generate(path: &Path, cancel: &AtomicBool) -> Result<Waveform, String> {
    let mut command = super::command("ffmpeg");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Waveform analysis should yield CPU time to interactive playback.
        command.creation_flags(0x08000000 | 0x00004000);
    }
    let mut child = command
        .args(["-v", "error", "-nostdin", "-threads", "1", "-i"])
        .arg(path)
        .args([
            "-map", "0:a:0", "-vn", "-ac", "2", "-ar", "48000", "-f", "f32le", "pipe:1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("无法解码波形：{e}"))?;
    let mut errors = child.stderr.take().unwrap();
    let error_reader = std::thread::spawn(move || {
        let mut text = String::new();
        errors
            .read_to_string(&mut text)
            .map(|_| text)
            .map_err(|e| e.to_string())
    });
    let result = read_peaks(child.stdout.take().unwrap(), cancel);
    if result.is_err() && child.try_wait().map_err(|e| e.to_string())?.is_none() {
        child.kill().map_err(|e| e.to_string())?;
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let stderr = error_reader
        .join()
        .map_err(|_| "波形错误读取线程异常退出")??;
    if !status.success() && !cancel.load(Ordering::Relaxed) {
        let detail = result.err().unwrap_or_default();
        return Err(format!("波形解码失败（{status}）：{stderr} {detail}"));
    }
    result
}

fn read_peaks(mut pcm: impl Read, cancel: &AtomicBool) -> Result<Waveform, String> {
    let mut bytes = [0u8; 8192];
    let mut carried = 0;
    let mut frames = 0;
    let mut peaks = Vec::new();
    let mut peak = Peak::default();
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("波形生成已取消".into());
        }
        let count = pcm.read(&mut bytes[carried..]).map_err(|e| e.to_string())?;
        if count == 0 {
            if carried != 0 {
                return Err("波形 PCM 数据不完整".into());
            }
            break;
        }
        let total = carried + count;
        let complete = total / 8 * 8;
        for frame in bytes[..complete].chunks_exact(8) {
            let left = f32::from_le_bytes(frame[..4].try_into().unwrap());
            let right = f32::from_le_bytes(frame[4..].try_into().unwrap());
            if !left.is_finite() || !right.is_finite() {
                return Err("音频波形包含无效采样".into());
            }
            let sample = Peak {
                min: left.min(right),
                max: left.max(right),
            };
            peak = if frames % BUCKET_FRAMES == 0 {
                sample
            } else {
                peak.merge(sample)
            };
            frames += 1;
            if frames % BUCKET_FRAMES == 0 {
                peaks.push(peak);
            }
        }
        bytes.copy_within(complete..total, 0);
        carried = total - complete;
    }
    if frames == 0 {
        return Err("音频没有可绘制的采样".into());
    }
    if frames % BUCKET_FRAMES != 0 {
        peaks.push(peak);
    }
    Ok(Waveform::from_peaks(frames, peaks))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stereo_peaks_keep_antiphase_and_partial_final_bucket() {
        let samples = (0..300)
            .flat_map(|i| {
                let amplitude: f32 = if i == 299 { 0.9 } else { 0.25 };
                [amplitude.to_le_bytes(), (-amplitude).to_le_bytes()].concat()
            })
            .collect::<Vec<_>>();
        let waveform = read_peaks(std::io::Cursor::new(samples), &AtomicBool::new(false)).unwrap();
        assert_eq!(waveform.frames, 300);
        assert_eq!(
            waveform.peak(0., 256. / SAMPLE_RATE),
            Peak {
                min: -0.25,
                max: 0.25
            }
        );
        assert_eq!(
            waveform.peak(256. / SAMPLE_RATE, 300. / SAMPLE_RATE),
            Peak {
                min: -0.9,
                max: 0.9
            }
        );
        assert_eq!(waveform.peak(0., 300. / SAMPLE_RATE).max, 0.9);
        assert_eq!(waveform.peak(1., 2.), Peak::default());
    }

    #[test]
    fn streaming_handles_short_reads_and_rejects_truncated_data() {
        struct ShortReads(std::io::Cursor<Vec<u8>>);

        impl Read for ShortReads {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                let count = buffer.len().min(3);
                self.0.read(&mut buffer[..count])
            }
        }
        let bytes = [0.5f32.to_le_bytes(), (-0.5f32).to_le_bytes()].concat();
        let cancel = AtomicBool::new(false);
        assert_eq!(
            read_peaks(ShortReads(std::io::Cursor::new(bytes.clone())), &cancel)
                .unwrap()
                .frames,
            1
        );
        assert!(read_peaks(std::io::Cursor::new(&bytes[..7]), &cancel).is_err());
        cancel.store(true, Ordering::Relaxed);
        assert!(read_peaks(std::io::Cursor::new(bytes), &cancel).is_err());
    }

    #[test]
    fn actual_decoder_keeps_silence_and_tone_ranges() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("tone.wav");
        let output = super::super::command("ffmpeg").args(["-v", "error", "-f", "lavfi", "-i",
            "aevalsrc=0.5*sin(2*PI*440*t)*gte(t\\,0.1)|0.5*sin(2*PI*440*t)*gte(t\\,0.1):s=48000:d=0.3",
            "-c:a", "pcm_f32le"]).arg(&file).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let waveform = generate(&file, &AtomicBool::new(false)).unwrap();
        assert_eq!(waveform.frames, 14_400);
        assert_eq!(waveform.peak(0.02, 0.06), Peak::default());
        let tone = waveform.peak(0.15, 0.2);
        assert!(tone.min < -0.49 && tone.max > 0.49, "{tone:?}");
    }

    #[test]
    fn pyramid_storage_stays_below_twice_the_base_data() {
        let base = vec![
            Peak {
                min: -0.5,
                max: 0.5
            };
            10_001
        ];
        let waveform = Waveform::from_peaks(10_001 * BUCKET_FRAMES, base);
        let total: usize = waveform.levels.iter().map(Vec::len).sum();
        assert!(total <= 20_002 + waveform.levels.len());
        assert_eq!(waveform.peak(0., 50.).max, 0.5);
    }
}
