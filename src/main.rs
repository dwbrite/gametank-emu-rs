#![feature(exclusive_range_pattern)]
#![allow(clippy::disallowed_methods, clippy::single_match)]

extern crate wee_alloc;

#[cfg(target_arch = "wasm32")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

mod color_map;
mod blitter;
mod gametank_bus;
mod helpers;

use std::rc::Rc;
use pixels::{PixelsBuilder, SurfaceTexture};
use w65c02s::W65C02S;
use winit::{
    event::{Event, WindowEvent},
    event_loop::EventLoop,
};

use winit::dpi::{LogicalSize};
use winit::event_loop::ControlFlow;

const WIDTH: u32 = 128;
const HEIGHT: u32 = 128;

#[cfg(target_arch = "wasm32")]
use web_sys::HtmlCanvasElement;

use winit::window::WindowBuilder;
use crate::blitter::Blitter;
use crate::color_map::COLOR_MAP;
pub use crate::gametank_bus::{Bus};
use crate::helpers::*;

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

pub fn main() {
    init();
}

#[cfg(target_arch = "wasm32")]
async fn wasm_init(canvas: Option<HtmlCanvasElement>) {
    use winit::platform::web::{WindowBuilderExtWebSys};
    let canv = canvas.clone().unwrap();
    let surface_size = LogicalSize::new(canv.width() as f64, canv.height() as f64);

    let builder = winit::window::WindowBuilder::new()
        .with_title("GameTank!")
        .with_inner_size(LogicalSize::new(WIDTH*2, HEIGHT*2))
        .with_min_inner_size(LogicalSize::new(WIDTH, HEIGHT))
        .with_canvas(canvas);

    run(builder, surface_size);
}

fn init() {
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

    let window = Rc::new(builder.build(&event_loop).unwrap());

    let mut pixels = {
        let surface_texture = SurfaceTexture::new(surface_size.width as u32, surface_size.height as u32, &window);
        let builder = PixelsBuilder::new(WIDTH, HEIGHT, surface_texture);

        futures::executor::block_on(builder.build_async()).expect("you were fucked from the start")
    };

    let mut bus = Bus::default();
    // let mut cpu = MOS6502::new_reset_position(&mut bus);
    let mut cpu = W65C02S::new();
    cpu.step(&mut bus); // take one initial step, to get through the reset vector

    let mut blitter = Blitter::default();

    log::info!("{:?}", bus);

    let mut last_cpu_tick = get_current_time();
    let cpu_frequency_hz = 3_579_545.0; // Precise frequency
    let ns_per_cycle = 1_000_000_000.0 / cpu_frequency_hz; // Nanoseconds per cycle

    let mut last_render_time = get_current_time();

    event_loop.run(move |event, elwt| {
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                println!("The close button was pressed; stopping");
                elwt.exit();
            },
            Event::AboutToWait => {
                let now = get_current_time();
                let elapsed_ms = now - last_cpu_tick;
                let elapsed_ns = elapsed_ms * 1000000.0;
                let cycles_to_emulate = (elapsed_ns / ns_per_cycle) as u64;

                // log::info!("{} ms elapsed", elapsed_ms);

                for _ in 0..cycles_to_emulate {
                    // print_next_instruction(&mut cpu, &mut bus);

                    blitter.cycle(&mut bus);
                    cpu.step(&mut bus);

                    cpu.set_irq(blitter.clear_irq_trigger());
                }

                if cycles_to_emulate > 0 {
                    last_cpu_tick = now;
                }

                if (now - last_render_time) >= 16.67 { // 16.67ms
                    last_render_time = now;
                    //
                    let fb = bus.read_full_framebuffer();
                    //
                    for (p, pixel) in pixels.frame_mut().chunks_exact_mut(4).enumerate() {
                        let color_index = fb[p]; // Get the 8-bit color index from the console's framebuffer
                        let (r, g, b, a) = COLOR_MAP[color_index as usize]; // Retrieve the corresponding RGBA color

                        // Map the color to the pixel's RGBA channels
                        pixel[0] = r; // R
                        pixel[1] = g; // G
                        pixel[2] = b; // B
                        pixel[3] = a; // A
                    }

                    // flip framebuffer, illegally
                    // bus.write_byte(0x2007, 0b0000_0010 ^ bus.system_control.dma_flags.0);

                    window.request_redraw();
                    if bus.system_control.dma_flags.dma_nmi() {
                        // cpu.non_maskable_interrupt_request();
                        cpu.set_nmi(bus.system_control.dma_flags.dma_nmi());
                    }
                    // println!("triggered nmi")
                }
            }
            Event::WindowEvent {
                event: WindowEvent::RedrawRequested,
                ..
            } => if let Err(_err) = pixels.render() {
                elwt.exit();
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

