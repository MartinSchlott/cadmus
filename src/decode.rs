use std::io::Cursor;

use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

pub(crate) const TARGET_SAMPLE_RATE: u32 = 16_000;

#[derive(Debug)]
pub(crate) enum AudioError {
    Decode(String),
    Resample(String),
}

impl std::fmt::Display for AudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AudioError::Decode(m) => write!(f, "decode failed: {m}"),
            AudioError::Resample(m) => write!(f, "resample failed: {m}"),
        }
    }
}
impl std::error::Error for AudioError {}

/// Decode arbitrary audio bytes to 16 kHz mono `f32` in `[-1, 1]`.
/// Format is detected from the byte stream — caller does not declare it.
pub(crate) fn decode_to_pcm16k(bytes: &[u8]) -> Result<Vec<f32>, AudioError> {
    let (interleaved, sample_rate, channels) = decode_interleaved(bytes)?;
    let mono = downmix_to_mono(&interleaved, channels);
    if sample_rate == TARGET_SAMPLE_RATE {
        return Ok(mono);
    }
    resample_to_target(&mono, sample_rate)
}

fn decode_interleaved(bytes: &[u8]) -> Result<(Vec<f32>, u32, u16), AudioError> {
    let mss = MediaSourceStream::new(Box::new(Cursor::new(bytes.to_vec())), Default::default());

    let probed = symphonia::default::get_probe()
        .format(
            &Hint::new(),
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| AudioError::Decode(format!("probe: {e}")))?;

    let mut format = probed.format;

    let track = format
        .default_track()
        .or_else(|| {
            format
                .tracks()
                .iter()
                .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        })
        .ok_or_else(|| AudioError::Decode("no decodable track in stream".into()))?;

    let track_id = track.id;
    let codec_params = track.codec_params.clone();

    let sample_rate = codec_params
        .sample_rate
        .ok_or_else(|| AudioError::Decode("track lacks sample rate".into()))?;
    let channels = codec_params
        .channels
        .ok_or_else(|| AudioError::Decode("track lacks channel layout".into()))?
        .count() as u16;

    let mut decoder = symphonia::default::get_codecs()
        .make(&codec_params, &DecoderOptions::default())
        .map_err(|e| AudioError::Decode(format!("make decoder: {e}")))?;

    let mut interleaved: Vec<f32> = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymphoniaError::IoError(ref e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(e) => return Err(AudioError::Decode(format!("next_packet: {e}"))),
        };

        if packet.track_id() != track_id {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(decoded) => {
                let spec = *decoded.spec();
                let duration = decoded.capacity() as u64;
                let mut buf = SampleBuffer::<f32>::new(duration, spec);
                buf.copy_interleaved_ref(decoded);
                interleaved.extend_from_slice(buf.samples());
            }
            Err(e) => return Err(AudioError::Decode(format!("decode: {e}"))),
        }
    }

    Ok((interleaved, sample_rate, channels))
}

fn downmix_to_mono(interleaved: &[f32], channels: u16) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    let n = channels as usize;
    let inv = 1.0 / n as f32;
    interleaved
        .chunks_exact(n)
        .map(|frame| frame.iter().sum::<f32>() * inv)
        .collect()
}

fn resample_to_target(mono: &[f32], src_rate: u32) -> Result<Vec<f32>, AudioError> {
    const CHUNK: usize = 1024;

    let params = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 256,
        window: WindowFunction::BlackmanHarris2,
    };

    let mut resampler = SincFixedIn::<f32>::new(
        TARGET_SAMPLE_RATE as f64 / src_rate as f64,
        1.0,
        params,
        CHUNK,
        1,
    )
    .map_err(|e| AudioError::Resample(format!("construct: {e}")))?;

    let mut out: Vec<f32> = Vec::with_capacity(
        ((mono.len() as f64) * TARGET_SAMPLE_RATE as f64 / src_rate as f64) as usize + CHUNK,
    );

    let full_chunks = mono.len() / CHUNK;
    for i in 0..full_chunks {
        let chunk = &mono[i * CHUNK..(i + 1) * CHUNK];
        let processed = resampler
            .process(&[chunk], None)
            .map_err(|e| AudioError::Resample(format!("process: {e}")))?;
        out.extend_from_slice(&processed[0]);
    }

    let tail = &mono[full_chunks * CHUNK..];
    let processed = if tail.is_empty() {
        resampler
            .process_partial::<&[f32]>(None, None)
            .map_err(|e| AudioError::Resample(format!("process_partial: {e}")))?
    } else {
        resampler
            .process_partial(Some(&[tail]), None)
            .map_err(|e| AudioError::Resample(format!("process_partial: {e}")))?
    };
    out.extend_from_slice(&processed[0]);

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MP3: &[u8] = include_bytes!("../fixtures/eins-zwei-drei.mp3");
    const WAV: &[u8] = include_bytes!("../fixtures/eins-zwei-drei.wav");
    const FLAC: &[u8] = include_bytes!("../fixtures/eins-zwei-drei.flac");

    fn assert_valid_pcm16k(samples: &[f32]) {
        assert!(!samples.is_empty(), "decoded buffer is empty");
        assert!(
            samples.len() >= 16_000 * 2,
            "≥ 2 seconds expected, got {} samples",
            samples.len()
        );
        assert!(
            samples.len() <= 16_000 * 30,
            "≤ 30 seconds expected, got {} samples",
            samples.len()
        );
        assert!(
            samples.iter().all(|s| (-1.0..=1.0).contains(s)),
            "sample outside [-1, 1]"
        );
    }

    #[test]
    fn decode_mp3_to_pcm16k() {
        assert_valid_pcm16k(&decode_to_pcm16k(MP3).unwrap());
    }
    #[test]
    fn decode_wav_to_pcm16k() {
        assert_valid_pcm16k(&decode_to_pcm16k(WAV).unwrap());
    }
    #[test]
    fn decode_flac_to_pcm16k() {
        assert_valid_pcm16k(&decode_to_pcm16k(FLAC).unwrap());
    }

    #[test]
    fn fixtures_have_consistent_length() {
        let m = decode_to_pcm16k(MP3).unwrap().len();
        let w = decode_to_pcm16k(WAV).unwrap().len();
        let f = decode_to_pcm16k(FLAC).unwrap().len();
        let lo = *[m, w, f].iter().min().unwrap();
        let hi = *[m, w, f].iter().max().unwrap();
        assert!(
            hi - lo < 2048,
            "fixtures diverge by {} samples (mp3={m} wav={w} flac={f})",
            hi - lo
        );
    }

    #[test]
    fn stereo_downmix_cancels() {
        let frames = 1000usize;
        let mut buf = Vec::with_capacity(frames * 2);
        for _ in 0..frames {
            buf.push(0.5);
            buf.push(-0.5);
        }
        let mono = downmix_to_mono(&buf, 2);
        assert_eq!(mono.len(), frames);
        assert!(mono.iter().all(|s| s.abs() < 1e-6));
    }

    #[test]
    fn resample_48k_sine_to_16k() {
        let src_rate = 48_000u32;
        let n = src_rate as usize;
        let mut sine = Vec::with_capacity(n);
        for i in 0..n {
            sine.push(
                (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / src_rate as f32).sin(),
            );
        }
        let out = resample_to_target(&sine, src_rate).unwrap();
        assert!(
            (15_900..=16_100).contains(&out.len()),
            "unexpected output length {}",
            out.len()
        );
        let peak = out.iter().fold(0f32, |a, &b| a.max(b.abs()));
        assert!(
            (0.85..=1.0).contains(&peak),
            "peak amplitude {peak} outside [0.85, 1.0]"
        );
    }

    #[test]
    fn corrupt_audio_returns_decode_error() {
        // FLAC header + metadata blocks stay intact so probe succeeds; every
        // byte from offset 4096 onward is overwritten with 0xFF so frame syncs
        // and CRCs fail. With the contract from definition.md §3 ("truly
        // corrupt audio raises Decode") this must surface AudioError::Decode
        // instead of silently yielding partial PCM.
        let mut corrupt = FLAC.to_vec();
        for b in &mut corrupt[4096..] {
            *b = 0xFF;
        }
        let result = decode_to_pcm16k(&corrupt);
        assert!(
            matches!(result, Err(AudioError::Decode(_))),
            "expected AudioError::Decode, got {result:?}"
        );
    }

    #[test]
    fn mono_16k_passthrough() {
        let n = 16_000usize;
        let mut samples = Vec::with_capacity(n);
        for i in 0..n {
            samples.push((i as f32 * 0.0001).sin() * 0.5);
        }

        let data_len: u32 = (n as u32) * 2;
        let mut wav: Vec<u8> = Vec::with_capacity(44 + data_len as usize);
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36u32 + data_len).to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
        wav.extend_from_slice(&1u16.to_le_bytes()); // channels
        wav.extend_from_slice(&16_000u32.to_le_bytes()); // sample rate
        wav.extend_from_slice(&32_000u32.to_le_bytes()); // byte rate
        wav.extend_from_slice(&2u16.to_le_bytes()); // block align
        wav.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_len.to_le_bytes());
        for &s in &samples {
            let q: i16 = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            wav.extend_from_slice(&q.to_le_bytes());
        }

        let decoded = decode_to_pcm16k(&wav).unwrap();
        assert_eq!(decoded.len(), n);
        let tol = 2.0 / 32768.0;
        for (i, (&orig, &round)) in samples.iter().zip(decoded.iter()).enumerate() {
            assert!(
                (orig - round).abs() <= tol,
                "sample {i}: {orig} vs {round} (diff {})",
                (orig - round).abs()
            );
        }
    }
}
