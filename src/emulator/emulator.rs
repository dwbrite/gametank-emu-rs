use w65c02s::W65C02S;
use std::collections::HashMap;
use winit::keyboard::{Key, NamedKey, SmolStr};
use tracing::{debug, error, warn};
use w65c02s::State::AwaitingInterrupt;
use winit::event::ElementState;
use std::fmt::{Debug, Formatter};
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use bytemuck::bytes_of;
use crate::emulator::audio_output::GameTankAudio;
use crate::emulator::blitter::Blitter;
use crate::emulator::cartridges::CartridgeType;
use crate::emulator::gametank_bus::{AcpBus, Bus, CpuBus};
use crate::helpers::get_now_ms;
use crate::input::ControllerButton::{Down, Left, Right, Start, Up, A, B, C};
use crate::input::{ControllerButton, InputCommand, KeyState};
use crate::input::InputCommand::{Controller1, Controller2, HardReset, PlayPause, SoftReset};
use crate::input::KeyState::JustReleased;
use crate::PlayState;
use crate::PlayState::{Paused, Playing, WasmInit};

pub const WIDTH: u32 = 128;
pub const HEIGHT: u32 = 128;

pub struct Emulator {
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

    // TODO: move bindings out of emulator
    pub input_bindings: HashMap<Key, InputCommand>,
    pub input_state: HashMap<InputCommand, KeyState>
}

impl Emulator {
    pub(crate) fn load_rom(&mut self, bytes: &[u8]) {
        warn!("loading new rom from memory, size: {}", bytes.len());
        self.cpu_bus.cartridge = CartridgeType::from_slice(bytes);
        warn!(" - cartridge loaded from memory");
        self.cpu.reset();
        warn!(" - cpu reset");
        self.acp.reset();
        warn!(" - acp reset");
        self.blitter.clear_irq_trigger();
        warn!(" - blitter irq cleared");
    }
}

impl Debug for Emulator {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.debug_struct("Emulator")
            .field("cpu_bus", &self.cpu_bus)
            .field("acp_bus", &self.acp_bus)
            .field("cpu", &self.cpu)
            .field("acp", &self.acp)
            .field("blitter", &self.blitter)
            .field("clock_cycles_to_vblank", &self.clock_cycles_to_vblank)
            .field("last_emu_tick", &self.last_emu_tick);

        Ok(())
    }}

impl Emulator {
    pub fn wasm_init(&mut self) {
        if self.play_state == WasmInit {
            self.play_state = Playing;
            self.last_emu_tick = get_now_ms();
            self.last_render_time = get_now_ms();
        }
    }

    pub fn init() -> Self {
        let play_state = WasmInit;

        let mut bus = CpuBus::default();
        let mut cpu = W65C02S::new();
        cpu.step(&mut bus); // take one initial step, to get through the reset vector
        let acp = W65C02S::new();

        let blitter = Blitter::default();

        let last_cpu_tick_ms = get_now_ms();
        let cpu_frequency_hz = 3_579_545.0; // Precise frequency
        let cpu_ns_per_cycle = 1_000_000_000.0 / cpu_frequency_hz; // Nanoseconds per cycle

        let last_render_time = get_now_ms();


        // TODO: separation of concerns: input bindings are part of the app, not the emulator
        let mut input_bindings = HashMap::new();

        // controller 1
        input_bindings.insert(Key::Named(NamedKey::Enter), InputCommand::Controller1(Start));
        input_bindings.insert(Key::Named(NamedKey::ArrowLeft), InputCommand::Controller1(Left));
        input_bindings.insert(Key::Named(NamedKey::ArrowRight), InputCommand::Controller1(Right));
        input_bindings.insert(Key::Named(NamedKey::ArrowUp), InputCommand::Controller1(Up));
        input_bindings.insert(Key::Named(NamedKey::ArrowDown), InputCommand::Controller1(Down));
        input_bindings.insert(Key::Character(SmolStr::new("z")), InputCommand::Controller1(A));
        input_bindings.insert(Key::Character(SmolStr::new("x")), InputCommand::Controller1(B));
        input_bindings.insert(Key::Character(SmolStr::new("c")), InputCommand::Controller1(C));

        // controller 2
        input_bindings.insert(Key::Named(NamedKey::Space), InputCommand::Controller2(Start));
        input_bindings.insert(Key::Character(SmolStr::new("a")), InputCommand::Controller2(Left));
        input_bindings.insert(Key::Character(SmolStr::new("d")), InputCommand::Controller2(Right));
        input_bindings.insert(Key::Character(SmolStr::new("w")), InputCommand::Controller2(Up));
        input_bindings.insert(Key::Character(SmolStr::new("s")), InputCommand::Controller2(Down));
        input_bindings.insert(Key::Character(SmolStr::new("j")), InputCommand::Controller2(A));
        input_bindings.insert(Key::Character(SmolStr::new("k")), InputCommand::Controller2(B));
        input_bindings.insert(Key::Character(SmolStr::new("l")), InputCommand::Controller2(C));

        // emulator
        input_bindings.insert(Key::Character(SmolStr::new("r")), InputCommand::SoftReset);
        input_bindings.insert(Key::Character(SmolStr::new("R")), InputCommand::HardReset);
        input_bindings.insert(Key::Character(SmolStr::new("p")), InputCommand::PlayPause);

        Emulator {
            play_state,
            cpu_bus: bus,
            acp_bus: AcpBus::default(),
            cpu,
            acp,
            blitter,

            clock_cycles_to_vblank: 59659,
            last_emu_tick: last_cpu_tick_ms,
            cpu_frequency_hz,
            cpu_ns_per_cycle,
            last_render_time,
            audio_out: None,
            wait_counter: 0,

            input_bindings,
            input_state: Default::default(),
        }
    }

    pub fn process_cycles(&mut self, is_web: bool) {
        self.process_inputs();

        if self.play_state != Playing {
            return
        }

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

        while remaining_cycles > 0 {
            if self.cpu.get_state() == AwaitingInterrupt {
                self.wait_counter += 1;
                // get cpu's current asm code
            } else if self.wait_counter > 0 {
                // warn!("waited {} cycles", self.wait_counter);
                self.wait_counter = 0;
            }

            let _ = self.cpu.step(&mut self.cpu_bus);
            // clear interrupts after a step
            // self.cpu.set_nmi(false);
            // self.cpu.set_irq(false);

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
            // TODO: instant blit option

            let blit_irq = self.blitter.irq_trigger;
            if blit_irq {
                debug!("blit irq");
            }
            self.cpu.set_irq(blit_irq);

            self.clock_cycles_to_vblank -= cpu_cycles;
            if self.clock_cycles_to_vblank <= 0 {
                self.vblank();
            }
        }

        self.last_emu_tick = now_ms;

        if !is_web && (now_ms - self.last_render_time) >= 16.67 {
            debug!("time since last render: {}", now_ms - self.last_render_time);
            self.last_render_time = now_ms;
        }
    }

    fn run_acp(&mut self, acp_cycle_accumulator: &mut i32) {
        self.acp_bus.aram = self.cpu_bus.aram.take();

        if self.cpu_bus.system_control.clear_acp_reset() {
            self.acp.reset();
        }

        if self.cpu_bus.system_control.clear_acp_nmi() {
            self.acp.set_nmi(true);
        }

        while *acp_cycle_accumulator > 0 {
            let _ = self.acp.step(&mut self.acp_bus);
            *acp_cycle_accumulator -= self.acp_bus.clear_cycles() as i32;

            // clear stuff ig
            self.acp.set_irq(false);
            self.acp.set_nmi(false);

            if self.acp_bus.irq_counter <= 0 {
                self.acp_bus.irq_counter = self.cpu_bus.system_control.sample_rate() as i32 * 4;
                self.acp.set_irq(true);

                let sample_rate = self.cpu_frequency_hz / self.cpu_bus.system_control.sample_rate() as f64;
                // if audio_out is none or mismatched sample rate
                if self.audio_out.as_ref().map_or(true, |gta| gta.sample_rate != sample_rate) {
                    warn!("recreated audio stream with new sample rate: {:.3}Hz ({})", sample_rate, self.cpu_bus.system_control.sample_rate());
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
        self.clock_cycles_to_vblank += 59659;

        if self.cpu_bus.vblank_nmi_enabled() {
            self.cpu.set_nmi(true);
            debug!("vblanked");
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
                Controller2(button) => { self.set_gamepad_input(1, key, button); }
                PlayPause => {
                    if self.input_state[key] == JustReleased {
                        match self.play_state {
                            Paused => { self.play_state = Playing; }
                            Playing => { self.play_state = Paused; }
                            WasmInit => { self.play_state = Playing; }
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
            self.input_state.insert(*key, self.input_state[key].update());
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