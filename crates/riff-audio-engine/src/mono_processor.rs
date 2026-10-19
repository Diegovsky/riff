//! Mono downmix processor.
//!
//! Averages the left and right channels of interleaved stereo audio and writes
//! the result to both channels, producing a mono signal. Toggled live via a
//! [`MonoController`].

use super::{AudioBuffer, Processor};

/// A thread-safe handle for toggling mono downmixing while audio is playing.
///
/// Uses the same lock-free generation-counter pattern as the other controllers
/// so the audio thread can cheaply detect changes.
#[derive(Clone)]
pub struct MonoController {
    generation: std::sync::Arc<std::sync::atomic::AtomicU64>,
    enabled: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl MonoController {
    pub fn new(enabled: bool) -> Self {
        Self {
            generation: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            enabled: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(enabled)),
        }
    }

    /// Update the mono setting. Cheap; safe to call from any thread.
    pub fn update(&self, enabled: bool) {
        self.enabled
            .store(enabled, std::sync::atomic::Ordering::Release);
        self.generation
            .fetch_add(1, std::sync::atomic::Ordering::Release);
    }

    fn generation(&self) -> u64 {
        self.generation.load(std::sync::atomic::Ordering::Acquire)
    }

    fn read(&self) -> bool {
        self.enabled.load(std::sync::atomic::Ordering::Acquire)
    }
}

/// A [`Processor`] that downmixes stereo audio to mono when enabled.
pub struct MonoProcessor {
    controller: MonoController,
    enabled: bool,
    generation: u64,
}

impl MonoProcessor {
    /// Create a processor bound to the given controller.
    pub fn new(controller: MonoController) -> Self {
        let enabled = controller.read();
        let generation = controller.generation();
        Self {
            controller,
            enabled,
            generation,
        }
    }

    /// Cheaply poll the controller for a changed mono setting.
    #[inline]
    fn refresh(&mut self) {
        let generation = self.controller.generation();
        if generation != self.generation {
            self.enabled = self.controller.read();
            self.generation = generation;
        }
    }

    /// Mix interleaved stereo samples to mono in-place. Each frame's left and
    /// right channels are averaged, then written to both channels.
    #[inline]
    fn mix_to_mono(samples: &mut [f64]) {
        for frame in samples.chunks_exact_mut(2) {
            let mono = (frame[0] + frame[1]) * 0.5;
            frame[0] = mono;
            frame[1] = mono;
        }
    }
}

impl Processor for MonoProcessor {
    fn name(&self) -> &str {
        "mono"
    }

    fn is_active(&self) -> bool {
        self.enabled
    }

    fn process(&mut self, buffer: &mut AudioBuffer) {
        self.refresh();
        // Only stereo (or more) audio can be downmixed to mono.
        if self.enabled && buffer.channels == 2 {
            Self::mix_to_mono(&mut buffer.samples);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_processor_is_inactive() {
        let processor = MonoProcessor::new(MonoController::new(false));
        assert!(!processor.is_active());
    }

    #[test]
    fn enabled_processor_is_active() {
        let processor = MonoProcessor::new(MonoController::new(true));
        assert!(processor.is_active());
    }

    #[test]
    fn disabled_processor_leaves_audio_untouched() {
        let mut processor = MonoProcessor::new(MonoController::new(false));
        let mut buffer = AudioBuffer::stereo_44100(vec![1.0, -1.0, 0.5, 0.25]);
        let original = buffer.clone();
        processor.process(&mut buffer);
        assert_eq!(buffer, original);
    }

    #[test]
    fn enabled_processor_averages_channels() {
        let mut processor = MonoProcessor::new(MonoController::new(true));
        // Values chosen to be exact in binary floating point.
        // Frame 0: L=1.0 R=0.0 -> 0.5. Frame 1: L=0.5 R=-0.5 -> 0.0.
        let mut buffer = AudioBuffer::stereo_44100(vec![1.0, 0.0, 0.5, -0.5]);
        processor.process(&mut buffer);
        assert_eq!(buffer.samples, vec![0.5, 0.5, 0.0, 0.0]);
    }

    #[test]
    fn processor_picks_up_live_updates() {
        let controller = MonoController::new(false);
        let mut processor = MonoProcessor::new(controller.clone());
        assert!(!processor.is_active());

        controller.update(true);

        let mut buffer = AudioBuffer::stereo_44100(vec![1.0, 0.0]);
        processor.process(&mut buffer);
        assert!(processor.is_active());
        assert_eq!(buffer.samples, vec![0.5, 0.5]);
    }
}
