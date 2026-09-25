//! Audio helpers with no I/O: mixing, resampling, levels, silence detection,
//! noise suppression and WAV encoding. Speech models want 16 kHz mono f32.

pub const SPEECH_RATE: u32 = 16_000;
const DENOISE_RATE: u32 = 48_000;

/// Averages interleaved channels to mono.
pub fn to_mono(interleaved: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    interleaved
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

/// Linear-interpolation resampler that keeps its position across chunks, so
/// a live stream resamples without clicks at chunk boundaries. Downsampling
/// first averages each output sample's input span, which is enough of a
/// low-pass filter for speech.
pub struct Resampler {
    ratio: f64,
    pos: f64,
    last: f32,
}

impl Resampler {
    pub fn new(from: u32, to: u32) -> Self {
        Self {
            ratio: from as f64 / to as f64,
            pos: 0.0,
            last: 0.0,
        }
    }

    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        if (self.ratio - 1.0).abs() < f64::EPSILON {
            return input.to_vec();
        }
        let mut out = Vec::with_capacity((input.len() as f64 / self.ratio) as usize + 1);
        // Sample i of the virtual stream: index -1 is the previous chunk's last sample.
        let at = |i: isize| -> f32 {
            if i < 0 {
                self.last
            } else {
                input[i as usize]
            }
        };
        while self.pos < input.len() as f64 - 1.0
            || (self.pos < input.len() as f64 && input.len() == 1)
        {
            let value = if self.ratio > 1.0 {
                let start = self.pos.floor() as isize;
                let end = ((self.pos + self.ratio).floor() as isize)
                    .min(input.len() as isize - 1)
                    .max(start);
                let span = (start..=end).map(at).sum::<f32>();
                span / (end - start + 1) as f32
            } else {
                let i = self.pos.floor() as isize;
                let frac = (self.pos - i as f64) as f32;
                let a = at(i);
                let b = at(i + 1);
                a + (b - a) * frac
            };
            out.push(value);
            self.pos += self.ratio;
        }
        self.pos -= input.len() as f64;
        if let Some(l) = input.last() {
            self.last = *l;
        }
        out
    }
}

/// Root mean square of a block.
pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

/// A 0..1 meter value: -60 dBFS and below is 0, 0 dBFS is 1.
pub fn meter_level(samples: &[f32]) -> f32 {
    let db = 20.0 * rms(samples).max(1e-9).log10();
    ((db + 60.0) / 60.0).clamp(0.0, 1.0)
}

/// Decides when the user has stopped talking.
///
/// The noise floor is learned from quiet blocks, so a fan or a noisy room
/// doesn't count as speech. Speech has to be at least 3x (about 10 dB) above
/// the floor and above an absolute minimum. After speech has been heard,
/// `silence` of quiet ends the recording.
pub struct SilenceDetector {
    floor: f32,
    heard_speech: bool,
    quiet_for: f32,
    silence: f32,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Vad {
    Speech,
    Quiet,
    /// Enough silence after speech: stop listening.
    Finished,
}

const MIN_SPEECH_RMS: f32 = 0.01;

impl SilenceDetector {
    /// `silence_seconds` of 0 never finishes on its own.
    pub fn new(silence_seconds: f64) -> Self {
        Self {
            floor: 0.003,
            heard_speech: false,
            quiet_for: 0.0,
            silence: silence_seconds as f32,
        }
    }

    pub fn heard_speech(&self) -> bool {
        self.heard_speech
    }

    /// Feeds one block of 16 kHz audio.
    pub fn push(&mut self, block: &[f32]) -> Vad {
        let level = rms(block);
        let seconds = block.len() as f32 / SPEECH_RATE as f32;
        let speech = level > MIN_SPEECH_RMS && level > self.floor * 3.0;
        if speech {
            self.heard_speech = true;
            self.quiet_for = 0.0;
            return Vad::Speech;
        }
        // Track the floor slowly upward and quickly downward.
        self.floor = if level < self.floor {
            level.max(1e-5)
        } else {
            self.floor * 0.95 + level * 0.05
        };
        if self.heard_speech {
            self.quiet_for += seconds;
            if self.silence > 0.0 && self.quiet_for >= self.silence {
                return Vad::Finished;
            }
        }
        Vad::Quiet
    }
}

/// RNNoise noise suppression. Works on 48 kHz audio in 480-sample frames;
/// leftover samples wait for the next call.
pub struct Denoiser {
    state: Box<nnnoiseless::DenoiseState<'static>>,
    pending: Vec<f32>,
    frame_out: [f32; nnnoiseless::FRAME_SIZE],
}

impl Default for Denoiser {
    fn default() -> Self {
        Self {
            state: nnnoiseless::DenoiseState::new(),
            pending: Vec::new(),
            frame_out: [0.0; nnnoiseless::FRAME_SIZE],
        }
    }
}

impl Denoiser {
    /// Input and output are 48 kHz floats in -1..1.
    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        self.pending.extend(input.iter().map(|s| s * 32767.0));
        let mut out = Vec::with_capacity(self.pending.len());
        let frames = self.pending.len() / nnnoiseless::FRAME_SIZE;
        for f in 0..frames {
            let frame =
                &self.pending[f * nnnoiseless::FRAME_SIZE..(f + 1) * nnnoiseless::FRAME_SIZE];
            self.state.process_frame(&mut self.frame_out, frame);
            out.extend(self.frame_out.iter().map(|s| s / 32767.0));
        }
        self.pending.drain(..frames * nnnoiseless::FRAME_SIZE);
        out
    }
}

/// Everything between the microphone callback and the speech model:
/// device format in, 16 kHz mono out, optionally denoised.
pub struct Pipeline {
    channels: usize,
    to_48k: Option<Resampler>,
    denoiser: Option<Denoiser>,
    to_16k: Resampler,
}

impl Pipeline {
    pub fn new(device_rate: u32, channels: usize, denoise: bool) -> Self {
        if denoise {
            Self {
                channels,
                to_48k: Some(Resampler::new(device_rate, DENOISE_RATE)),
                denoiser: Some(Denoiser::default()),
                to_16k: Resampler::new(DENOISE_RATE, SPEECH_RATE),
            }
        } else {
            Self {
                channels,
                to_48k: None,
                denoiser: None,
                to_16k: Resampler::new(device_rate, SPEECH_RATE),
            }
        }
    }

    pub fn process(&mut self, interleaved: &[f32]) -> Vec<f32> {
        let mono = to_mono(interleaved, self.channels);
        match (&mut self.to_48k, &mut self.denoiser) {
            (Some(up), Some(dn)) => {
                let clean = dn.process(&up.process(&mono));
                self.to_16k.process(&clean)
            }
            _ => self.to_16k.process(&mono),
        }
    }
}

/// 16-bit PCM WAV, for cloud speech-to-text uploads.
pub fn wav(samples: &[f32], rate: u32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes()); // byte rate
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    out
}

/// 16-bit little-endian PCM bytes to floats.
pub fn pcm16_to_f32(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, rate: u32, seconds: f32, amp: f32) -> Vec<f32> {
        (0..(rate as f32 * seconds) as usize)
            .map(|i| amp * (2.0 * std::f32::consts::PI * freq * i as f32 / rate as f32).sin())
            .collect()
    }

    #[test]
    fn mono_mix_averages_channels() {
        assert_eq!(to_mono(&[1.0, 0.0, 0.5, 0.5], 2), vec![0.5, 0.5]);
    }

    #[test]
    fn resampling_keeps_duration_across_chunks() {
        let input = sine(440.0, 48_000, 1.0, 0.5);
        let mut r = Resampler::new(48_000, 16_000);
        let out: Vec<f32> = input.chunks(1234).flat_map(|c| r.process(c)).collect();
        assert!((out.len() as i64 - 16_000).abs() <= 2, "{}", out.len());
        let mut up = Resampler::new(44_100, 48_000);
        let out: Vec<f32> = sine(440.0, 44_100, 1.0, 0.5)
            .chunks(777)
            .flat_map(|c| up.process(c))
            .collect();
        assert!((out.len() as i64 - 48_000).abs() <= 2, "{}", out.len());
    }

    #[test]
    fn resampling_preserves_a_speech_band_tone() {
        let out = Resampler::new(48_000, 16_000).process(&sine(300.0, 48_000, 0.5, 0.5));
        // A 0.5-amplitude sine has an RMS of about 0.354.
        assert!((rms(&out) - 0.354).abs() < 0.02, "{}", rms(&out));
    }

    #[test]
    fn meter_maps_decibels_to_zero_one() {
        assert_eq!(meter_level(&[0.0; 100]), 0.0);
        assert!((meter_level(&[1.0; 100]) - 1.0).abs() < 1e-6);
        assert!((meter_level(&[0.1; 100]) - 2.0 / 3.0).abs() < 0.01); // -20 dBFS
    }

    #[test]
    fn silence_after_speech_finishes_and_noise_alone_never_starts() {
        let mut d = SilenceDetector::new(1.0);
        let block = |amp: f32| sine(200.0, SPEECH_RATE, 0.1, amp);
        // Steady background noise: never counts as speech.
        for _ in 0..30 {
            assert_ne!(d.push(&block(0.004)), Vad::Speech);
        }
        assert!(!d.heard_speech());
        assert_eq!(d.push(&block(0.3)), Vad::Speech);
        let mut quiet_blocks = 0;
        loop {
            quiet_blocks += 1;
            if d.push(&block(0.004)) == Vad::Finished {
                break;
            }
            assert!(quiet_blocks < 50);
        }
        assert_eq!(quiet_blocks, 10, "1 second of 100 ms blocks");
    }

    #[test]
    fn zero_silence_never_auto_stops() {
        let mut d = SilenceDetector::new(0.0);
        d.push(&sine(200.0, SPEECH_RATE, 0.1, 0.3));
        for _ in 0..200 {
            assert_ne!(d.push(&[0.0; 1600]), Vad::Finished);
        }
    }

    #[test]
    fn denoiser_keeps_length_in_whole_frames_and_quiets_noise() {
        let mut dn = Denoiser::default();
        // Deterministic pseudo-random noise.
        let mut seed = 1u32;
        let noise: Vec<f32> = (0..48_000 * 3)
            .map(|_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                ((seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.1
            })
            .collect();
        let out: Vec<f32> = noise.chunks(1000).flat_map(|c| dn.process(c)).collect();
        assert_eq!(
            out.len(),
            48_000 * 3 / nnnoiseless::FRAME_SIZE * nnnoiseless::FRAME_SIZE
        );
        // After it has adapted (the last second), steady noise comes out quieter.
        let settled = rms(&out[48_000 * 2..]);
        assert!(settled < rms(&noise) * 0.8, "{settled} vs {}", rms(&noise));
    }

    #[test]
    fn pipeline_outputs_16k_mono() {
        let stereo: Vec<f32> = sine(300.0, 44_100, 1.0, 0.3)
            .iter()
            .flat_map(|s| [*s, *s])
            .collect();
        for denoise in [false, true] {
            let mut p = Pipeline::new(44_100, 2, denoise);
            let out: Vec<f32> = stereo.chunks(882).flat_map(|c| p.process(c)).collect();
            assert!(
                (out.len() as i64 - 16_000).abs() < 400,
                "denoise={denoise}: {}",
                out.len()
            );
        }
    }

    #[test]
    fn wav_header_and_round_trip() {
        let w = wav(&[0.0, 0.5, -0.5, 1.0], 16_000);
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(w[24..28].try_into().unwrap()), 16_000);
        assert_eq!(w.len(), 44 + 8);
        let back = pcm16_to_f32(&w[44..]);
        assert!((back[1] - 0.5).abs() < 1e-3 && (back[3] - 1.0).abs() < 1e-3);
    }
}
