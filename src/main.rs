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
mod app;
mod egui_renderer;
mod graphics;

use std::cmp::PartialEq;
use tracing::{info, Level};
use winit::event_loop::EventLoop;

use winit::event_loop::ControlFlow;

const WIDTH: u32 = 128;
const HEIGHT: u32 = 128;

use tracing_subscriber::util::SubscriberInitExt;

#[cfg(target_arch = "wasm32")]
use web_sys::{window, HtmlCanvasElement};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
use std::future::Future;
use crate::app::App;
pub use crate::gametank_bus::Bus;
use crate::PlayState::*;

//
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
        use tracing_subscriber::layer::SubscriberExt;

        // Set up the WASM layer for tracing logs
        let wlconfig = WASMLayerConfigBuilder::new()
            .set_max_level(Level::WARN).build();

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
    use std::panic;
    use winit::platform::web::{EventLoopExtWebSys, WindowAttributesExtWebSys};
    panic::set_hook(Box::new(console_error_panic_hook::hook));
    setup_logging();
    info!("console logger started.");

    let event_loop = EventLoop::<()>::with_user_event().build().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);

    let app = App::new();

    let _ = event_loop.spawn_app(app);
}

pub fn main() {
    setup_logging();
    info!("stdout logger started");

    let event_loop = EventLoop::<()>::with_user_event().build().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::new();
    app.emulator.play_state = Playing;

    let _ = event_loop.run_app(&mut app);
}

pub fn spawn<F>(future: F)
where
    F: Future<Output = ()> + 'static,
{
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_futures::spawn_local(future);
    #[cfg(not(target_arch = "wasm32"))]
    pollster::block_on(future)
}