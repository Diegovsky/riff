//! The boundary between librespot and the audio engine pipeline.
//!
//! [`CaptureSink`] implements librespot's [`Sink`] trait. It "captures" the
//! decoded PCM that librespot would otherwise write straight to the audio
//! backend, wraps it in an [`AudioBuffer`], runs it through a [`ProcessorChain`],
//! and forwards the processed audio to the real backend sink.

use librespot::playback::audio_backend::{Sink, SinkResult};
use librespot::playback::convert::Converter;
use librespot::playback::decoder::AudioPacket;

use super::{AudioBuffer, ProcessorChain};

/// A [`Sink`] that runs decoded audio through the audio engine pipeline
/// before forwarding it to the wrapped backend sink.
///
/// Only [`AudioPacket::Samples`] are processed; [`AudioPacket::Raw`]
/// (passthrough/encoded) packets are forwarded untouched, since the pipeline
/// operates on decoded PCM.
pub struct CaptureSink {
    inner: Box<dyn Sink>,
    chain: ProcessorChain,
}

impl CaptureSink {
    /// Wrap a backend sink with the given processor chain.
    pub fn wrap(inner: Box<dyn Sink>, chain: ProcessorChain) -> Box<dyn Sink> {
        Box::new(Self { inner, chain })
    }
}

impl Sink for CaptureSink {
    fn start(&mut self) -> SinkResult<()> {
        debug!(
            "audio engine pipeline: {} (active={})",
            self.chain.summary(),
            self.chain.is_active()
        );
        self.inner.start()
    }

    fn stop(&mut self) -> SinkResult<()> {
        self.inner.stop()
    }

    fn write(&mut self, packet: AudioPacket, converter: &mut Converter) -> SinkResult<()> {
        match packet {
            AudioPacket::Samples(samples) => {
                // Hand the samples to the pipeline. Every processor refreshes its
                // own config and self-gates work, so this is cheap when nothing
                // is active and always reflects the latest live settings.
                let mut buffer = AudioBuffer::stereo_44100(samples);
                self.chain.process(&mut buffer);
                self.inner
                    .write(AudioPacket::Samples(buffer.samples), converter)
            }
            // Encoded/passthrough data is not decoded PCM; forward untouched.
            raw => self.inner.write(raw, converter),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::eq_processor::NUM_BANDS;
    use super::*;
    use crate::audio_engine::{
        EqController, EqProcessor, MonoController, MonoProcessor, Processor,
    };
    use std::sync::{Arc, Mutex};

    /// A backend sink stand-in that records everything written to it.
    #[derive(Clone, Default)]
    struct RecordingSink {
        samples: Arc<Mutex<Vec<f64>>>,
        raw: Arc<Mutex<Vec<u8>>>,
        started: Arc<Mutex<bool>>,
        stopped: Arc<Mutex<bool>>,
    }

    impl Sink for RecordingSink {
        fn start(&mut self) -> SinkResult<()> {
            *self.started.lock().unwrap() = true;
            Ok(())
        }
        fn stop(&mut self) -> SinkResult<()> {
            *self.stopped.lock().unwrap() = true;
            Ok(())
        }
        fn write(&mut self, packet: AudioPacket, _converter: &mut Converter) -> SinkResult<()> {
            match packet {
                AudioPacket::Samples(s) => self.samples.lock().unwrap().extend_from_slice(&s),
                AudioPacket::Raw(d) => self.raw.lock().unwrap().extend_from_slice(&d),
            }
            Ok(())
        }
    }

    fn boosted_eq_controller() -> EqController {
        let mut band_gains = [0.0; NUM_BANDS];
        band_gains[5] = 6.0;
        EqController::new(band_gains)
    }

    #[test]
    fn passthrough_when_chain_empty() {
        let backend = RecordingSink::default();
        let recorded = backend.samples.clone();
        let mut sink = CaptureSink::wrap(Box::new(backend), ProcessorChain::new());

        let mut converter = Converter::new(None);
        let input = vec![0.1, 0.2, 0.3, 0.4];
        sink.write(AudioPacket::Samples(input.clone()), &mut converter)
            .unwrap();

        assert_eq!(*recorded.lock().unwrap(), input);
    }

    #[test]
    fn raw_packets_forwarded_untouched() {
        let backend = RecordingSink::default();
        let recorded_raw = backend.raw.clone();
        let recorded_samples = backend.samples.clone();

        // A chain with active processors must not touch Raw packets.
        let chain = ProcessorChain::new().with(Box::new(EqProcessor::new(boosted_eq_controller())));
        let mut sink = CaptureSink::wrap(Box::new(backend), chain);

        let mut converter = Converter::new(None);
        let raw = vec![1u8, 2, 3, 4, 5];
        sink.write(AudioPacket::Raw(raw.clone()), &mut converter)
            .unwrap();

        assert_eq!(*recorded_raw.lock().unwrap(), raw);
        assert!(recorded_samples.lock().unwrap().is_empty());
    }

    #[test]
    fn samples_run_through_chain() {
        // Mono downmix makes the effect easy to assert.
        let backend = RecordingSink::default();
        let recorded = backend.samples.clone();

        let chain =
            ProcessorChain::new().with(Box::new(MonoProcessor::new(MonoController::new(true))));
        let mut sink = CaptureSink::wrap(Box::new(backend), chain);

        let mut converter = Converter::new(None);
        // Frame 0: L=1.0 R=0.0 -> 0.5. Frame 1: L=0.5 R=-0.5 -> 0.0.
        sink.write(
            AudioPacket::Samples(vec![1.0, 0.0, 0.5, -0.5]),
            &mut converter,
        )
        .unwrap();

        assert_eq!(*recorded.lock().unwrap(), vec![0.5, 0.5, 0.0, 0.0]);
    }

    #[test]
    fn start_and_stop_delegate_to_inner() {
        let backend = RecordingSink::default();
        let started = backend.started.clone();
        let stopped = backend.stopped.clone();
        let mut sink = CaptureSink::wrap(Box::new(backend), ProcessorChain::new());

        sink.start().unwrap();
        sink.stop().unwrap();

        assert!(*started.lock().unwrap());
        assert!(*stopped.lock().unwrap());
    }

    #[test]
    fn eq_processor_matches_reference() {
        // Full pipeline (EQ only) must match the standalone EqProcessor output.
        let backend = RecordingSink::default();
        let recorded = backend.samples.clone();

        let controller = boosted_eq_controller();
        let chain = ProcessorChain::new().with(Box::new(EqProcessor::new(controller.clone())));
        let mut sink = CaptureSink::wrap(Box::new(backend), chain);

        let input: Vec<f64> = (0..2048).map(|i| (i as f64 * 0.01).sin()).collect();

        let mut converter = Converter::new(None);
        sink.write(AudioPacket::Samples(input.clone()), &mut converter)
            .unwrap();

        let mut reference = EqProcessor::new(controller);
        let mut expected = AudioBuffer::stereo_44100(input);
        reference.process(&mut expected);

        assert_eq!(*recorded.lock().unwrap(), expected.samples);
    }
}
