//! PCM to Opus, 20 ms stereo frames at a constant 96 kbit/s.

use opus::{Application, Bitrate, Channels, Encoder, Signal};

use crate::pcm::{BYTES_PER_FRAME, SAMPLE_RATE};
use crate::{Error, Result};

const BITRATE: i32 = 96_000;
const MAX_PACKET: usize = 1275;

fn failed(error: opus::Error) -> Error {
    Error(format!("Opus: {error}"))
}

pub struct OpusEncoder {
    encoder: Encoder,
    samples: Vec<i16>,
    packet: [u8; MAX_PACKET],
}

impl OpusEncoder {
    pub fn new() -> Result<Self> {
        let mut encoder = Encoder::new(SAMPLE_RATE, Channels::Stereo, Application::Audio).map_err(failed)?;
        encoder.set_bitrate(Bitrate::Bits(BITRATE)).map_err(failed)?;
        // Constant size: spikes in packet size are what voice servers drop.
        encoder.set_vbr(false).map_err(failed)?;
        encoder.set_signal(Signal::Music).map_err(failed)?;
        Ok(Self { encoder, samples: Vec::with_capacity(BYTES_PER_FRAME / 2), packet: [0; MAX_PACKET] })
    }

    /// Encode one frame of signed 16-bit little-endian PCM at `volume` percent.
    pub fn encode(&mut self, pcm: &[u8], volume: u8) -> Result<Vec<u8>> {
        if pcm.len() != BYTES_PER_FRAME {
            return Err(Error(format!("A frame is {BYTES_PER_FRAME} bytes, got {}", pcm.len())));
        }
        self.samples.clear();
        self.samples.extend(pcm.chunks_exact(2).map(|b| scale(i16::from_le_bytes([b[0], b[1]]), volume)));
        let size = self.encoder.encode(&self.samples, &mut self.packet).map_err(failed)?;
        Ok(self.packet[..size].to_vec())
    }
}

fn scale(sample: i16, volume: u8) -> i16 {
    if volume == 100 {
        return sample;
    }
    let scaled = (i32::from(sample) * i32::from(volume)) as f32 / 100.0;
    scaled.round().clamp(-32768.0, 32767.0) as i16
}

#[cfg(test)]
mod tests {
    use super::*;
    use opus::Decoder;

    /// One frame of a sine wave, continuing from sample `start`.
    fn sine(start: usize, frequency: f64, amplitude: f64) -> Vec<u8> {
        let mut pcm = Vec::with_capacity(BYTES_PER_FRAME);
        for i in start..start + BYTES_PER_FRAME / 4 {
            let value = (2.0 * std::f64::consts::PI * frequency * i as f64 / f64::from(SAMPLE_RATE)).sin() * amplitude;
            for _ in 0..2 {
                pcm.extend_from_slice(&(value as i16).to_le_bytes());
            }
        }
        pcm
    }

    fn peak(samples: &[i16]) -> i32 {
        samples.iter().map(|s| i32::from(*s).abs()).max().unwrap_or(0)
    }

    #[test]
    fn a_tone_survives_the_round_trip_at_constant_size() {
        let mut encoder = OpusEncoder::new().unwrap();
        let mut decoder = Decoder::new(SAMPLE_RATE, Channels::Stereo).unwrap();
        let mut out = [0i16; 1920];
        let mut sizes = Vec::new();
        for frame in 0..50 {
            let packet = encoder.encode(&sine(frame * 960, 440.0, 12_000.0), 100).unwrap();
            sizes.push(packet.len());
            assert_eq!(decoder.decode(&packet, &mut out, false).unwrap(), 960);
        }
        assert!(sizes.iter().all(|size| *size == 240), "96 kbit/s at 20 ms is 240 bytes: {sizes:?}");
        let level = peak(&out);
        assert!((10_000..14_000).contains(&level), "peak {level}");
    }

    #[test]
    fn volume_scales_the_signal() {
        let mut encoder = OpusEncoder::new().unwrap();
        let mut decoder = Decoder::new(SAMPLE_RATE, Channels::Stereo).unwrap();
        let mut out = [0i16; 1920];
        for frame in 0..50 {
            let packet = encoder.encode(&sine(frame * 960, 440.0, 12_000.0), 25).unwrap();
            decoder.decode(&packet, &mut out, false).unwrap();
        }
        let level = peak(&out);
        assert!((2_400..3_600).contains(&level), "peak {level}");
    }

    #[test]
    fn scaling_rounds_and_stays_in_range() {
        assert_eq!(scale(1000, 50), 500);
        assert_eq!(scale(-32768, 100), -32768);
        assert_eq!(scale(32767, 0), 0);
        assert_eq!(scale(3, 50), 2);
    }

    #[test]
    fn a_wrong_frame_size_is_refused() {
        assert!(OpusEncoder::new().unwrap().encode(&[0; 100], 100).is_err());
    }
}
