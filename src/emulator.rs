use std::collections::hash_map::Keys;
use std::collections::HashMap;
use tracing::{debug, error, info, warn};
use std::rc::Rc;
use winit::window::Window;
use pixels::Pixels;
use w65c02s::State::AwaitingInterrupt;
use w65c02s::W65C02S;
use winit::event::ElementState;
use winit::keyboard::Key;
use crate::audio_output::GameTankAudio;
use crate::blitter::Blitter;
use crate::{Bus, PlayState};
use crate::color_map::COLOR_MAP;
use crate::emulator::ControllerButton::*;
use crate::emulator::InputCommand::*;
use crate::emulator::KeyState::{Held, JustPressed, JustReleased, Released};
use crate::gamepad::GamePad;
use crate::gametank_bus::{AcpBus, CpuBus};
use crate::helpers::get_now_ms;
use crate::PlayState::{Paused, Playing, WasmInit};

#[derive(Copy, Clone)]
#[derive(Eq, Hash, PartialEq)]
pub enum ControllerButton {
    Up,
    Down,
    Left,
    Right,
    B,
    A,
    Start,
    C,
}

#[derive(Copy, Clone)]
#[derive(Eq, Hash, PartialEq)]
pub enum InputCommand {
    Controller1(ControllerButton),
    Controller2(ControllerButton),
    PlayPause,
    SoftReset,
    HardReset,
}


#[derive(Copy, Clone)]
#[derive(Eq, Hash, PartialEq)]
pub enum KeyState {
    JustPressed,
    Held,
    JustReleased,
    Released
}

impl KeyState {
    fn is_pressed(&self) -> bool {
        match self {
            JustPressed => { true }
            Held => { true }
            JustReleased => { false }
            Released => { false }
        }
    }

    fn new(pressed: bool) -> Self {
        if pressed {
            return JustPressed
        }
        Released
    }

    fn update_state(&self, pressed: bool) -> Self {
        if pressed {
            return match self {
                JustPressed => { Held }
                Held => { Held }
                JustReleased => { JustPressed }
                Released => { JustPressed }
            }
        }
        match self {
            JustPressed => { JustReleased }
            Held => { JustReleased }
            JustReleased => { Released }
            Released => { Released }
        }
    }
}

pub struct Emulator {
    pub window: Rc<Window>,
    pub pixels: Pixels,

    pub cpu_bus: CpuBus,
    pub acp_bus: AcpBus,
    pub cpu: W65C02S,
    pub acp: W65C02S,

    pub blitter: Blitter,

    pub clock_cycles_to_vblank: i32,

    pub last_emu_tick: f64,
    pub cpu_ns_per_cycle: f64,
    pub cpu_frequency_hz: f64,
    pub last_render_time: f64,
    pub audio_out: Option<GameTankAudio>,
    pub play_state: PlayState,
    pub wait_counter: u64,

    pub input_bindings: HashMap<Key, InputCommand>,
    pub input_state: HashMap<InputCommand, KeyState>
}

impl Emulator {
    pub fn process_cycles(&mut self, is_web: bool) {
        // web redraw
        let now_ms = get_now_ms();
        let mut elapsed_ms = now_ms - self.last_emu_tick;

        if elapsed_ms > 33.0 {
            warn!("emulator took more than 33ms to process cycles");
            elapsed_ms = 16.667;
        }

        let elapsed_ns = elapsed_ms * 1000000.0;
        let mut remaining_cycles: i32 = (elapsed_ns / self.cpu_ns_per_cycle) as i32;

        let mut acp_cycle_accumulator = 0;

        self.process_inputs();

        while remaining_cycles > 0 {
            let op_addr = self.cpu.get_pc();
            let op = self.cpu_bus.read_byte(op_addr);

            if self.cpu.get_state() == AwaitingInterrupt {
                self.wait_counter += 1;
            } else if self.wait_counter > 0 {
                // info!("waited {} cycles", self.wait_counter);
                self.wait_counter = 0;
            }


            let _ = self.cpu.step(&mut self.cpu_bus);
            // clear interrupts after a step
            self.cpu.set_nmi(false);
            self.cpu.set_irq(false);

            let cpu_cycles = self.cpu_bus.clear_cycles() as i32;
            remaining_cycles -= cpu_cycles;

            acp_cycle_accumulator += cpu_cycles * 4;

            // pass aram to acp
            if self.cpu_bus.system_control.acp_enabled() {
                self.run_acp(&mut acp_cycle_accumulator);
            }

            // blit
            for _ in 0..cpu_cycles {
                self.blitter.cycle(&mut self.cpu_bus);
            }
            // self.blitter.instant_blit(&mut self.cpu_bus);

            let blit_irq = self.blitter.clear_irq_trigger();
            self.cpu.set_irq(blit_irq);
            if blit_irq {
                // info!("blit irq triggered");
            }


            self.clock_cycles_to_vblank -= cpu_cycles;
            if self.clock_cycles_to_vblank <= 0 {
                self.vblank();
            }
        }

        self.last_emu_tick = now_ms;

        if is_web || (now_ms - self.last_render_time) >= 16.67 {
            debug!("time since last render: {}", now_ms - self.last_render_time);
            self.last_render_time = now_ms;

            self.window.request_redraw();
        }
    }

    fn run_acp(&mut self, acp_cycle_accumulator: &mut i32) {
        self.acp_bus.aram = self.cpu_bus.aram.take();
        while *acp_cycle_accumulator > 0 {
            let _ = self.acp.step(&mut self.acp_bus);
            *acp_cycle_accumulator -= self.acp_bus.clear_cycles() as i32;

            // clear irq
            self.acp.set_irq(false);

            if self.acp_bus.irq_counter <= 0 {
                self.acp_bus.irq_counter = self.cpu_bus.system_control.sample_rate() as i32 * 4;
                self.acp.set_irq(true);

                let sample_rate = self.cpu_frequency_hz / self.cpu_bus.system_control.sample_rate() as f64;
                // if audio_out is none or mismatched sample rate
                if self.audio_out.as_ref().map_or(true, |gta| gta.sample_rate != sample_rate) {
                    info!("recreated audio stream with new sample rate: {:.3}Hz ({})", sample_rate, self.cpu_bus.system_control.sample_rate());
                    self.audio_out = Some(GameTankAudio::new(sample_rate, 48000.0));
                }

                if let Some(audio) = &mut self.audio_out {
                    let next_sample_u8 = self.acp_bus.sample;
                    if let Err(e) = audio.producer.push(next_sample_u8) {
                        error!("not enough slots in audio producer: {e}");
                    }
                }

                if let Some(audio) = &mut self.audio_out {
                    audio.convert_to_output_buffers();
                    audio.process_audio();
                }
            }
        }
        self.cpu_bus.aram = self.acp_bus.aram.take();
    }

    fn vblank(&mut self) {
        // info!("vblank");
        self.clock_cycles_to_vblank += 59659;

        if self.cpu_bus.vblank_nmi_enabled() {
            self.cpu.set_nmi(true);
        }

        let fb = self.cpu_bus.read_full_framebuffer();

        for (p, pixel) in self.pixels.frame_mut().chunks_exact_mut(4).enumerate() {
            let color_index = fb[p]; // Get the 8-bit color index from the console's framebuffer
            let (r, g, b, a) = COLOR_MAP[color_index as usize]; // Retrieve the corresponding RGBA color

            // Map the color to the pixel's RGBA channels
            pixel[0] = r; // R
            pixel[1] = g; // G
            pixel[2] = b; // B
            pixel[3] = a; // A
        }
    }

    pub fn set_input_state(&mut self, key: Key, state: ElementState) {
        if let Some(command) = self.input_bindings.get(&key) {
            if let Some(ks) = self.input_state.get(command) {
                self.input_state.insert(*command, ks.update_state(state.is_pressed()));
            } else {
                self.input_state.insert(*command, KeyState::new(state.is_pressed()));
            }
        }
    }

    fn process_inputs(&mut self) {
        let keys: Vec<_> = self.input_state.keys().cloned().collect();  // Clone keys to avoid borrowing conflicts

        if keys.len() > 0 && self.play_state == WasmInit {
            self.play_state = Playing;
        }

        for key in &keys {
            match key {
                Controller1(button) => { self.set_gamepad_input(0, key, button); }
                Controller2(button) => { self.set_gamepad_input(0, key, button); }
                PlayPause => {
                    if self.input_state[key] == JustReleased {
                        match self.play_state {
                            Paused => { self.play_state = Playing; }
                            Playing => { self.play_state = Paused; }
                            WasmInit => {}
                        }
                    }
                }
                SoftReset => {
                    // TODO
                }
                HardReset => {
                    // TODO
                }
            }
        }
    }
    fn set_gamepad_input(&mut self, gamepad: usize, key: &InputCommand, button: &ControllerButton) {
        let gamepad = &mut self.cpu_bus.system_control.gamepads[gamepad];
        match button {
            Up =>     { gamepad.up    = self.input_state[&key].is_pressed(); }
            Down =>   { gamepad.down  = self.input_state[&key].is_pressed(); }
            Left =>   { gamepad.left  = self.input_state[&key].is_pressed(); }
            Right =>  { gamepad.right = self.input_state[&key].is_pressed(); }
            B =>      { gamepad.b     = self.input_state[&key].is_pressed(); }
            A =>      { gamepad.a     = self.input_state[&key].is_pressed(); }
            Start =>  { gamepad.start = self.input_state[&key].is_pressed(); }
            C =>      { gamepad.c     = self.input_state[&key].is_pressed(); }
        }
    }
}


