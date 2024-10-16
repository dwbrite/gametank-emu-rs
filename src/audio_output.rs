use std::collections::VecDeque;
use std::ops::IndexMut;
use dasp_graph::{Buffer, Input, NodeData};
use dasp_interpolate::linear::Linear;
use dasp_interpolate::sinc::Sinc;
use dasp_signal::Signal;
use klingt::{AudioNode, Klingt};
use klingt::nodes::effect::SlewLimiter;
use klingt::nodes::sink::CpalMonoSink;
use rtrb::{Consumer, Producer, RingBuffer};
use tracing::{debug, trace};
use petgraph::prelude::NodeIndex;

pub struct GameTankSignal {
    buffer: Consumer<u8>,
}

impl GameTankSignal {
    pub fn new(buffer: Consumer<u8>) -> Self {
        Self {
            buffer,
        }
    }
}

impl Signal for GameTankSignal {
    type Frame = f32;

    fn next(&mut self) -> Self::Frame {
        if let Ok(sample) = self.buffer.pop() {
            (sample as f32 / 255.0) * 2.0 - 1.0
        } else {
            println!("FEED THE BUFFFEERRRRRR");
            0.0
        }
    }

    fn is_exhausted(&self) -> bool {
        self.buffer.slots() < 64
    }
}


#[derive(Debug)]
pub struct RtrbSource {
    output_buffer: Consumer<Buffer>
}

#[derive()]
pub struct GameTankAudio {
    pub producer: Producer<u8>,

    klingt: Klingt<GTNode>,

    idx_in: NodeIndex,
    idx_out: NodeIndex,

    pub resampled: VecDeque<f32>,

    pub output_queue: Producer<Buffer>, // ring buffer for output buffers

    pub sample_rate: f64,
    pub converter: Box<dyn Signal<Frame = f32> + Send>,
}

impl GameTankAudio {
    pub fn new(sample_rate: f64, target_sample_rate: f64) -> Self {
        // caps out around 48kHz, but technically the system can go higher...
        let (input_producer, input_buffer) = RingBuffer::<u8>::new(128); // Ring buffer to hold GameTank samples
        let (output_producer, output_consumer) = RingBuffer::<Buffer>::new(512); // Ring buffer to hold output buffers
        let interp = Linear::new(0.0, 0.0);

        // let frames = dasp_ring_buffer::Fixed::from(vec![0.0; 64]);
        // let interp = Sinc::new(frames);
        let signal = GameTankSignal::new(input_buffer);
        let converter = signal.from_hz_to_hz(interp, sample_rate, target_sample_rate);

        let mut klingt = Klingt::default();

        let sink = CpalMonoSink::default();
        let out_node = NodeData::new1(GTNode::CpalMonoSink(sink));

        let gt_node = NodeData::new1(GTNode::GameTankSource(RtrbSource{ output_buffer: output_consumer }));

        let idx_in = klingt.add_node(gt_node);
        let idx_out = klingt.add_node(out_node);

        klingt.add_edge(idx_in, idx_out, ());
        
        Self {
            producer: input_producer,
            klingt,
            idx_in,
            idx_out,
            resampled: VecDeque::with_capacity(1024),
            output_queue: output_producer,
            sample_rate,
            // target_sample_rate,
            converter: Box::new(converter),
        }
    }

    pub fn convert_to_output_buffers(&mut self) {
        while !self.converter.is_exhausted() {
            self.resampled.push_back(self.converter.next());
        }

        while self.resampled.len() >= 64 && self.output_queue.slots() >= 8 {
            if let Some(chunk) = self.resampled.drain(..64).collect::<Vec<_>>().try_into().ok() {
                let mut buf = Buffer::SILENT;
                for (b, v) in buf.iter_mut().zip::<[f32;64]>(chunk) {
                    *b = v;
                }
                self.output_queue.push(buf).unwrap()
            }
        }
    }

    pub fn process_audio(&mut self) {
        let mut ready_to_output = 0;
        if let GTNode::GameTankSource(src) = &mut self.klingt.index_mut(self.idx_in).node {
            ready_to_output = src.output_buffer.slots();
        }

        // Generate buffers in a loop
        let mut can_output = false;
        if let GTNode::CpalMonoSink(sink) = &mut self.klingt.index_mut(self.idx_out).node {
            can_output = sink.buffer.slots() >= 64 && ready_to_output >= 4;
        };

        while can_output {
            self.klingt.processor.process(&mut self.klingt.graph, self.idx_out);

            if let GTNode::GameTankSource(src) = &mut self.klingt.index_mut(self.idx_in).node {
                ready_to_output = src.output_buffer.slots();

                if let GTNode::CpalMonoSink(sink) = &mut self.klingt.index_mut(self.idx_out).node {
                    can_output = sink.buffer.slots() >= 64 && ready_to_output >= 4;
                };
                // sleep(Duration::from_millis(1)); // takes 1.33ms per 64 samples, so this should be safe
                trace!("ready to output {ready_to_output}");
            }
        }
    }
}

impl AudioNode for RtrbSource {
    fn process(&mut self, _inputs: &[Input], output: &mut [Buffer]) {
        let b = match self.output_buffer.pop() {
            Ok(buf) => { buf }
            Err(_) => { println!("FEED THE BUFFER"); Buffer::SILENT }
        };
        for buffer in output.iter_mut() {
            *buffer = b.clone();
        }
        debug!("processed rtrb source");
    }
}

#[enum_delegate::implement(AudioNode, pub trait AudioNode { fn process(&mut self, inputs: &[Input], output: &mut [Buffer]);})]
pub enum GTNode {
    CpalMonoSink(CpalMonoSink),
    GameTankSource(RtrbSource),
    SlewLimiter(SlewLimiter)
}