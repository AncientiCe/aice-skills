//! Microphone capture using `cpal`.
//!
//! The backend expects 16 kHz mono signed-16 PCM, so we resample with a
//! naive averaging downsampler. For a demo this is good enough; a
//! production frontend should use `rubato` or similar for proper
//! anti-aliased resampling.
//!
//! Capture errors are downgraded to a warning: the bridge keeps running
//! with no mic, the browser UI just shows the "no audio" state, and the
//! user can still drive turns through any external audio source.

use crate::BACKEND_SAMPLE_RATE;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use std::sync::{Arc, Mutex};
use thiserror::Error;
use tokio::sync::mpsc;

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("no input device available")]
    NoInputDevice,
    #[error("default input config failed: {0}")]
    NoConfig(String),
    #[error("build_input_stream failed: {0}")]
    BuildStream(String),
    #[error("stream play failed: {0}")]
    Play(String),
    #[error("unsupported sample format: {0:?}")]
    UnsupportedSampleFormat(SampleFormat),
}

/// Spawn a microphone capture and forward 16 kHz mono int16 frames over
/// the returned channel. Capture stops when the returned `_stream` is
/// dropped (kept alive for as long as the bridge is running).
pub fn spawn_capture(
    chunk_size_samples: usize,
) -> Result<(mpsc::UnboundedReceiver<Vec<i16>>, cpal::Stream), AudioError> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or(AudioError::NoInputDevice)?;
    let supported = device
        .default_input_config()
        .map_err(|error| AudioError::NoConfig(error.to_string()))?;
    let sample_format = supported.sample_format();
    let mut config: StreamConfig = supported.into();
    config.channels = config.channels.max(1);

    let (tx, rx) = mpsc::unbounded_channel();
    let buffer = Arc::new(Mutex::new(ResampleBuffer::new(
        config.sample_rate.0,
        config.channels,
        chunk_size_samples,
    )));

    let stream = match sample_format {
        SampleFormat::F32 => {
            let buffer = buffer.clone();
            let tx = tx.clone();
            device
                .build_input_stream(
                    &config,
                    move |data: &[f32], _| {
                        let Ok(mut buf) = buffer.lock() else { return };
                        for &sample in data {
                            let clamped = sample.clamp(-1.0, 1.0);
                            buf.push(((clamped * 32767.0) as i32).clamp(-32768, 32767) as i16);
                        }
                        buf.flush_full(&tx);
                    },
                    move |error| tracing::warn!(%error, "cpal stream error"),
                    None,
                )
                .map_err(|error| AudioError::BuildStream(error.to_string()))?
        }
        SampleFormat::I16 => {
            let buffer = buffer.clone();
            let tx = tx.clone();
            device
                .build_input_stream(
                    &config,
                    move |data: &[i16], _| {
                        let Ok(mut buf) = buffer.lock() else { return };
                        for &sample in data {
                            buf.push(sample);
                        }
                        buf.flush_full(&tx);
                    },
                    move |error| tracing::warn!(%error, "cpal stream error"),
                    None,
                )
                .map_err(|error| AudioError::BuildStream(error.to_string()))?
        }
        other => return Err(AudioError::UnsupportedSampleFormat(other)),
    };

    stream
        .play()
        .map_err(|error| AudioError::Play(error.to_string()))?;
    Ok((rx, stream))
}

/// Channel-aware downsampling buffer that emits chunks of mono i16
/// at [`BACKEND_SAMPLE_RATE`].
pub struct ResampleBuffer {
    channels: u16,
    chunk_size: usize,
    accumulator: Vec<i16>,
    samples_per_channel_seen: u32,
    next_emit_at: f64,
    step: f64,
    output: Vec<i16>,
}

impl ResampleBuffer {
    pub fn new(input_rate: u32, channels: u16, chunk_size: usize) -> Self {
        let step = input_rate as f64 / BACKEND_SAMPLE_RATE as f64;
        Self {
            channels: channels.max(1),
            chunk_size,
            accumulator: Vec::with_capacity(channels as usize),
            samples_per_channel_seen: 0,
            next_emit_at: 0.0,
            step,
            output: Vec::with_capacity(chunk_size),
        }
    }

    pub fn push(&mut self, sample: i16) {
        self.accumulator.push(sample);
        if self.accumulator.len() == self.channels as usize {
            let mixed =
                self.accumulator.iter().map(|&s| s as i32).sum::<i32>() / self.channels as i32;
            self.accumulator.clear();
            let frame_index = self.samples_per_channel_seen as f64;
            if frame_index >= self.next_emit_at {
                self.output.push(mixed.clamp(-32768, 32767) as i16);
                self.next_emit_at += self.step;
            }
            self.samples_per_channel_seen = self.samples_per_channel_seen.saturating_add(1);
        }
    }

    pub fn flush_full(&mut self, tx: &mpsc::UnboundedSender<Vec<i16>>) {
        while self.output.len() >= self.chunk_size {
            let chunk: Vec<i16> = self.output.drain(..self.chunk_size).collect();
            let _ = tx.send(chunk);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_at_target_rate() {
        let mut buf = ResampleBuffer::new(BACKEND_SAMPLE_RATE, 1, 4);
        let (tx, mut rx) = mpsc::unbounded_channel();
        for sample in 0..8i16 {
            buf.push(sample * 100);
        }
        buf.flush_full(&tx);
        let chunk = rx.try_recv().expect("first chunk");
        assert_eq!(chunk.len(), 4);
        let chunk2 = rx.try_recv().expect("second chunk");
        assert_eq!(chunk2.len(), 4);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn downsamples_48khz_to_16khz() {
        let mut buf = ResampleBuffer::new(48_000, 1, 4);
        let (tx, mut rx) = mpsc::unbounded_channel();
        for sample in 0..48i16 {
            buf.push(sample);
        }
        buf.flush_full(&tx);
        let chunks: Vec<Vec<i16>> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        let total: usize = chunks.iter().map(Vec::len).sum();
        assert_eq!(total, 16, "48 input samples at 48k -> ~16 at 16k");
    }
}
