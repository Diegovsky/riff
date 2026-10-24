//! Real-time pitch-shift processor (fine cent adjustments).
//!
//! Shifts the pitch of stereo audio up or down by a number of *cents*
//! (1 cent = 1/100 of a semitone) while preserving tempo and producing the same
//! number of output samples as input (so it drops straight into the in-place
//! [`Processor`] pipeline).
//!
//! # Technique
//!
//! This uses the classic time-domain "dual-tap delay line" pitch shifter (the
//! same idea behind hardware harmonizers). Each channel keeps a ring buffer of
//! recent input. Two read taps move through that buffer at the target pitch
//! ratio; because reading faster/slower than writing raises/lowers pitch (a
//! Doppler effect), the taps drift relative to the write head. To hide the
//! discontinuity when a tap wraps around its window, the two taps are offset by
//! half a window and crossfaded with triangular windows that always sum to 1.
//!
//! Quality is well suited to the small (sub-semitone) adjustments this control
//! exposes, and the ratio can be changed live with no state reset.

use super::{AudioBuffer, Processor};

/// Cent magnitudes below this are treated as no shift (bypass).
const PITCH_EPSILON_CENTS: f64 = 0.01;

/// Length of each channel's ring buffer (power of two). Must exceed `WINDOW`.
const BUF_SIZE: usize = 4096;

/// Crossfade window length, in samples. Larger windows reduce warble at the
/// cost of latency; ~46 ms at 44.1 kHz is a good balance for fine shifts.
const WINDOW: f64 = 2048.0;

/// Convert a pitch offset in cents to a linear frequency ratio.
///
/// `ratio = 2^(cents / 1200)`, so +1200 cents doubles the frequency (one octave)
/// and 0 cents is unity (no change).
#[inline]
fn cents_to_ratio(cents: f64) -> f64 {
    f64::powf(2.0, cents / 1200.0)
}

/// Triangular crossfade window over a normalized phase in `[0, 1)`: zero at the
/// edges, one at the center. Two of these offset by half a period sum to 1.
#[inline]
fn triangular_window(phase: f64) -> f64 {
    1.0 - (2.0 * phase - 1.0).abs()
}

/// Per-channel delay line plus the shared read-phase state.
struct PitchShifter {
    /// Ring buffers, one per channel (stereo).
    buffers: [[f64; BUF_SIZE]; 2],
    /// Shared write head (advances once per frame).
    write_index: usize,
    /// Normalized read phase in `[0, 1)`, shared across channels.
    phase: f64,
}

impl PitchShifter {
    fn new() -> Self {
        Self {
            buffers: [[0.0; BUF_SIZE]; 2],
            write_index: 0,
            phase: 0.0,
        }
    }

    /// Read one interpolated sample from `buffer`, `delay` samples behind the
    /// write head, with wraparound.
    #[inline]
    fn read_interp(buffer: &[f64; BUF_SIZE], write_index: usize, delay: f64) -> f64 {
        let read_pos = write_index as f64 - delay;
        let base = read_pos.floor();
        let frac = read_pos - base;

        let n = BUF_SIZE as i64;
        let i0 = (((base as i64) % n) + n) % n; // wrap into [0, BUF_SIZE)
        let idx0 = i0 as usize;
        let idx1 = (idx0 + 1) % BUF_SIZE;

        buffer[idx0] * (1.0 - frac) + buffer[idx1] * frac
    }

    /// Pitch-shift interleaved stereo samples in place by `ratio`.
    #[inline]
    fn process(&mut self, samples: &mut [f64], ratio: f64) {
        // Per output sample the read head must advance by `ratio` while the write
        // head advances by 1; since read_pos = write - phase*WINDOW, the phase
        // must change by (1 - ratio)/WINDOW each frame.
        let phase_inc = (1.0 - ratio) / WINDOW;

        for frame in samples.chunks_exact_mut(2) {
            // Write the current input into both delay lines.
            self.buffers[0][self.write_index] = frame[0];
            self.buffers[1][self.write_index] = frame[1];

            // Two taps, offset by half a window, triangular-crossfaded.
            let p1 = self.phase;
            let p2 = if p1 + 0.5 >= 1.0 { p1 - 0.5 } else { p1 + 0.5 };
            let w1 = triangular_window(p1);
            let w2 = triangular_window(p2);
            let d1 = p1 * WINDOW;
            let d2 = p2 * WINDOW;

            frame[0] = w1 * Self::read_interp(&self.buffers[0], self.write_index, d1)
                + w2 * Self::read_interp(&self.buffers[0], self.write_index, d2);
            frame[1] = w1 * Self::read_interp(&self.buffers[1], self.write_index, d1)
                + w2 * Self::read_interp(&self.buffers[1], self.write_index, d2);

            // Advance write head and read phase.
            self.write_index = (self.write_index + 1) % BUF_SIZE;
            self.phase += phase_inc;
            if self.phase >= 1.0 {
                self.phase -= 1.0;
            } else if self.phase < 0.0 {
                self.phase += 1.0;
            }
        }
    }
}

/// A thread-safe handle for updating the pitch offset (in cents) while audio is
/// playing. Cloned into both the player (writer) and the processor (reader).
///
/// Fully lock-free: the f64 value is stored as bits in an `AtomicU64`, and a
/// generation counter lets the audio thread cheaply detect changes.
#[derive(Clone)]
pub struct PitchController {
    generation: std::sync::Arc<std::sync::atomic::AtomicU64>,
    value: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl PitchController {
    pub fn new(cents: f64) -> Self {
        Self {
            generation: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            value: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(cents.to_bits())),
        }
    }

    /// Update the pitch offset in cents. Lock-free; safe to call from any thread.
    pub fn update(&self, cents: f64) {
        self.value
            .store(cents.to_bits(), std::sync::atomic::Ordering::Release);
        self.generation
            .fetch_add(1, std::sync::atomic::Ordering::Release);
    }

    fn generation(&self) -> u64 {
        self.generation.load(std::sync::atomic::Ordering::Acquire)
    }

    fn read(&self) -> f64 {
        f64::from_bits(self.value.load(std::sync::atomic::Ordering::Acquire))
    }
}

/// A [`Processor`] that pitch-shifts audio by a live-adjustable number of cents.
///
/// It is a no-op (passthrough) at 0 cents. The delay-line state persists across
/// buffers so live changes to the offset are smooth.
pub struct PitchProcessor {
    controller: PitchController,
    cents: f64,
    ratio: f64,
    shifter: PitchShifter,
    generation: u64,
}

impl PitchProcessor {
    /// Create a processor bound to the given controller, seeding the ratio from
    /// the controller's current configuration.
    pub fn new(controller: PitchController) -> Self {
        let cents = controller.read();
        let ratio = cents_to_ratio(cents);
        let generation = controller.generation();
        Self {
            controller,
            cents,
            ratio,
            shifter: PitchShifter::new(),
            generation,
        }
    }

    /// Cheaply check the controller for an updated offset and recompute the
    /// ratio if it changed. Only takes the mutex when the generation advances.
    #[inline]
    fn refresh(&mut self) {
        let generation = self.controller.generation();
        if generation != self.generation {
            let cents = self.controller.read();
            self.cents = cents;
            self.ratio = cents_to_ratio(cents);
            self.generation = generation;
        }
    }

    /// Whether the current configuration would alter the audio.
    #[inline]
    fn would_process(&self) -> bool {
        self.cents.abs() > PITCH_EPSILON_CENTS
    }
}

impl Processor for PitchProcessor {
    fn name(&self) -> &str {
        "pitch"
    }

    fn is_active(&self) -> bool {
        self.would_process()
    }

    fn process(&mut self, buffer: &mut AudioBuffer) {
        self.refresh();
        // Only meaningful for stereo audio, and only when shifted off unity.
        if !self.would_process() || buffer.channels != 2 {
            return;
        }
        self.shifter.process(&mut buffer.samples, self.ratio);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    #[test]
    fn zero_cents_is_unity_ratio() {
        assert_eq!(cents_to_ratio(0.0), 1.0);
    }

    #[test]
    fn octave_up_doubles_ratio() {
        assert!((cents_to_ratio(1200.0) - 2.0).abs() < 1e-9);
    }

    #[test]
    fn octave_down_halves_ratio() {
        assert!((cents_to_ratio(-1200.0) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn triangular_windows_sum_to_one() {
        for i in 0..1000 {
            let p1 = i as f64 / 1000.0;
            let p2 = if p1 + 0.5 >= 1.0 { p1 - 0.5 } else { p1 + 0.5 };
            let sum = triangular_window(p1) + triangular_window(p2);
            assert!(
                (sum - 1.0).abs() < 1e-12,
                "windows summed to {sum} at p={p1}"
            );
        }
    }

    #[test]
    fn centered_processor_is_inactive() {
        let processor = PitchProcessor::new(PitchController::new(0.0));
        assert!(!processor.is_active());
    }

    #[test]
    fn shifted_processor_is_active() {
        let processor = PitchProcessor::new(PitchController::new(25.0));
        assert!(processor.is_active());
    }

    #[test]
    fn zero_cents_leaves_audio_untouched() {
        let mut processor = PitchProcessor::new(PitchController::new(0.0));
        let mut buffer = AudioBuffer::stereo_44100(vec![0.1, -0.2, 0.3, -0.4, 0.5, -0.6]);
        let original = buffer.clone();
        processor.process(&mut buffer);
        assert_eq!(buffer, original);
    }

    #[test]
    fn output_length_is_preserved() {
        let mut processor = PitchProcessor::new(PitchController::new(50.0));
        let mut buffer = AudioBuffer::stereo_44100(vec![0.25; 4096]);
        processor.process(&mut buffer);
        assert_eq!(buffer.samples.len(), 4096);
    }

    #[test]
    fn constant_signal_is_preserved_after_fill() {
        // A DC / constant signal shifted by any ratio is still that constant,
        // because the crossfade windows sum to 1 and every tap reads the same
        // value once the ring buffer has filled. This validates the read +
        // crossfade math.
        let mut processor = PitchProcessor::new(PitchController::new(37.0));
        let mut buffer = AudioBuffer::stereo_44100(vec![0.5; BUF_SIZE * 2 * 2]);
        processor.process(&mut buffer);
        // After the buffer has filled, the tail must be ~0.5 in both channels.
        for &s in buffer.samples.iter().rev().take(200) {
            assert!((s - 0.5).abs() < 1e-9, "expected ~0.5, got {s}");
        }
    }

    #[test]
    fn output_is_finite() {
        let mut processor = PitchProcessor::new(PitchController::new(-80.0));
        let mut samples: Vec<f64> = (0..8192)
            .map(|i| (2.0 * PI * 220.0 * i as f64 / 44100.0).sin())
            .flat_map(|s| [s, s])
            .collect();
        let mut buffer = AudioBuffer::stereo_44100(std::mem::take(&mut samples));
        processor.process(&mut buffer);
        assert!(buffer.samples.iter().all(|s| s.is_finite()));
    }

    /// Count zero crossings in the trailing portion of the left channel.
    fn zero_crossings_tail(samples: &[f64], tail_frames: usize) -> usize {
        let total = samples.len() / 2;
        let start = total.saturating_sub(tail_frames);
        let mut count = 0;
        let mut prev = samples[start * 2];
        for frame in (start + 1)..total {
            let v = samples[frame * 2];
            if (prev < 0.0 && v >= 0.0) || (prev >= 0.0 && v < 0.0) {
                count += 1;
            }
            prev = v;
        }
        count
    }

    #[test]
    fn pitch_up_increases_zero_crossing_rate() {
        // A clean sine shifted up should oscillate faster (more zero crossings)
        // than the same sine shifted down, over the settled tail.
        let make_sine = || -> Vec<f64> {
            (0..44_100)
                .map(|i| (2.0 * PI * 150.0 * i as f64 / 44100.0).sin())
                .flat_map(|s| [s, s])
                .collect()
        };

        let mut up = PitchProcessor::new(PitchController::new(100.0));
        let mut buf_up = AudioBuffer::stereo_44100(make_sine());
        up.process(&mut buf_up);

        let mut down = PitchProcessor::new(PitchController::new(-100.0));
        let mut buf_down = AudioBuffer::stereo_44100(make_sine());
        down.process(&mut buf_down);

        let crossings_up = zero_crossings_tail(&buf_up.samples, 10_000);
        let crossings_down = zero_crossings_tail(&buf_down.samples, 10_000);
        assert!(
            crossings_up > crossings_down,
            "pitch-up crossings ({crossings_up}) should exceed pitch-down ({crossings_down})"
        );
    }

    #[test]
    fn processor_picks_up_live_updates() {
        let controller = PitchController::new(0.0);
        let mut processor = PitchProcessor::new(controller.clone());
        assert!(!processor.is_active());

        controller.update(60.0);

        let mut buffer = AudioBuffer::stereo_44100(vec![0.2; 256]);
        processor.process(&mut buffer);
        assert!(processor.is_active());
        assert!((processor.ratio - cents_to_ratio(60.0)).abs() < 1e-12);
    }
}
