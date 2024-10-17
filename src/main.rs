#![feature(exclusive_range_pattern)]
#![allow(clippy::disallowed_methods, clippy::single_match)]

mod color_map;
mod blitter;
mod gametank_bus;
mod helpers;
mod audio_output;
mod emulator;
mod cartridges;
mod gamepad;
mod input;

use std::cmp::PartialEq;
use std::collections::HashMap;
use std::panic;
use std::rc::Rc;
use std::sync::Arc;
use std::thread::yield_now;
use pixels::{Pixels, PixelsBuilder, SurfaceTexture};
use tracing::{error, info, warn, Level};
use tracing_subscriber::layer::SubscriberExt;
use w65c02s::W65C02S;
use winit::{event::{Event, WindowEvent}, event_loop::EventLoop};

use winit::dpi::LogicalSize;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoopBuilder};

const WIDTH: u32 = 128;
const HEIGHT: u32 = 128;

use tracing_subscriber::util::SubscriberInitExt;

#[cfg(target_arch = "wasm32")]
use web_sys::{HtmlCanvasElement, window};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
use winit::application::ApplicationHandler;
use winit::event::{KeyEvent, MouseButton};
use winit::keyboard::{Key, NamedKey, SmolStr};
use winit::window::{Window, WindowAttributes, WindowId};
use emulator::Emulator;
use EmulatorEvent::LogicTick;
use crate::blitter::Blitter;
use input::ControllerButton::*;
use input::InputCommand;
use crate::EmulatorEvent::Redraw;
pub use crate::gametank_bus::Bus;
use crate::gametank_bus::{AcpBus, CpuBus};
use crate::helpers::*;
use crate::PlayState::*;

#[derive(Copy, Clone, Debug, PartialEq)]
enum PlayState {
    WasmInit,
    Paused,
    Playing,
}
impl <'win> Emulator<'win> {
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
        // TODO:

        // emulator
        input_bindings.insert(Key::Character(SmolStr::new("r")), InputCommand::SoftReset);
        input_bindings.insert(Key::Character(SmolStr::new("R")), InputCommand::HardReset);
        input_bindings.insert(Key::Character(SmolStr::new("p")), InputCommand::PlayPause);

        Emulator {
            play_state,
            window: None,
            pixels: None,
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
}

impl <'win> ApplicationHandler<EmulatorEvent> for Emulator<'win> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            info!("initializing...");
            let mut window_attributes = WindowAttributes::default()
                .with_title("GameTank!")
                .with_inner_size(LogicalSize::new(WIDTH*2, HEIGHT*2))
                .with_min_inner_size(LogicalSize::new(WIDTH, HEIGHT));

            #[cfg(target_arch = "wasm32")] {
                use winit::platform::web::{EventLoopExtWebSys, WindowAttributesExtWebSys};

                let window = window().expect("should have a Window");
                let document = window.document().expect("should have a Document");
                let canvas = document.get_element_by_id("gt-canvas").expect("should have a canvas element");
                let canvas: HtmlCanvasElement = canvas.dyn_into::<HtmlCanvasElement>().expect("failed to transmute canvas element");

                window_attributes = window_attributes.with_canvas(Some(canvas));
                info!("found canvas");
            }

            let window = Arc::new(event_loop.create_window(window_attributes).expect("failed to create window"));
            self.window = Some(window.clone());


            if let Some(window) = &mut self.window {
                let size = window.inner_size();
                if self.pixels.is_none() && size.width > 0 && size.height > 0 {
                    let pixels = {
                        let surface_texture = pixels::SurfaceTexture::new(size.width, size.height, window.clone());
                        futures::executor::block_on(
                            Pixels::new_async(WIDTH, HEIGHT, surface_texture)
                        ).expect("you were fucked from the start")
                    };
                    self.pixels = Some(pixels);
                }
            }
            info!("done initializing");
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, e: EmulatorEvent) {
        match e {
            LogicTick => {
                self.process_cycles(true);
            }
            Redraw => {
                if let Some(window) = &mut self.window {
                    window.request_redraw();
                } else {
                    error!("no window found on redraw event")
                }
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
        info!("processing window event");
        match event {
            WindowEvent::Resized(size) => {
                if let Some(window) = &mut self.window {
                    if self.pixels.is_none() && size.width > 0 && size.height > 0 {
                        info!("it was a resize event, and we have no pixels!");
                        let pixels = {
                            let surface_texture = pixels::SurfaceTexture::new(size.width, size.height, window.clone());
                            futures::executor::block_on(Pixels::new_async(WIDTH, HEIGHT, surface_texture)).expect("you were fucked from the start")
                        };
                        self.pixels = Some(pixels);
                    }
                }

                if let Some(pixels) = &mut self.pixels {
                    let _ = pixels.resize_surface(size.width, size.height);
                } else {
                    error!("can't resize non-existent pixels :)))")
                }
            }
            WindowEvent::CloseRequested => {
                info!("The close button was pressed; stopping");
                event_loop.exit();
            }
            WindowEvent::Destroyed => {}
            WindowEvent::DroppedFile(_) => {} // TODO: roms

            // WindowEvent::HoveredFile(_) => {}
            // WindowEvent::HoveredFileCancelled => {}
            // WindowEvent::Focused(_) => {}

            WindowEvent::KeyboardInput { event, .. } => {
                let KeyEvent {  logical_key,   state,  .. } = event;
                self.set_input_state(logical_key, state);
            },
            // WindowEvent::ModifiersChanged(_) => {}
            // WindowEvent::MouseWheel { .. } => {} // future stuffs
            WindowEvent::MouseInput { .. } => { self.wasm_init(); }
            WindowEvent::RedrawRequested => {
                if let Some(pixels) = &mut self.pixels {
                    pixels.render().expect("error rendering pixels");
                } else {
                    error!("can't render without pixels!");
                }

                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::Touch(_) => { self.wasm_init(); }
            _ => {}
        }
        info!("done ^");
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        info!("about to wait; processing");
        self.process_cycles(false);
    }
}

impl<'win> Emulator<'win> {
    fn wasm_init(&mut self) {
        if self.play_state == WasmInit {
            self.play_state = Playing;
            self.last_emu_tick = get_now_ms();
            self.last_render_time = get_now_ms();
        }
    }
}

fn setup_logging() {
    #[cfg(target_arch = "wasm32")]
    {

        use tracing_wasm::{WASMLayer, WASMLayerConfigBuilder};

        // Set up the WASM layer for tracing logs
        let wlconfig = WASMLayerConfigBuilder::new()
            .set_max_level(Level::INFO).build();

        let wasm_layer = WASMLayer::new(wlconfig);
        // Configure the subscriber with the WASM layer
        tracing_subscriber::registry()
            .with(wasm_layer)
            .init();
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        tracing_subscriber::fmt()
            .with_max_level(Level::WARN)
            .compact()
            .finish()
            .init();
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(start))]
#[cfg(target_arch = "wasm32")]
pub fn wasm_main() {
    use winit::platform::web::{EventLoopExtWebSys, WindowAttributesExtWebSys};
    panic::set_hook(Box::new(console_error_panic_hook::hook));
    setup_logging();
    info!("console logger started.");

    let event_loop = EventLoop::<EmulatorEvent>::with_user_event().build().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = Emulator::init();

    event_loop.spawn_app(app);
}

pub fn main() {
    setup_logging();
    info!("stdout logger started");

    let event_loop = EventLoop::<EmulatorEvent>::with_user_event().build().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = Emulator::init();

    app.play_state = Playing;

    let _ = event_loop.run_app(&mut app);
}

#[derive(Debug, Copy, Clone)]
enum EmulatorEvent {
    LogicTick,
    Redraw,
}
