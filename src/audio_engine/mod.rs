//! The **audio engine**: a DSP pipeline that sits between librespot's decoder
//! and the audio backend.
//!
//! librespot decodes audio into interleaved `f64` samples and hands them to a
//! [`Sink`](librespot::playback::audio_backend::Sink). Instead of writing those
//! samples straight to the backend, Riff routes them through a
//! [`ProcessorChain`]: an ordered list of [`Processor`]s that can equalize, mix
//! to mono, pitch-shift, and (eventually) mix in additional sources before the
//! audio reaches the speakers.
//!
//! # Architecture
//!
//! ```text
//! librespot decoder
//!        │  AudioPacket::Samples
//!        ▼
//!   CaptureSink  ── implements librespot's Sink trait (the boundary)
//!        │  AudioBuffer
//!        ▼
//!   ProcessorChain
//!        │   EqProcessor → MonoProcessor → PanProcessor → PitchProcessor → MixProcessor
//!        ▼
//!   backend Sink (PulseAudio / ALSA)
//! ```
//!
//! Each [`Processor`] operates in place on an [`AudioBuffer`]. Processors expose
//! [`Processor::is_active`] so the chain can cheaply skip stages that would not
//! change the audio (e.g. a flat EQ or disabled mono). Live configuration
//! updates are delivered through per-processor *controllers* that use a
//! lock-free generation counter, so the audio thread never blocks on the UI.

mod buffer;
mod capture_sink;
mod eq_processor;
mod mix_processor;
mod mono_processor;
mod pan_processor;
mod pitch_processor;

pub use buffer::AudioBuffer;
pub use capture_sink::CaptureSink;
pub use eq_processor::{EqController, EqProcessor};
pub use mix_processor::{MixController, MixProcessor};
pub use mono_processor::{MonoController, MonoProcessor};
pub use pan_processor::{PanController, PanProcessor};
pub use pitch_processor::{PitchController, PitchProcessor};

/// A single audio-processing stage in the [`ProcessorChain`].
///
/// Implementors process an [`AudioBuffer`] in place. They must be `Send` so the
/// chain can live on librespot's audio thread.
pub trait Processor: Send {
    /// Human-readable name, used for logging and diagnostics.
    fn name(&self) -> &str;

    /// Whether this processor would currently modify the audio.
    ///
    /// This is a *diagnostic* hint reflecting the last-observed configuration
    /// (as of the most recent [`process`](Processor::process) call); it may lag
    /// a live configuration change by one buffer. It must **not** be used to
    /// decide whether to call [`process`](Processor::process) — doing so would
    /// prevent a disabled processor from ever observing its own re-enablement.
    fn is_active(&self) -> bool;

    /// Process the buffer in place.
    ///
    /// Implementations must first (cheaply) poll their controller for live
    /// configuration changes, then apply DSP only if currently active. This
    /// method is always safe to call — a disabled or no-op processor simply
    /// refreshes its state and returns, leaving the buffer untouched.
    fn process(&mut self, buffer: &mut AudioBuffer);
}

/// An ordered collection of [`Processor`]s applied in sequence.
///
/// The order is fixed at construction time (see the module docs for the
/// canonical order). Every processor's [`process`](Processor::process) is
/// invoked for each buffer so that live configuration changes are always
/// observed; each processor is responsible for cheaply short-circuiting when it
/// has no work to do.
pub struct ProcessorChain {
    processors: Vec<Box<dyn Processor>>,
}

impl ProcessorChain {
    /// Create an empty chain (pure passthrough).
    pub fn new() -> Self {
        Self {
            processors: Vec::new(),
        }
    }

    /// Append a processor to the end of the chain.
    pub fn push(&mut self, processor: Box<dyn Processor>) {
        self.processors.push(processor);
    }

    /// Builder-style variant of [`push`](ProcessorChain::push).
    pub fn with(mut self, processor: Box<dyn Processor>) -> Self {
        self.push(processor);
        self
    }

    /// Whether any processor in the chain currently reports itself active.
    ///
    /// Diagnostic only; see [`Processor::is_active`] for the staleness caveat.
    pub fn is_active(&self) -> bool {
        self.processors.iter().any(|p| p.is_active())
    }

    /// A human-readable description of the chain and which stages are currently
    /// active, e.g. `equalizer[on] -> mono[off] -> pitch[off] -> mixer[off]`.
    ///
    /// Intended for logging the pipeline composition; see
    /// [`Processor::is_active`] for the staleness caveat.
    pub fn summary(&self) -> String {
        if self.processors.is_empty() {
            return "<empty>".to_string();
        }
        self.processors
            .iter()
            .map(|p| {
                let state = if p.is_active() { "on" } else { "off" };
                format!("{}[{}]", p.name(), state)
            })
            .collect::<Vec<_>>()
            .join(" -> ")
    }

    /// Run the buffer through every processor, in order.
    ///
    /// Each processor refreshes its own configuration and self-gates any
    /// expensive work, so this always reflects the latest live settings.
    pub fn process(&mut self, buffer: &mut AudioBuffer) {
        for processor in self.processors.iter_mut() {
            processor.process(buffer);
        }
    }
}

impl Default for ProcessorChain {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A test processor that scales every sample by a constant factor.
    struct GainProcessor {
        factor: f64,
        active: bool,
        calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    impl GainProcessor {
        fn new(
            factor: f64,
            active: bool,
        ) -> (Self, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
            let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            (
                Self {
                    factor,
                    active,
                    calls: calls.clone(),
                },
                calls,
            )
        }
    }

    impl Processor for GainProcessor {
        fn name(&self) -> &str {
            "gain"
        }
        fn is_active(&self) -> bool {
            self.active
        }
        fn process(&mut self, buffer: &mut AudioBuffer) {
            // Real processors self-gate inside process(); model that here.
            if !self.active {
                return;
            }
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            for s in buffer.samples.iter_mut() {
                *s *= self.factor;
            }
        }
    }

    /// A processor that records the sample values it saw on entry, letting us
    /// assert ordering between stages.
    struct RecordingProcessor {
        seen: std::sync::Arc<std::sync::Mutex<Vec<f64>>>,
    }

    impl Processor for RecordingProcessor {
        fn name(&self) -> &str {
            "recording"
        }
        fn is_active(&self) -> bool {
            true
        }
        fn process(&mut self, buffer: &mut AudioBuffer) {
            self.seen.lock().unwrap().extend_from_slice(&buffer.samples);
        }
    }

    #[test]
    fn empty_chain_is_passthrough() {
        let mut chain = ProcessorChain::new();
        assert!(!chain.is_active());
        let mut buf = AudioBuffer::stereo_44100(vec![1.0, -1.0, 0.5, -0.5]);
        let original = buf.clone();
        chain.process(&mut buf);
        assert_eq!(buf, original);
    }

    #[test]
    fn active_processor_modifies_buffer() {
        let (gain, _) = GainProcessor::new(2.0, true);
        let mut chain = ProcessorChain::new().with(Box::new(gain));
        assert!(chain.is_active());
        let mut buf = AudioBuffer::stereo_44100(vec![1.0, 2.0, 3.0, 4.0]);
        chain.process(&mut buf);
        assert_eq!(buf.samples, vec![2.0, 4.0, 6.0, 8.0]);
    }

    #[test]
    fn inactive_processor_is_skipped() {
        let (gain, calls) = GainProcessor::new(2.0, false);
        let mut chain = ProcessorChain::new().with(Box::new(gain));
        assert!(!chain.is_active());
        let mut buf = AudioBuffer::stereo_44100(vec![1.0, 2.0]);
        let original = buf.clone();
        chain.process(&mut buf);
        // Unchanged, and process() was never called.
        assert_eq!(buf, original);
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[test]
    fn processors_run_in_order() {
        // gain x2, then record. The recorder must see the doubled values,
        // proving the gain ran first.
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let (gain, _) = GainProcessor::new(2.0, true);
        let recorder = RecordingProcessor { seen: seen.clone() };

        let mut chain = ProcessorChain::new()
            .with(Box::new(gain))
            .with(Box::new(recorder));

        let mut buf = AudioBuffer::stereo_44100(vec![1.0, 2.0]);
        chain.process(&mut buf);

        assert_eq!(*seen.lock().unwrap(), vec![2.0, 4.0]);
    }

    #[test]
    fn summary_describes_stages_and_state() {
        let (active, _) = GainProcessor::new(2.0, true);
        let (inactive, _) = GainProcessor::new(2.0, false);
        let chain = ProcessorChain::new()
            .with(Box::new(active))
            .with(Box::new(inactive));
        assert_eq!(chain.summary(), "gain[on] -> gain[off]");
    }

    #[test]
    fn summary_of_empty_chain() {
        assert_eq!(ProcessorChain::new().summary(), "<empty>");
    }
}
