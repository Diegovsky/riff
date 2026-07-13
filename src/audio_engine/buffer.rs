//! The audio buffer that flows through the audio engine pipeline.

/// Sample rate delivered by librespot (44100 Hz).
pub const DEFAULT_SAMPLE_RATE: u32 = 44_100;

/// Channel count delivered by librespot (interleaved stereo).
pub const DEFAULT_CHANNELS: u16 = 2;

/// A block of interleaved PCM audio flowing through the processor chain.
///
/// Samples are 64-bit floats, interleaved by channel (for stereo:
/// `[L0, R0, L1, R1, ...]`). Carrying `channels` and `sample_rate` alongside
/// the samples keeps each [`Processor`](crate::audio_engine::Processor)
/// self-describing: processors that need the sample rate (e.g. pitch
/// correction) or channel layout (e.g. mono mixing) can read it explicitly
/// instead of assuming a fixed format.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioBuffer {
    /// Interleaved PCM samples.
    pub samples: Vec<f64>,
    /// Number of interleaved channels (2 = stereo).
    pub channels: u16,
    /// Sample rate in Hz.
    pub sample_rate: u32,
}

impl AudioBuffer {
    /// Create a buffer from interleaved samples with an explicit format.
    pub fn new(samples: Vec<f64>, channels: u16, sample_rate: u32) -> Self {
        Self {
            samples,
            channels,
            sample_rate,
        }
    }

    /// Create a buffer using librespot's default format (44100 Hz stereo).
    pub fn stereo_44100(samples: Vec<f64>) -> Self {
        Self::new(samples, DEFAULT_CHANNELS, DEFAULT_SAMPLE_RATE)
    }
}
