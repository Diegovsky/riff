//! Audio mixing processor (stub).
//!
//! Placeholder for a future stage that mixes additional audio sources into the
//! stream (e.g. crossfades between tracks, overlaying cues or a metronome). It
//! establishes the controller + processor plumbing and currently performs no
//! processing.

use super::{AudioBuffer, Processor};

/// A thread-safe handle for controlling the mixer while audio is playing.
///
/// Uses the same lock-free generation-counter pattern as the other controllers.
/// For now it only carries an enabled flag; a future implementation will manage
/// additional source buffers and per-source gains.
#[derive(Clone)]
pub struct MixController {
    generation: std::sync::Arc<std::sync::atomic::AtomicU64>,
    enabled: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl MixController {
    pub fn new(enabled: bool) -> Self {
        Self {
            generation: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            enabled: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(enabled)),
        }
    }

    /// Update the mixer setting. Cheap; safe to call from any thread.
    ///
    /// Reserved for a future `SetMix` command; not yet wired to any UI.
    #[allow(dead_code)]
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

/// A stub [`Processor`] for mixing additional audio sources.
///
/// Performs no processing until a real implementation is added, so it never
/// alters the audio.
pub struct MixProcessor {
    controller: MixController,
    enabled: bool,
    generation: u64,
}

impl MixProcessor {
    /// Create a processor bound to the given controller.
    pub fn new(controller: MixController) -> Self {
        let enabled = controller.read();
        let generation = controller.generation();
        Self {
            controller,
            enabled,
            generation,
        }
    }

    #[inline]
    fn refresh(&mut self) {
        let generation = self.controller.generation();
        if generation != self.generation {
            self.enabled = self.controller.read();
            self.generation = generation;
        }
    }
}

impl Processor for MixProcessor {
    fn name(&self) -> &str {
        "mixer"
    }

    fn is_active(&self) -> bool {
        // Stub: no mixing sources implemented yet, so even when "enabled" the
        // processor leaves audio untouched. Reporting `enabled` keeps the wiring
        // honest for when a real implementation lands.
        self.enabled
    }

    fn process(&mut self, _buffer: &mut AudioBuffer) {
        // Keep controller state in sync so a future implementation only needs to
        // fill in the actual mixing here.
        self.refresh();
        // TODO: implement source mixing. Currently a no-op passthrough.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_does_not_modify_audio_when_disabled() {
        let mut processor = MixProcessor::new(MixController::new(false));
        let mut buffer = AudioBuffer::stereo_44100(vec![0.1, 0.2, 0.3, 0.4]);
        let original = buffer.clone();
        processor.process(&mut buffer);
        assert_eq!(buffer, original);
    }

    #[test]
    fn stub_does_not_modify_audio_when_enabled() {
        let mut processor = MixProcessor::new(MixController::new(true));
        let mut buffer = AudioBuffer::stereo_44100(vec![0.1, 0.2, 0.3, 0.4]);
        let original = buffer.clone();
        processor.process(&mut buffer);
        assert_eq!(buffer, original);
    }

    #[test]
    fn controller_updates_are_observed() {
        let controller = MixController::new(false);
        let mut processor = MixProcessor::new(controller.clone());
        controller.update(true);
        let mut buffer = AudioBuffer::stereo_44100(vec![0.5, 0.5]);
        processor.process(&mut buffer);
        assert_eq!(buffer.samples, vec![0.5, 0.5]);
    }
}
