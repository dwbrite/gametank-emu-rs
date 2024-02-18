#![allow(clippy::disallowed_methods, clippy::single_match)]

use std::num::NonZeroU32;
use std::rc::Rc;
use pixels::{Pixels, SurfaceTexture};
use winit::{
    event::{ElementState, Event, KeyEvent, WindowEvent},
    event_loop::EventLoop,
    keyboard::Key,
    window::{Fullscreen, Window},
};

use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event_loop::ControlFlow;
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};                 //added futures = "0.3" in cargo.toml dependencies
//
// use softbuffer::{Buffer, NoDisplayHandle, NoWindowHandle, Surface};

const WIDTH: u32 = 128;
const HEIGHT: u32 = 128;


// struct SystemInterface {
//     // buffer: softbuffer::Buffer<'a, D, W>,
// }

struct GameTankMemory {
    framebuffers: [[u8; (WIDTH*HEIGHT) as usize]; 2],
    ram_banks: [[u8; 0x2000]; 4],
    scr: [u8; 8],
    gamepads: [u8; 2],
    via: [u8; 0x10],
    audio_ram: [u8; 0x1000],
    vram_banks: [[[u8; 128*128]; 4]; 8]
}

#[cfg(target_arch = "wasm32")]
use web_sys::HtmlCanvasElement;
use winit::window::WindowBuilder;

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(start))]
#[cfg(target_arch = "wasm32")]
pub fn initialize() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    console_log::init().unwrap();
    log::info!("RRR loaded.");
}


#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
#[cfg(target_arch = "wasm32")]
pub fn play(canvas: Option<HtmlCanvasElement>) {
    wasm_bindgen_futures::spawn_local(wasm_init(canvas));
}





#[cfg(target_arch = "wasm32")]
async fn wasm_init(canvas: Option<HtmlCanvasElement>) {
    use winit::platform::web::{WindowBuilderExtWebSys, WindowExtWebSys};
    let canv = canvas.clone().unwrap();
    let surface_size = LogicalSize::new(canv.width() as f64, canv.height() as f64);

    let builder = winit::window::WindowBuilder::new()
        .with_title("GameTank!")
        .with_inner_size(LogicalSize::new(WIDTH*2, HEIGHT*2))
        .with_min_inner_size(LogicalSize::new(WIDTH, HEIGHT))
        .with_canvas(canvas);

    run(builder, surface_size).await;
}

async fn init() {
    let surface_size = LogicalSize::new((WIDTH*2) as f64, (HEIGHT*2) as f64);

    let builder = winit::window::WindowBuilder::new()
        .with_title("GameTank!")
        .with_inner_size(LogicalSize::new(WIDTH*2, HEIGHT*2))
        .with_min_inner_size(LogicalSize::new(WIDTH, HEIGHT));

    run(builder, surface_size).await;
}

async fn run(builder: WindowBuilder, surface_size: LogicalSize<f64>) {
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);

    let window = Rc::new(builder.build(&event_loop).unwrap());

    let mut pixels = {
        let surface_texture = SurfaceTexture::new(surface_size.width as u32, surface_size.height as u32, &window);
        Pixels::new_async(WIDTH, HEIGHT, surface_texture).await.expect("you were fucked from the start")
    };


    event_loop.run(move |event, elwt| {
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                println!("The close button was pressed; stopping");
                elwt.exit();
            },
            Event::WindowEvent {
                event: WindowEvent::RedrawRequested,
                ..
            } => {
                // Redraw the application.
                for (p, i) in pixels.frame_mut().iter_mut().enumerate() {
                    *i = p as u8;
                }

                if let Err(_err) = pixels.render() {
                    elwt.exit();
                    return;
                }
            },
            Event::WindowEvent {
                event: WindowEvent::Resized(size),
                ..
            } => {
                // explicity ignore resize failures?
                let _ = pixels.resize_surface(size.width, size.height);
            }

            _ => (),
        }
    }).expect("Something went wrong :(");
}

pub fn main() {
    futures::executor::block_on(init());
}
