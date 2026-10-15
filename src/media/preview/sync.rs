use std::sync::mpsc::{Receiver, TryRecvError};

type Frame = (Vec<u8>, f64);

/// Keep the newest decoded frame whose display interval contains the audio clock.
/// Future frames remain queued; expired frames are discarded, never replayed late.
pub(super) struct FrameScheduler {
    frames: Receiver<Result<Frame, String>>,
    pending: Option<Frame>,
    closed: bool,
    frame_duration: f64,
}

impl FrameScheduler {
    pub(super) fn new(frames: Receiver<Result<Frame, String>>, frame_duration: f64) -> Self {
        Self {
            frames,
            pending: None,
            closed: false,
            frame_duration,
        }
    }

    pub(super) fn at(&mut self, clock: f64) -> Result<Option<Frame>, String> {
        let mut latest = None;
        loop {
            let frame = if let Some(frame) = self.pending.take() {
                frame
            } else {
                match self.frames.try_recv() {
                    Ok(frame) => frame?,
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        self.closed = true;
                        break;
                    }
                }
            };
            if frame.1 > clock {
                self.pending = Some(frame);
                break;
            }
            latest = Some(frame);
        }
        Ok(latest.filter(|(_, time)| clock < time + self.frame_duration))
    }

    pub(super) fn finished(&self) -> bool {
        self.closed && self.pending.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_expiry_uses_the_project_frame_rate() {
        for (fps, visible) in [(24., true), (60., false)] {
            let (sender, receiver) = std::sync::mpsc::channel();
            sender.send(Ok((vec![1], 0.))).unwrap();
            drop(sender);
            let mut scheduler = FrameScheduler::new(receiver, 1. / fps);
            assert_eq!(scheduler.at(0.035).unwrap().is_some(), visible);
        }
    }
    use std::sync::mpsc;

    #[test]
    fn stall_discards_old_frames_and_preserves_the_next_future_frame() {
        let (sender, receiver) = mpsc::channel();
        for index in 0..10 {
            sender.send(Ok((vec![index], index as f64 / 30.))).unwrap();
        }
        let mut scheduler = FrameScheduler::new(receiver, 1. / 30.);
        assert_eq!(scheduler.at(0.).unwrap().unwrap().0, vec![0]);
        assert_eq!(scheduler.at(0.205).unwrap().unwrap().0, vec![6]);
        assert!(scheduler.at(0.21).unwrap().is_none());
        assert_eq!(scheduler.at(0.24).unwrap().unwrap().0, vec![7]);
    }

    #[test]
    fn stale_frames_are_not_shown_when_decoding_cannot_catch_up() {
        let (sender, receiver) = mpsc::channel();
        sender.send(Ok((vec![0], 0.))).unwrap();
        let mut scheduler = FrameScheduler::new(receiver, 1. / 30.);
        assert!(scheduler.at(1.).unwrap().is_none());
        sender.send(Ok((vec![1], 1.))).unwrap();
        assert_eq!(scheduler.at(1.01).unwrap().unwrap().0, vec![1]);
        drop(sender);
        assert!(scheduler.at(1.02).unwrap().is_none());
        assert!(scheduler.finished());
    }

    #[test]
    fn a_frozen_audio_clock_does_not_present_future_video_or_hide_errors() {
        let (sender, receiver) = mpsc::channel();
        sender.send(Ok((vec![0], 0.))).unwrap();
        sender.send(Ok((vec![1], 1. / 30.))).unwrap();
        let mut scheduler = FrameScheduler::new(receiver, 1. / 30.);
        assert_eq!(scheduler.at(0.).unwrap().unwrap().0, vec![0]);
        assert!(scheduler.at(0.).unwrap().is_none());
        assert_eq!(scheduler.at(1. / 30.).unwrap().unwrap().0, vec![1]);
        sender.send(Err("解码失败".into())).unwrap();
        assert_eq!(scheduler.at(0.1).unwrap_err(), "解码失败");
    }
}
