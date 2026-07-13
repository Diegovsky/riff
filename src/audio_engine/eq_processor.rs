//! 10-band parametric equalizer using biquad peaking EQ filters.
//!
//! Filter coefficients are derived from the Audio EQ Cookbook:
//! https://webaudio.github.io/Audio-EQ-Cookbook/audio-eq-cookbook.html
//!
//! This module provides [`EqProcessor`], a [`Processor`] that applies per-sample
//! EQ to an [`AudioBuffer`]. Configuration can be changed live while audio is
//! playing through an [`EqController`].

use std::f64::consts::PI;

use super::{AudioBuffer, Processor};

/// Number of EQ bands.
pub const NUM_BANDS: usize = 10;

/// Center frequencies for the 10-band EQ.
pub const BAND_FREQUENCIES: [f64; NUM_BANDS] = [
    31.0, 62.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];

/// Q factor for peaking EQ filters. 1.41 gives ~1 octave bandwidth.
const Q_FACTOR: f64 = 1.41;

/// Biquad filter coefficients.
#[derive(Clone, Copy, Debug)]
struct BiquadCoeffs {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
}

impl BiquadCoeffs {
    /// Compute peaking EQ filter coefficients.
    ///
    /// From the Audio EQ Cookbook:
    ///   H(s) = (s^2 + s*(A/Q) + 1) / (s^2 + s/(A*Q) + 1)
    ///
    /// where A = 10^(dBgain/40) (amplitude), w0 = 2*pi*f0/Fs, alpha = sin(w0)/(2*Q)
    fn peaking_eq(freq: f64, gain_db: f64, q: f64, sample_rate: f64) -> Self {
        if gain_db.abs() < 0.01 {
            // Passthrough — no processing needed
            return Self {
                b0: 1.0,
                b1: 0.0,
                b2: 0.0,
                a1: 0.0,
                a2: 0.0,
            };
        }

        let a = f64::powf(10.0, gain_db / 40.0);
        let w0 = 2.0 * PI * freq / sample_rate;
        let sin_w0 = w0.sin();
        let cos_w0 = w0.cos();
        let alpha = sin_w0 / (2.0 * q);

        let b0 = 1.0 + alpha * a;
        let b1 = -2.0 * cos_w0;
        let b2 = 1.0 - alpha * a;
        let a0 = 1.0 + alpha / a;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha / a;

        // Normalize by a0
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        }
    }
}

/// Per-channel biquad filter state (Direct Form II Transposed).
#[derive(Clone, Copy, Debug, Default)]
struct BiquadState {
    z1: f64,
    z2: f64,
}

/// Threshold below which filter state is flushed to zero to avoid denormals.
///
/// Denormalized floating-point numbers cause severe CPU slowdowns (often 10–100x)
/// on x86. When audio goes silent, the recursive filter state decays toward zero
/// and can enter the denormal range. Flushing to zero prevents this.
const DENORMAL_THRESHOLD: f64 = 1.0e-15;

impl BiquadState {
    /// Process a single sample through the filter (Direct Form II Transposed).
    #[inline(always)]
    fn process(&mut self, input: f64, coeffs: &BiquadCoeffs) -> f64 {
        let output = coeffs.b0 * input + self.z1;
        let z1 = coeffs.b1 * input - coeffs.a1 * output + self.z2;
        let z2 = coeffs.b2 * input - coeffs.a2 * output;

        // Flush denormals to zero.
        self.z1 = if z1.abs() < DENORMAL_THRESHOLD {
            0.0
        } else {
            z1
        };
        self.z2 = if z2.abs() < DENORMAL_THRESHOLD {
            0.0
        } else {
            z2
        };

        output
    }
}

/// Configuration for the equalizer.
#[derive(Clone, Debug)]
pub struct EqConfig {
    /// Gain in dB for each of the 10 bands.
    pub band_gains: [f64; NUM_BANDS],
}

impl Default for EqConfig {
    fn default() -> Self {
        Self {
            band_gains: [0.0; NUM_BANDS],
        }
    }
}

/// The equalizer engine. Holds filter coefficients and per-channel state.
///
/// Only bands with non-zero gain are processed, so a partially-configured EQ
/// (or an enabled-but-flat EQ) incurs minimal cost.
struct Equalizer {
    coeffs: [BiquadCoeffs; NUM_BANDS],
    /// Filter states: [band][channel] — stereo = 2 channels.
    states: [[BiquadState; 2]; NUM_BANDS],
    /// Indices of bands with non-zero gain that actually need processing.
    active_bands: Vec<usize>,
}

impl Equalizer {
    fn new(config: &EqConfig, sample_rate: f64) -> Self {
        let mut coeffs = [BiquadCoeffs {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
        }; NUM_BANDS];

        let mut active_bands = Vec::with_capacity(NUM_BANDS);

        for (i, &freq) in BAND_FREQUENCIES.iter().enumerate() {
            let gain = config.band_gains[i];
            if gain.abs() >= 0.01 {
                coeffs[i] = BiquadCoeffs::peaking_eq(freq, gain, Q_FACTOR, sample_rate);
                active_bands.push(i);
            }
        }

        Self {
            coeffs,
            states: [[BiquadState::default(); 2]; NUM_BANDS],
            active_bands,
        }
    }

    /// Whether the equalizer will modify audio at all.
    #[inline]
    fn is_active(&self) -> bool {
        !self.active_bands.is_empty()
    }

    /// Process interleaved stereo f64 samples in-place.
    #[inline]
    fn process_samples(&mut self, samples: &mut [f64]) {
        // Destructure to give the borrow checker disjoint field borrows.
        let Self {
            coeffs,
            states,
            active_bands,
        } = self;

        // Only iterate over bands that have a non-zero gain. Processing one band
        // across the whole buffer keeps its coefficients and state hot, and is
        // mathematically identical to a per-sample cascade.
        for &band in active_bands.iter() {
            let c = coeffs[band];
            let [state_l, state_r] = &mut states[band];
            for frame in samples.chunks_exact_mut(2) {
                frame[0] = state_l.process(frame[0], &c);
                frame[1] = state_r.process(frame[1], &c);
            }
        }
    }
}

/// A thread-safe handle for updating the equalizer configuration while audio
/// is playing. Cloned into both the player (writer) and the processor (reader).
///
/// A lock-free generation counter lets the audio thread cheaply detect changes
/// on every packet without taking the mutex unless something actually changed.
#[derive(Clone)]
pub struct EqController {
    generation: std::sync::Arc<std::sync::atomic::AtomicU64>,
    shared: std::sync::Arc<std::sync::Mutex<EqShared>>,
}

struct EqShared {
    band_gains: [f64; NUM_BANDS],
}

impl EqController {
    pub fn new(band_gains: [f64; NUM_BANDS]) -> Self {
        Self {
            generation: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            shared: std::sync::Arc::new(std::sync::Mutex::new(EqShared { band_gains })),
        }
    }

    /// Update the equalizer configuration. Cheap; safe to call from any thread.
    pub fn update(&self, band_gains: [f64; NUM_BANDS]) {
        if let Ok(mut shared) = self.shared.lock() {
            shared.band_gains = band_gains;
        }
        // Bump generation last so readers only pick up fully-written values.
        self.generation
            .fetch_add(1, std::sync::atomic::Ordering::Release);
    }

    fn generation(&self) -> u64 {
        self.generation.load(std::sync::atomic::Ordering::Acquire)
    }

    fn read(&self) -> [f64; NUM_BANDS] {
        let shared = self.shared.lock().unwrap();
        shared.band_gains
    }
}

/// A [`Processor`] that applies a 10-band parametric EQ to an [`AudioBuffer`].
///
/// The EQ is always in the chain; it is "active" whenever at least one band has
/// a non-zero gain, and a no-op (passthrough) when every band is flat. It can be
/// reconfigured live via its [`EqController`]; the processor cheaply checks a
/// generation counter on every buffer and rebuilds its filters only when the
/// configuration actually changed.
pub struct EqProcessor {
    controller: EqController,
    equalizer: Equalizer,
    generation: u64,
    sample_rate: f64,
}

impl EqProcessor {
    /// Create a processor bound to the given controller, seeding filters from
    /// the controller's current configuration.
    pub fn new(controller: EqController) -> Self {
        let band_gains = controller.read();
        let sample_rate = super::buffer::DEFAULT_SAMPLE_RATE as f64;
        let equalizer = Equalizer::new(&EqConfig { band_gains }, sample_rate);
        let generation = controller.generation();
        Self {
            controller,
            equalizer,
            generation,
            sample_rate,
        }
    }

    /// Cheaply check the controller for updated settings and rebuild filters if
    /// they changed. Only takes the mutex when the generation counter advances.
    /// Also rebuilds if the sample rate changed since the last buffer.
    #[inline]
    fn refresh(&mut self, sample_rate: f64) {
        let generation = self.controller.generation();
        if generation != self.generation || sample_rate != self.sample_rate {
            let band_gains = self.controller.read();
            self.equalizer = Equalizer::new(&EqConfig { band_gains }, sample_rate);
            self.generation = generation;
            self.sample_rate = sample_rate;
        }
    }
}

impl Processor for EqProcessor {
    fn name(&self) -> &str {
        "equalizer"
    }

    fn is_active(&self) -> bool {
        self.equalizer.is_active()
    }

    fn process(&mut self, buffer: &mut AudioBuffer) {
        self.refresh(buffer.sample_rate as f64);
        if self.equalizer.is_active() {
            self.equalizer.process_samples(&mut buffer.samples);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sample rate used in tests (matches librespot default).
    const TEST_SAMPLE_RATE: f64 = 44_100.0;

    /// Number of frames used for steady-state frequency-response measurements.
    const TEST_FRAMES: usize = 44_100; // 1 second
    /// Number of trailing frames measured once the filter has settled.
    const TAIL_FRAMES: usize = 4_410; // 0.1 second

    /// Build an `EqConfig` with a single band set to `gain_db`.
    fn config_with_band(band: usize, gain_db: f64) -> EqConfig {
        let mut band_gains = [0.0; NUM_BANDS];
        band_gains[band] = gain_db;
        EqConfig { band_gains }
    }

    /// Generate an interleaved-stereo sine wave (same signal in both channels).
    fn generate_sine(freq: f64, n_frames: usize) -> Vec<f64> {
        let mut samples = Vec::with_capacity(n_frames * 2);
        for i in 0..n_frames {
            let s = (2.0 * PI * freq * i as f64 / TEST_SAMPLE_RATE).sin();
            samples.push(s); // left
            samples.push(s); // right
        }
        samples
    }

    /// RMS of the left channel over the final `tail_frames` frames.
    fn rms_tail_left(samples: &[f64], tail_frames: usize) -> f64 {
        let total_frames = samples.len() / 2;
        let start_frame = total_frames - tail_frames;
        let mut sum_sq = 0.0;
        for frame in start_frame..total_frames {
            let v = samples[frame * 2];
            sum_sq += v * v;
        }
        (sum_sq / tail_frames as f64).sqrt()
    }

    /// Measure the steady-state linear gain the EQ applies to a sine at `freq`.
    fn measured_gain(config: &EqConfig, freq: f64) -> f64 {
        let mut eq = Equalizer::new(config, TEST_SAMPLE_RATE);
        let mut samples = generate_sine(freq, TEST_FRAMES);
        let rms_in = rms_tail_left(&samples, TAIL_FRAMES);
        eq.process_samples(&mut samples);
        let rms_out = rms_tail_left(&samples, TAIL_FRAMES);
        rms_out / rms_in
    }

    fn db_to_linear(db: f64) -> f64 {
        f64::powf(10.0, db / 20.0)
    }

    #[test]
    fn peaking_eq_zero_gain_is_passthrough() {
        let c = BiquadCoeffs::peaking_eq(1000.0, 0.0, Q_FACTOR, TEST_SAMPLE_RATE);
        assert_eq!(c.b0, 1.0);
        assert_eq!(c.b1, 0.0);
        assert_eq!(c.b2, 0.0);
        assert_eq!(c.a1, 0.0);
        assert_eq!(c.a2, 0.0);
    }

    #[test]
    fn flat_equalizer_is_inactive() {
        let eq = Equalizer::new(&EqConfig::default(), TEST_SAMPLE_RATE);
        assert!(!eq.is_active());
        assert!(eq.active_bands.is_empty());
    }

    #[test]
    fn only_nonzero_bands_are_active() {
        let mut band_gains = [0.0; NUM_BANDS];
        band_gains[3] = 4.0;
        band_gains[7] = -2.0;
        let eq = Equalizer::new(&EqConfig { band_gains }, TEST_SAMPLE_RATE);
        assert!(eq.is_active());
        assert_eq!(eq.active_bands, vec![3, 7]);
    }

    #[test]
    fn tiny_gains_are_treated_as_flat() {
        // Below the 0.01 dB threshold the band should be ignored.
        let eq = Equalizer::new(&config_with_band(5, 0.005), TEST_SAMPLE_RATE);
        assert!(!eq.is_active());
    }

    #[test]
    fn flat_equalizer_does_not_modify_samples() {
        let mut eq = Equalizer::new(&EqConfig::default(), TEST_SAMPLE_RATE);
        let original = generate_sine(1000.0, 512);
        let mut samples = original.clone();
        eq.process_samples(&mut samples);
        assert_eq!(samples, original);
    }

    #[test]
    fn boost_increases_amplitude_at_center_frequency() {
        // Band 5 is 1 kHz. A +6 dB peaking boost should raise a 1 kHz tone
        // by ~6 dB (linear factor ~1.995) at the center frequency.
        let config = config_with_band(5, 6.0);
        let gain = measured_gain(&config, BAND_FREQUENCIES[5]);
        let expected = db_to_linear(6.0);
        assert!(
            (gain - expected).abs() / expected < 0.03,
            "measured gain {} not within 3% of expected {}",
            gain,
            expected
        );
    }

    #[test]
    fn cut_decreases_amplitude_at_center_frequency() {
        // A -6 dB cut should attenuate a 1 kHz tone by ~6 dB (factor ~0.501).
        let config = config_with_band(5, -6.0);
        let gain = measured_gain(&config, BAND_FREQUENCIES[5]);
        let expected = db_to_linear(-6.0);
        assert!(
            (gain - expected).abs() / expected < 0.03,
            "measured gain {} not within 3% of expected {}",
            gain,
            expected
        );
    }

    #[test]
    fn distant_frequency_is_mostly_unaffected() {
        // Boosting the 31 Hz band should barely touch a 1 kHz tone (5 octaves away).
        let config = config_with_band(0, 6.0);
        let gain = measured_gain(&config, 1000.0);
        assert!(
            (gain - 1.0).abs() < 0.03,
            "distant-frequency gain {} should be ~1.0",
            gain
        );
    }

    #[test]
    fn both_channels_are_processed_identically() {
        // With identical input in both channels, the output must match per channel.
        let config = config_with_band(5, 6.0);
        let mut eq = Equalizer::new(&config, TEST_SAMPLE_RATE);
        let mut samples = generate_sine(1000.0, 4096);
        eq.process_samples(&mut samples);
        for frame in samples.chunks_exact(2) {
            assert!(
                (frame[0] - frame[1]).abs() < 1e-12,
                "left {} and right {} channel outputs diverged",
                frame[0],
                frame[1]
            );
        }
    }

    #[test]
    fn filter_state_flushes_denormals_to_zero() {
        // After a signal burst followed by a long run of silence, the recursive
        // state must be flushed exactly to zero (denormal protection), not left
        // as a lingering denormal value.
        let mut eq = Equalizer::new(&config_with_band(5, 6.0), TEST_SAMPLE_RATE);

        let mut burst = generate_sine(1000.0, 1_000);
        eq.process_samples(&mut burst);

        let mut silence = vec![0.0f64; 20_000 * 2];
        eq.process_samples(&mut silence);

        for state in &eq.states[5] {
            assert_eq!(state.z1, 0.0, "z1 should be flushed to exactly zero");
            assert_eq!(state.z2, 0.0, "z2 should be flushed to exactly zero");
        }

        // The tail of the silence buffer must also be exactly zero.
        assert!(silence.iter().rev().take(100).all(|&s| s == 0.0));
    }

    #[test]
    fn band_outer_processing_matches_per_sample_cascade() {
        // The optimized band-outer loop must produce the same result as a
        // straightforward per-sample cascade over all bands.
        let mut band_gains = [0.0; NUM_BANDS];
        band_gains[2] = 5.0;
        band_gains[5] = -4.0;
        band_gains[8] = 3.0;
        let config = EqConfig { band_gains };

        let input = generate_sine(440.0, 2048);

        // Reference: per-frame, per-band cascade using fresh coefficients/state.
        let mut ref_eq = Equalizer::new(&config, TEST_SAMPLE_RATE);
        let mut reference = input.clone();
        {
            let Equalizer {
                coeffs,
                states,
                active_bands,
            } = &mut ref_eq;
            for frame in reference.chunks_exact_mut(2) {
                for &band in active_bands.iter() {
                    frame[0] = states[band][0].process(frame[0], &coeffs[band]);
                    frame[1] = states[band][1].process(frame[1], &coeffs[band]);
                }
            }
        }

        // Actual implementation.
        let mut eq = Equalizer::new(&config, TEST_SAMPLE_RATE);
        let mut actual = input.clone();
        eq.process_samples(&mut actual);

        for (a, r) in actual.iter().zip(reference.iter()) {
            assert!((a - r).abs() < 1e-9, "outputs diverged: {} vs {}", a, r);
        }
    }

    #[test]
    fn coefficients_are_finite_across_all_bands() {
        // Guard against NaN/inf coefficients at any band frequency.
        for &freq in BAND_FREQUENCIES.iter() {
            for &gain in &[-12.0, -6.0, 6.0, 12.0] {
                let c = BiquadCoeffs::peaking_eq(freq, gain, Q_FACTOR, TEST_SAMPLE_RATE);
                for v in [c.b0, c.b1, c.b2, c.a1, c.a2] {
                    assert!(
                        v.is_finite(),
                        "non-finite coeff at {} Hz, {} dB",
                        freq,
                        gain
                    );
                }
            }
        }
    }

    // --- Processor-level tests -------------------------------------------------

    #[test]
    fn processor_inactive_when_flat() {
        let controller = EqController::new([0.0; NUM_BANDS]);
        let processor = EqProcessor::new(controller);
        assert!(!processor.is_active());
    }

    #[test]
    fn processor_active_when_configured() {
        let mut band_gains = [0.0; NUM_BANDS];
        band_gains[5] = 6.0;
        let controller = EqController::new(band_gains);
        let processor = EqProcessor::new(controller);
        assert!(processor.is_active());
    }

    #[test]
    fn processor_matches_direct_equalizer_output() {
        let mut band_gains = [0.0; NUM_BANDS];
        band_gains[5] = 6.0;
        let controller = EqController::new(band_gains);
        let mut processor = EqProcessor::new(controller);

        let input = generate_sine(1000.0, 2048);
        let mut buffer = AudioBuffer::stereo_44100(input.clone());
        processor.process(&mut buffer);

        let mut reference = Equalizer::new(&EqConfig { band_gains }, TEST_SAMPLE_RATE);
        let mut expected = input;
        reference.process_samples(&mut expected);

        assert_eq!(buffer.samples, expected);
    }

    #[test]
    fn processor_picks_up_live_updates() {
        let controller = EqController::new([0.0; NUM_BANDS]);
        let mut processor = EqProcessor::new(controller.clone());
        assert!(!processor.is_active());

        // Live-update to a boosted band.
        let mut band_gains = [0.0; NUM_BANDS];
        band_gains[5] = 6.0;
        controller.update(band_gains);

        // A process() call refreshes and picks up the new config.
        let mut buffer = AudioBuffer::stereo_44100(generate_sine(1000.0, 512));
        processor.process(&mut buffer);
        assert!(processor.is_active());
    }
}
