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

use std::cmp::PartialEq;
use std::collections::HashMap;
use std::rc::Rc;
use pixels::{PixelsBuilder, SurfaceTexture};
use tracing::{debug, info, Level};
use w65c02s::W65C02S;
use winit::{event::{Event, WindowEvent}, event_loop::EventLoop, keyboard};

use winit::dpi::LogicalSize;
use winit::event_loop::ControlFlow;

const WIDTH: u32 = 128;
const HEIGHT: u32 = 128;

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

#[cfg(target_arch = "wasm32")]
use web_sys::{HtmlCanvasElement, window};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
use winit::event::{ElementState, KeyEvent, MouseButton};
use winit::event::ElementState::Pressed;
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey, SmolStr};

use winit::window::{WindowBuilder};
use emulator::Emulator;
use crate::blitter::Blitter;
use crate::emulator::ControllerButton::*;
use crate::emulator::InputCommand;
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
            .with_max_level(Level::INFO)
            .compact()
            .finish()
            .init();
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(start))]
#[cfg(target_arch = "wasm32")]
pub fn wasm_main() {
    use winit::platform::web::{WindowBuilderExtWebSys};

    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    setup_logging();
    info!("console logger started.");

    let window = window().expect("should have a Window");
    let document = window.document().expect("should have a Document");
    let canvas = document.get_element_by_id("gt-canvas").expect("should have a canvas element");
    let canvas: HtmlCanvasElement = canvas.dyn_into::<HtmlCanvasElement>().expect("failed to transmute canvas element");

    let canv = canvas.clone();
    let surface_size = LogicalSize::new(canv.width() as f64, canv.height() as f64);

    let builder = winit::window::WindowBuilder::new()
        .with_title("GameTank!")
        .with_inner_size(LogicalSize::new(WIDTH*2, HEIGHT*2))
        .with_min_inner_size(LogicalSize::new(WIDTH, HEIGHT))
        .with_canvas(Some(canvas));

    run(builder, surface_size);
}

pub fn main() {
    setup_logging();
    info!("stdout logger started");

    let surface_size = LogicalSize::new((WIDTH*2) as f64, (HEIGHT*2) as f64);

    let builder = winit::window::WindowBuilder::new()
        .with_title("GameTank!")
        .with_inner_size(LogicalSize::new(WIDTH*2, HEIGHT*2))
        .with_min_inner_size(LogicalSize::new(WIDTH, HEIGHT));

    run(builder, surface_size);
}



fn run(builder: WindowBuilder, surface_size: LogicalSize<f64>) {
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);

    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::closure::Closure;
        use wasm_bindgen::JsCast;
        use web_sys::window;
        use std::cell::RefCell;
        use std::rc::Rc;

        let event_loop_proxy = event_loop.create_proxy();

        let f = Rc::new(RefCell::new(None::<Closure<dyn FnMut()>>));
        let g = f.clone();

        *g.borrow_mut() = Some(Closure::wrap(Box::new(move || {
            event_loop_proxy.send_event(()).unwrap();

            window()
                .unwrap()
                .request_animation_frame(
                    f.borrow().as_ref().unwrap().as_ref().unchecked_ref(),
                )
                .expect("Failed to request animation frame");
        }) as Box<dyn FnMut()>));

        window()
            .unwrap()
            .request_animation_frame(
                g.borrow().as_ref().unwrap().as_ref().unchecked_ref(),
            )
            .expect("Failed to request animation frame");


        event_loop.set_control_flow(ControlFlow::Wait);
    }



    let window = Rc::new(builder.build(&event_loop).unwrap());

    let pixels = {
        let surface_texture = SurfaceTexture::new(surface_size.width as u32, surface_size.height as u32, &window);
        let builder = PixelsBuilder::new(WIDTH, HEIGHT, surface_texture);

        futures::executor::block_on(builder.build_async()).expect("you were fucked from the start")
    };

    let mut bus = CpuBus::default();
    // let mut cpu = MOS6502::new_reset_position(&mut bus);
    let mut cpu = W65C02S::new();
    cpu.step(&mut bus); // take one initial step, to get through the reset vector
    let acp = W65C02S::new();

    let blitter = Blitter::default();

    let last_cpu_tick_ms = get_now_ms();
    let cpu_frequency_hz = 3_579_545.0; // Precise frequency
    let cpu_ns_per_cycle = 1_000_000_000.0 / cpu_frequency_hz; // Nanoseconds per cycle

    let last_render_time = get_now_ms();

    let mut audio_out = None;

    // let sine_wave = rate(sample_rate).const_hz(60.0).sine();

    let mut play_state = Playing;

    let mut input_bindings = HashMap::new();
    input_bindings.insert(Key::Named(NamedKey::Enter), InputCommand::Controller1(Start));
    input_bindings.insert(Key::Named(NamedKey::ArrowLeft), InputCommand::Controller1(Left));
    input_bindings.insert(Key::Named(NamedKey::ArrowRight), InputCommand::Controller1(Right));
    input_bindings.insert(Key::Named(NamedKey::ArrowUp), InputCommand::Controller1(Up));
    input_bindings.insert(Key::Named(NamedKey::ArrowDown), InputCommand::Controller1(Down));
    input_bindings.insert(Key::Character(SmolStr::new("z")), InputCommand::Controller1(A));
    input_bindings.insert(Key::Character(SmolStr::new("x")), InputCommand::Controller1(B));
    input_bindings.insert(Key::Character(SmolStr::new("c")), InputCommand::Controller1(C));


    #[cfg(target_arch = "wasm32")]
    {
        play_state = WasmInit;
        audio_out = None;
    }

    let mut emu = Emulator {
        play_state,
        window,
        pixels,
        cpu_bus: bus,
        acp_bus: AcpBus::default(),
        cpu,
        acp,
        blitter,

        clock_cycles_to_vblank: 4656,
        last_emu_tick: last_cpu_tick_ms,
        cpu_frequency_hz,
        cpu_ns_per_cycle,
        last_render_time,
        audio_out,
        wait_counter: 0,
        // _sine_wave: sine_wave,
        input_bindings,
        input_state: Default::default(),
    };


    // debug!(target: "bus_init", "{:?}", emu.cpu_bus);

    event_loop.run(move |event, elwt| match event {
        Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } => {
            info!("The close button was pressed; stopping");
            elwt.exit();
        },
        Event::UserEvent(()) => {
            if emu.play_state == Playing {
                emu.process_cycles(true);
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        Event::AboutToWait => {
            if emu.play_state == Playing {
                emu.process_cycles(false);
            }
        }
        Event::WindowEvent { event: WindowEvent::MouseInput { button, .. }, .. } => match button {
            MouseButton::Left => if emu.play_state == WasmInit {
                emu.play_state = Playing;
                emu.last_emu_tick = get_now_ms();
                emu.last_render_time = get_now_ms();
            },
            _ => {}
        },
        Event::WindowEvent { event: WindowEvent::KeyboardInput { event, .. }, .. } => {
            // TODO: handle input
            let KeyEvent { physical_key, logical_key, text, location, state, repeat, .. } = event;
            emu.set_input_state(logical_key, state);

            // if emu.play_state == WasmInit {
            //     emu.play_state = Playing;
            //     emu.last_emu_tick = get_now_ms();
            //     emu.last_render_time = get_now_ms();
            // }
            //
            // match physical_key {
            //     PhysicalKey::Code(KeyCode::KeyP) => {
            //         if state == Pressed && !repeat {
            //             emu.play_state = match emu.play_state {
            //                 WasmInit => {
            //                     emu.last_emu_tick = get_now_ms();
            //                     emu.last_render_time = get_now_ms();
            //                     Playing
            //                 }
            //                 Paused => {
            //                     emu.last_emu_tick = get_now_ms();
            //                     emu.last_render_time = get_now_ms();
            //                     Playing
            //                 }
            //                 Playing => { Paused }
            //             };
            //         }
            //     }
            //     PhysicalKey::Code(_) => {}
            //     PhysicalKey::Unidentified(_) => {}
            // }
        },
        Event::WindowEvent {
            event: WindowEvent::RedrawRequested,
            ..
        } => if let Err(_err) = emu.pixels.render() {
            elwt.exit();
        },
        Event::WindowEvent {
            event: WindowEvent::Resized(size),
            ..
        } => {
            // explicity ignore resize failures?
            let _ = emu.pixels.resize_surface(size.width, size.height);
        }

        _ => {},
    }).expect("Something went wrong :(");
}