//! Stereo pan / balance processor.
//!
//! Adjusts the left/right balance of interleaved stereo audio. The pan value
//! ranges from `-1.0` (full left) through `0.0` (centered, no change) to `+1.0`
//! (full right). This is a *balance* control: panning towards one side
//! attenuates the opposite channel rather than moving signal between channels,
//! which keeps it simple and artifact-free.
//!
//! Configuration is changed live via a [`PanController`], using the same
//! lock-free generation-counter pattern as the equalizer.

use super::{AudioBuffer, Processor};

/// Pan values with magnitude below this are treated as centered (no-op).
const PAN_EPSILON: f64 = 1.0e-4;

/// Compute the per-channel linear gains for a given pan position.
///
/// - `pan == 0.0` → both gains `1.0` (passthrough).
/// - `pan > 0.0` (towards right) → the left channel is attenuated.
/// - `pan < 0.0` (towards left) → the right channel is attenuated.
///
/// At the extremes (`±1.0`) the opposite channel is fully silenced.
#[inline]
fn channel_gains(pan: f64) -> (f64, f64) {
    let pan = pan.clamp(-1.0, 1.0);
    let left = if pan > 0.0 { 1.0 - pan } else { 1.0 };
    let right = if pan < 0.0 { 1.0 + pan } else { 1.0 };
    (left, right)
}

/// A thread-safe handle for updating the pan configuration while audio is
/// playing. Cloned into both the player (writer) and the processor (reader).
///
/// Fully lock-free: the f64 value is stored as bits in an `AtomicU64`, and a
/// generation counter lets the audio thread cheaply detect changes.
#[derive(Clone)]
pub struct PanController {
    generation: std::sync::Arc<std::sync::atomic::AtomicU64>,
    value: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl PanController {
    pub fn new(pan: f64) -> Self {
        Self {
            generation: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            value: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(pan.to_bits())),
        }
    }

    /// Update the pan configuration. Lock-free; safe to call from any thread.
    pub fn update(&self, pan: f64) {
        self.value
            .store(pan.to_bits(), std::sync::atomic::Ordering::Release);
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

/// A [`Processor`] that applies a stereo balance to an [`AudioBuffer`].
///
/// Pan is always enabled; it simply has no effect while centered (`pan == 0`).
/// The pan can be reconfigured live via its [`PanController`]; the processor
/// cheaply checks a generation counter on every buffer and recomputes its gains
/// only when the configuration actually changed.
pub struct PanProcessor {
    controller: PanController,
    pan: f64,
    left_gain: f64,
    right_gain: f64,
    generation: u64,
}

impl PanProcessor {
    /// Create a processor bound to the given controller, seeding gains from the
    /// controller's current configuration.
    pub fn new(controller: PanController) -> Self {
        let pan = controller.read();
        let (left_gain, right_gain) = channel_gains(pan);
        let generation = controller.generation();
        Self {
            controller,
            pan,
            left_gain,
            right_gain,
            generation,
        }
    }

    /// Cheaply check the controller for updated settings and recompute gains if
    /// they changed. Only takes the mutex when the generation counter advances.
    #[inline]
    fn refresh(&mut self) {
        let generation = self.controller.generation();
        if generation != self.generation {
            let pan = self.controller.read();
            self.pan = pan;
            let (left_gain, right_gain) = channel_gains(pan);
            self.left_gain = left_gain;
            self.right_gain = right_gain;
            self.generation = generation;
        }
    }

    /// Whether the current configuration would alter the audio.
    #[inline]
    fn would_process(&self) -> bool {
        self.pan.abs() > PAN_EPSILON
    }
}

impl Processor for PanProcessor {
    fn name(&self) -> &str {
        "pan"
    }

    fn is_active(&self) -> bool {
        self.would_process()
    }

    fn process(&mut self, buffer: &mut AudioBuffer) {
        self.refresh();
        // Only meaningful for stereo audio, and only when panned off-center.
        if !self.would_process() || buffer.channels != 2 {
            return;
        }
        let (left_gain, right_gain) = (self.left_gain, self.right_gain);
        for frame in buffer.samples.chunks_exact_mut(2) {
            frame[0] *= left_gain;
            frame[1] *= right_gain;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn center_gains_are_unity() {
        assert_eq!(channel_gains(0.0), (1.0, 1.0));
    }

    #[test]
    fn full_right_silences_left() {
        assert_eq!(channel_gains(1.0), (0.0, 1.0));
    }

    #[test]
    fn full_left_silences_right() {
        assert_eq!(channel_gains(-1.0), (1.0, 0.0));
    }

    #[test]
    fn partial_right_attenuates_left_only() {
        let (l, r) = channel_gains(0.25);
        assert!((l - 0.75).abs() < 1e-12);
        assert_eq!(r, 1.0);
    }

    #[test]
    fn gains_clamp_out_of_range_input() {
        assert_eq!(channel_gains(2.0), (0.0, 1.0));
        assert_eq!(channel_gains(-2.0), (1.0, 0.0));
    }

    #[test]
    fn centered_processor_is_inactive() {
        let processor = PanProcessor::new(PanController::new(0.0));
        assert!(!processor.is_active());
    }

    #[test]
    fn panned_processor_is_active() {
        let processor = PanProcessor::new(PanController::new(0.5));
        assert!(processor.is_active());
    }

    #[test]
    fn centered_processor_leaves_audio_untouched() {
        let mut processor = PanProcessor::new(PanController::new(0.0));
        let mut buffer = AudioBuffer::stereo_44100(vec![1.0, 1.0, 0.5, 0.5]);
        let original = buffer.clone();
        processor.process(&mut buffer);
        assert_eq!(buffer, original);
    }

    #[test]
    fn pan_right_attenuates_left_channel() {
        let mut processor = PanProcessor::new(PanController::new(0.5));
        // Frames: (L, R) = (1.0, 1.0), (0.4, 0.8)
        let mut buffer = AudioBuffer::stereo_44100(vec![1.0, 1.0, 0.4, 0.8]);
        processor.process(&mut buffer);
        // left *= 0.5, right unchanged
        assert_eq!(buffer.samples, vec![0.5, 1.0, 0.2, 0.8]);
    }

    #[test]
    fn pan_full_left_silences_right_channel() {
        let mut processor = PanProcessor::new(PanController::new(-1.0));
        let mut buffer = AudioBuffer::stereo_44100(vec![1.0, 1.0, 0.4, 0.8]);
        processor.process(&mut buffer);
        assert_eq!(buffer.samples, vec![1.0, 0.0, 0.4, 0.0]);
    }

    #[test]
    fn processor_picks_up_live_updates() {
        let controller = PanController::new(0.0);
        let mut processor = PanProcessor::new(controller.clone());
        assert!(!processor.is_active());

        controller.update(1.0);

        let mut buffer = AudioBuffer::stereo_44100(vec![1.0, 1.0]);
        processor.process(&mut buffer);
        assert!(processor.is_active());
        // Full right: left silenced.
        assert_eq!(buffer.samples, vec![0.0, 1.0]);
    }
}
