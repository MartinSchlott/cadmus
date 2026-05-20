use crate::decode::AudioError;

const OPUS_INTERNAL_RATE: i32 = 48_000;
// 120 ms at 48 kHz — the maximum Opus frame size per channel.
const MAX_FRAME_SAMPLES_PER_CHANNEL: i32 = 5_760;

pub(crate) struct OpusHead {
    pub pre_skip: u16,
    pub channels: u8,
}

pub(crate) fn parse_opus_head(bytes: &[u8]) -> Result<OpusHead, AudioError> {
    if bytes.len() < 19 {
        return Err(AudioError::Decode(format!(
            "OpusHead too short: {} bytes",
            bytes.len()
        )));
    }
    if &bytes[0..8] != b"OpusHead" {
        return Err(AudioError::Decode("missing OpusHead".into()));
    }
    let version = bytes[8];
    if version != 1 {
        return Err(AudioError::Decode(format!(
            "unsupported OpusHead version: {version}"
        )));
    }
    let channels = bytes[9];
    if channels != 1 && channels != 2 {
        return Err(AudioError::Decode(format!(
            "unsupported channel count: {channels}"
        )));
    }
    let pre_skip = u16::from_le_bytes([bytes[10], bytes[11]]);
    // bytes 12..16: input sample rate (u32 LE) — ignored, decoder always at 48 kHz.
    // bytes 16..18: output gain (i16 LE, Q7.8 dB) — ignored, always 0 in practice.
    let mapping_family = bytes[18];
    if mapping_family != 0 {
        return Err(AudioError::Decode(format!(
            "unsupported channel mapping family: {mapping_family}"
        )));
    }
    Ok(OpusHead { pre_skip, channels })
}

pub(crate) fn apply_pre_skip(buf: &mut Vec<f32>, pre_skip: u16, channels: u8) {
    let drop = (pre_skip as usize).saturating_mul(channels as usize);
    let drop = drop.min(buf.len());
    buf.drain(..drop);
}

pub(crate) struct OpusDecoder {
    ptr: *mut unsafe_libopus::OpusDecoder,
    channels: u8,
}

impl OpusDecoder {
    pub(crate) fn new(channels: u8) -> Result<Self, AudioError> {
        if channels != 1 && channels != 2 {
            return Err(AudioError::Decode(format!(
                "unsupported channel count: {channels}"
            )));
        }
        let mut err: i32 = 0;
        let ptr = unsafe {
            unsafe_libopus::opus_decoder_create(OPUS_INTERNAL_RATE, channels as i32, &mut err)
        };
        if ptr.is_null() || err != unsafe_libopus::OPUS_OK {
            let msg = unsafe_libopus::opus_strerror(err);
            return Err(AudioError::Decode(format!("opus_decoder_create: {msg}")));
        }
        Ok(Self { ptr, channels })
    }

    pub(crate) fn decode_packet(&mut self, packet: &[u8]) -> Result<Vec<f32>, AudioError> {
        let mut buf =
            vec![0f32; MAX_FRAME_SAMPLES_PER_CHANNEL as usize * self.channels as usize];
        let n = unsafe {
            unsafe_libopus::opus_decode_float(
                self.ptr,
                packet.as_ptr(),
                packet.len() as i32,
                buf.as_mut_ptr(),
                MAX_FRAME_SAMPLES_PER_CHANNEL,
                0,
            )
        };
        if n < 0 {
            let msg = unsafe_libopus::opus_strerror(n);
            return Err(AudioError::Decode(format!("opus_decode_float: {msg}")));
        }
        buf.truncate(n as usize * self.channels as usize);
        Ok(buf)
    }
}

impl Drop for OpusDecoder {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe { unsafe_libopus::opus_decoder_destroy(self.ptr) };
            self.ptr = std::ptr::null_mut();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_opus_head_extracts_pre_skip_and_channels() {
        let mut head = vec![0u8; 19];
        head[0..8].copy_from_slice(b"OpusHead");
        head[8] = 1;
        head[9] = 2;
        head[10..12].copy_from_slice(&312u16.to_le_bytes());
        head[12..16].copy_from_slice(&48_000u32.to_le_bytes());
        head[16..18].copy_from_slice(&0i16.to_le_bytes());
        head[18] = 0;
        let parsed = parse_opus_head(&head).expect("valid OpusHead");
        assert_eq!(parsed.pre_skip, 312);
        assert_eq!(parsed.channels, 2);
    }

    #[test]
    fn parse_opus_head_rejects_nonzero_mapping_family() {
        let mut head = vec![0u8; 19];
        head[0..8].copy_from_slice(b"OpusHead");
        head[8] = 1;
        head[9] = 1;
        head[18] = 1;
        assert!(matches!(parse_opus_head(&head), Err(AudioError::Decode(_))));
    }

    #[test]
    fn pre_skip_drops_correct_prefix_for_stereo() {
        let mut buf: Vec<f32> = (0..2000).map(|i| i as f32).collect();
        apply_pre_skip(&mut buf, 100, 2);
        assert_eq!(buf.len(), 1800);
        assert_eq!(buf[0], 200.0);
    }
}
