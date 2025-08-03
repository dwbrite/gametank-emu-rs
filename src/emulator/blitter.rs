use std::cmp::PartialEq;
use std::intrinsics::{add_with_overflow, wrapping_add};
use std::ops::{Not, BitAnd};
use std::time::Instant;
use futures::AsyncReadExt;
use tracing::{debug, info, warn};
use crate::emulator::blitter::Signal::{High, Low};
use crate::emulator::gametank_bus::{CpuBus};

#[derive(Debug, Copy, Clone)]
pub struct FlipFlop {
    last_clock: Signal,
    last_data: Signal,
}

impl FlipFlop {
    pub fn cycle(&mut self, preset: Signal, data: Signal, clock: Signal, clear: Signal) -> Signal {
        if clear == Low {
            self.last_data = Low;
        } else if preset == Low {
            self.last_data = High;
        } else if clock == High && self.last_clock == Low {
            self.last_data = data;
        }
        self.last_clock = clock;
        self.last_data
    }

    pub fn val(&self) -> Signal {
        self.last_data
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Signal {
    Low,
    High,
}

impl Signal {
    pub fn nand(self, other: Self) -> Self {
        match (self, other) {
            (High, High) => Low,
            _ => High
        }
    }

    pub fn and(self, other: Self) -> Self {
        match (self, other) {
            (High, High) => High,
            _ => Low
        }
    }
}

impl From<bool> for Signal {
    fn from(value: bool) -> Self {
        if value {
            High
        } else {
            Low
        }
    }
}

impl Not for Signal {
    type Output = Signal;

    fn not(self) -> Self::Output {
        match self {
            Low => High,
            High => Low
        }
    }
}

/// this is _more or less_ a 74hc40103
#[derive(Debug)]
pub struct Counter {
    pub counter: u8,
    pub last_clock: Signal,
}

impl Counter {
    pub fn cycle(&mut self, clock_pulse: Signal, parallel_load: Signal, terminal_enable: Signal, parallel_enable: Signal, data: u8) -> Signal {
        let mut output = High;

        if parallel_load == Low {
            self.counter = data;
        } else if self.last_clock == Low && clock_pulse == High {
            if parallel_enable == Low {
                self.counter = data;
            } else if terminal_enable == Low && self.counter > 0 {
                self.counter -= 1;
            }
        }

        if terminal_enable == Low && self.counter == 0 {
            output = Low;
        }

        self.last_clock = clock_pulse;
        output
    }
}

#[derive(Debug)]
pub struct CountUp {
    pub counter: u8,
    pub last_clock: Signal,
}

impl CountUp {
    pub fn cycle(&mut self, _load: Signal, enp: Signal, ent: Signal, clk: Signal, load_val: u8) -> u8 {
        if _load == Low {
            self.counter = load_val;
        } else if enp == High && ent == High && clk == High && self.last_clock == Low {
            self.counter = self.counter.wrapping_add(1);
        }

        self.last_clock = clk;
        self.counter
    }
}

#[derive(Debug)]
pub struct Blitter {
    pub counter_x: Counter,
    pub counter_y: Counter,

    pub inita: FlipFlop,
    pub initb: FlipFlop,
    pub running: FlipFlop,
    pub irq: FlipFlop,

    // pub running: Signal,
    pub _row_complete: Signal,
    pub _copy_done: Signal,
    pub _irq_out: Signal,
    pub irq_triggered: bool,

    pub src_x: CountUp,
    pub src_y: CountUp,
    pub dst_x: CountUp,
    pub dst_y: CountUp,
}

impl Blitter {
    pub fn default() -> Self {
        Self {
            counter_x: Counter { counter: 0, last_clock: Low },
            counter_y: Counter { counter: 0, last_clock: Low },

            inita: FlipFlop { last_clock: High, last_data: Low },
            initb: FlipFlop { last_clock: High, last_data: Low },
            running: FlipFlop { last_clock: High, last_data: Low },
            _row_complete: High,
            _copy_done: High,
            irq: FlipFlop { last_clock: High, last_data: Low },
            _irq_out: High,
            irq_triggered: false,
            src_x: CountUp { counter: 0, last_clock: Signal::Low },
            src_y: CountUp { counter: 0, last_clock: Signal::Low },
            dst_x: CountUp { counter: 0, last_clock: Signal::Low },
            dst_y: CountUp { counter: 0, last_clock: Signal::Low },
        }
    }

    pub fn irq_trigger(&mut self) -> bool {
        let r = self.irq_triggered;
        self.irq_triggered = false;
        r
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn cycle(&mut self, bus: &mut CpuBus) {
        let b = &mut bus.blitter;
        let flip_x = b.width & 0b1000_0000 != 0;
        let flip_y = b.height & 0b1000_0000 != 0;

        // if DMA_enable (blitter settings on) && blitter.start was written to
        let (bit_start, start_addressed) = b.start.read_once();
        let _start = !Signal::from(bus.system_control.dma_flags.dma_enable() && bit_start);

        for phase in 0..4 {
            let p0 = Signal::from(phase == 0);
            let p1 = Signal::from(phase == 1);
            let p2 = Signal::from(phase == 2);
            let p3 = Signal::from(phase == 3);

            let initb_data = self.inita.cycle(_start, Low, Low, !self.initb.val());
            let running_pre = p2.nand(self.initb.val());
            let _init = !self.initb.cycle(High, initb_data, p3, running_pre);

            let running = self.running.cycle(running_pre, Low, Low, self._copy_done);
            let _running = !running;

            let _xreload = _init.and(self._row_complete); // if not (init or row_complete)

            let src_x = self.src_x.cycle(_xreload, running, _init, p1, b.gx) as usize;
            let src_y = self.src_y.cycle(_init, running, !self._row_complete, p1, b.gy) as usize;
            let dst_x = self.dst_x.cycle(_xreload, running, _init, p1, b.vx) as usize & 0b0111_1111;
            let dst_y = self.dst_y.cycle(_init, running, !self._row_complete, p1, b.vy) as usize & 0b0111_1111;

            let x_pl = p2.nand(!self._row_complete);
            let x_te = _running;
            let pe = _init;
            self._row_complete = self.counter_x.cycle(p0, x_pl, x_te, pe, b.width);
            self._copy_done = self.counter_y.cycle(p1, High, self._row_complete, pe, b.height);

            let _irq_clear = !Signal::from(start_addressed);
            let try_irq = self.irq.cycle(self._copy_done, Low, Low, _irq_clear);
            self._irq_out = try_irq.nand(Signal::from(bus.system_control.dma_flags.dma_irq()));

            // trigger IRQ even when it's cleared on a 1x1 blit
            if self._irq_out == Low || _irq_clear == Low && start_addressed {
                self.irq_triggered = true;
            }

            if phase == 0 && running == High {
                let src_offset = match (src_x >= 128, src_y >= 128) {
                    (true, true) => 128 * 128 * 3,  // Bottom-right quadrant
                    (true, false) => 128 * 128,     // Top-right quadrant
                    (false, true) => 128 * 128 * 2, // Bottom-left quadrant
                    _ => 0,                         // Top-left quadrant
                };

                let local_x = src_x % 128;
                let local_y = src_y % 128;
                let src_address = src_offset + local_x + local_y * 128;
                let dst_address = dst_x + dst_y * 128;
                let src_bank = bus.system_control.banking_register.vram_page() as usize;
                let dest_fb = bus.system_control.banking_register.framebuffer() as usize;


                let color = if bus.system_control.dma_flags.dma_colorfill_enable() {
                    !b.color
                } else {
                    bus.vram_banks[src_bank][src_address]
                };

                if bus.system_control.dma_flags.dma_opaque() || color != 0 {
                    bus.framebuffers[dest_fb].borrow_mut()[dst_address] = color;
                }
            }
        }
    }
}
