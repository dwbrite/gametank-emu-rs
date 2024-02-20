#![feature(exclusive_range_pattern)]
#![allow(clippy::disallowed_methods, clippy::single_match)]

extern crate wee_alloc;

// Use `wee_alloc` as the global allocator.
//
#[cfg(target_arch = "wasm32")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

mod gametank;
mod color_map;
mod blitter;

use std::rc::Rc;
use std::time::{Instant};
// use emulator_6502::MOS6502;
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
use crate::gametank::{Bus};

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


#[cfg(not(target_arch = "wasm32"))]
static mut START_INSTANT: Option<Instant> = None;

fn get_current_time() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        let window = web_sys::window().expect("should have a window in this context");
        let performance = window
            .performance()
            .expect("performance should be available");

        return performance.now()
    }

    #[cfg(not(target_arch = "wasm32"))]
    unsafe {
        if START_INSTANT.is_none() {
            START_INSTANT = Some(Instant::now());
        }
        return START_INSTANT.unwrap().elapsed().as_secs_f64() * 1000.0;
    }
}



fn run(builder: WindowBuilder, surface_size: LogicalSize<f64>) {
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);

    let window = Rc::new(builder.build(&event_loop).unwrap());

    let mut pixels = {
        let surface_texture = SurfaceTexture::new(surface_size.width as u32, surface_size.height as u32, &window);
        let builder = PixelsBuilder::new(WIDTH, HEIGHT, surface_texture);

        // let surface_texture = SurfaceTexture::new(surface_size.width as u32, surface_size.height as u32, &window);
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
                    cpu.set_nmi(bus.system_control.dma_flags.dma_nmi());
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


fn _print_next_instruction(cpu: &mut W65C02S, bus: &mut Bus) {
    let program_counter = cpu.get_pc();
    let opcode = bus.read_byte(program_counter);
    let (op, length) = _opcode_to_instruction_length(opcode);

    print!("${:04X} - {:02X} | {}", program_counter, opcode, op);
    for idx in 1..length as u16 {
        let val = bus.read_byte(program_counter + idx);
        print!(" {:02X}", val);
    }
    println!();
}

fn _opcode_to_instruction_length(opcode: u8) -> (&'static str, usize) {
    match opcode {
        0x00 => ("brk",      1),
        0x01 => ("ora",      2),
        0x02 => ("nop",      1),
        0x03 => ("nop",      1),
        0x04 => ("tsb",      2),
        0x05 => ("ora",      2),
        0x06 => ("asl",      2),
        0x07 => ("rmb",      2),
        0x08 => ("php",      1),
        0x09 => ("ora",      2),
        0x0A => ("asl",      2),
        0x0B => ("nop",      1),
        0x0C => ("tsb",      3),
        0x0D => ("ora",      3),
        0x0E => ("asl",      3),
        0x0F => ("bbr",      3),
        0x10 => ("branch",   2),
        0x11 => ("ora",      2),
        0x12 => ("ora",      2),
        0x13 => ("nop",      1),
        0x14 => ("trb",      2),
        0x15 => ("ora",      2),
        0x16 => ("asl",      2),
        0x17 => ("rmb",      2),
        0x18 => ("clc",      1),
        0x19 => ("ora",      3),
        0x1A => ("inca",     1),
        0x1B => ("nop",      1),
        0x1C => ("trb",      3),
        0x1D => ("ora",      3),
        0x1E => ("asl",      3),
        0x1F => ("bbr",      3),
        0x20 => ("jsr",      1),
        0x21 => ("and",      2),
        0x22 => ("nop",      1),
        0x23 => ("nop",      1),
        0x24 => ("bit",      2),
        0x25 => ("and",      2),
        0x26 => ("rol",      2),
        0x27 => ("rmb",      2),
        0x28 => ("plp",      1),
        0x29 => ("and",      2),
        0x2A => ("rol",      1),
        0x2B => ("nop",      1),
        0x2C => ("bit",      3),
        0x2D => ("and",      3),
        0x2E => ("rol",      3),
        0x2F => ("bbr",      3),
        0x30 => ("branch",   2),
        0x31 => ("and",      2),
        0x32 => ("and",      2),
        0x33 => ("nop",      1),
        0x34 => ("bit",      2),
        0x35 => ("and",      2),
        0x36 => ("rol",      2),
        0x37 => ("rmb",      2),
        0x38 => ("sec",      1),
        0x39 => ("and",      3),
        0x3A => ("dea",      1),
        0x3B => ("nop",      1),
        0x3C => ("bit",      3),
        0x3D => ("and",      3),
        0x3E => ("rol",      3),
        0x3F => ("bbr",      3),
        0x40 => ("rti",      1),
        0x41 => ("eor",      2),
        0x42 => ("nop",      1),
        0x43 => ("nop",      1),
        0x44 => ("nop",      2),
        0x45 => ("eor",      2),
        0x46 => ("lsr",      2),
        0x47 => ("rmb",      2),
        0x48 => ("pha",      1),
        0x49 => ("eor",      2),
        0x4A => ("lsr",      2),
        0x4B => ("nop",      1),
        0x4C => ("jmp",      3),
        0x4D => ("eor",      3),
        0x4E => ("lsr",      3),
        0x4F => ("bbr",      3),
        0x50 => ("branch",   2),
        0x51 => ("eor",      2),
        0x52 => ("eor",      2),
        0x53 => ("nop",      1),
        0x54 => ("nop",      2),
        0x55 => ("eor",      2),
        0x56 => ("lsr",      2),
        0x57 => ("rmb",      2),
        0x58 => ("cli",      1),
        0x59 => ("eor",      3),
        0x5A => ("phy",      1),
        0x5B => ("nop",      1),
        0x5C => ("nop_5c",   3),
        0x5D => ("eor",      3),
        0x5E => ("lsr",      3),
        0x5F => ("bbr",      3),
        0x60 => ("rts",      1),
        0x61 => ("adc",      2),
        0x62 => ("nop",      1),
        0x63 => ("nop",      1),
        0x64 => ("stz",      2),
        0x65 => ("adc",      2),
        0x66 => ("ror",      2),
        0x67 => ("rmb",      2),
        0x68 => ("pla",      1),
        0x69 => ("adc",      2),
        0x6A => ("rora",     1),
        0x6B => ("nop",      1),
        0x6C => ("jmp",      3),
        0x6D => ("adc",      3),
        0x6E => ("ror",      3),
        0x6F => ("bbr",      3),
        0x70 => ("branch",   2),
        0x71 => ("adc",      2),
        0x72 => ("adc",      2),
        0x73 => ("nop",      1),
        0x74 => ("stz",      2),
        0x75 => ("adc",      2),
        0x76 => ("ror",      2),
        0x77 => ("rmb",      2),
        0x78 => ("sei",      1),
        0x79 => ("adc",      3),
        0x7A => ("ply",      1),
        0x7B => ("nop",      1),
        0x7C => ("jmp",      3),
        0x7D => ("adc",      3),
        0x7E => ("ror",      3),
        0x7F => ("bbr",      3),
        0x80 => ("branch",   2),
        0x81 => ("sta",      2),
        0x82 => ("nop",      1),
        0x83 => ("nop",      1),
        0x84 => ("sty",      2),
        0x85 => ("sta",      2),
        0x86 => ("stx",      2),
        0x87 => ("smb",      2),
        0x88 => ("dec",      1),
        0x89 => ("bit_i",    2),
        0x8A => ("txa",      1),
        0x8B => ("nop",      1),
        0x8C => ("sty",      3),
        0x8D => ("sta",      3),
        0x8E => ("stx",      3),
        0x8F => ("bbs",      3),
        0x90 => ("branch" ,  2),
        0x91 => ("sta",      2),
        0x92 => ("sta",      2),
        0x93 => ("nop",      1),
        0x94 => ("sty",      2),
        0x95 => ("sta",      2),
        0x96 => ("stx",      2),
        0x97 => ("smb",      2),
        0x98 => ("tya",      1),
        0x99 => ("sta",      3),
        0x9A => ("txs",      1),
        0x9B => ("nop",      1),
        0x9C => ("stz",      3),
        0x9D => ("sta",      3),
        0x9E => ("stz",      3),
        0x9F => ("bbs",      3),
        0xA0 => ("ldy",      2),
        0xA1 => ("lda",      2),
        0xA2 => ("ldx",      2),
        0xA3 => ("nop",      1),
        0xA4 => ("ldy",      2),
        0xA5 => ("lda",      2),
        0xA6 => ("ldx",      2),
        0xA7 => ("smb",      2),
        0xA8 => ("tay",      1),
        0xA9 => ("lda",      2),
        0xAA => ("tax",      1),
        0xAB => ("nop",      1),
        0xAC => ("ldy",      3),
        0xAD => ("lda",      3),
        0xAE => ("ldx",      3),
        0xAF => ("bbs",      3),
        0xB0 => ("branch",   2),
        0xB1 => ("lda",      2),
        0xB2 => ("lda",      2),
        0xB3 => ("nop",      1),
        0xB4 => ("ldy",      2),
        0xB5 => ("lda",      2),
        0xB6 => ("ldx",      2),
        0xB7 => ("smb",      2),
        0xB8 => ("clv",      1),
        0xB9 => ("lda",      3),
        0xBA => ("tsx",      1),
        0xBB => ("nop",      1),
        0xBC => ("ldy",      3),
        0xBD => ("lda",      3),
        0xBE => ("ldx",      3),
        0xBF => ("bbs",      3),
        0xC0 => ("cpy",      2),
        0xC1 => ("cmp",      2),
        0xC2 => ("nop",      2),
        0xC3 => ("nop",      1),
        0xC4 => ("cpy",      2),
        0xC5 => ("cmp",      2),
        0xC6 => ("dec",      2),
        0xC7 => ("smb",      2),
        0xC8 => ("incy",      1),
        0xC9 => ("cmp",      2),
        0xCA => ("dec",      1),
        0xCB => ("wai",      1),
        0xCC => ("cpy",      3),
        0xCD => ("cmp",      3),
        0xCE => ("dec",      3),
        0xCF => ("bbs",      3),
        0xD0 => ("branch",   2),
        0xD1 => ("cmp",      2),
        0xD2 => ("cmp",      2),
        0xD3 => ("nop",      1),
        0xD4 => ("nop",      2),
        0xD5 => ("cmp",      2),
        0xD6 => ("dec",      2),
        0xD7 => ("smb",      2),
        0xD8 => ("cld",      1),
        0xD9 => ("cmp",      3),
        0xDA => ("phx",      1),
        0xDB => ("stp",      1),
        0xDC => ("nop",      3),
        0xDD => ("cmp",      3),
        0xDE => ("dec",      3),
        0xDF => ("bbs",      3),
        0xE0 => ("cpx",      2),
        0xE1 => ("sbc",      2),
        0xE2 => ("nop",      2),
        0xE3 => ("nop",      1),
        0xE4 => ("cpx",      2),
        0xE5 => ("sbc",      2),
        0xE6 => ("inc",      2),
        0xE7 => ("smb",      2),
        0xE8 => ("incx",     1),
        0xE9 => ("sbc",      2),
        0xEA => ("nop",      1),
        0xEB => ("nop",      1),
        0xEC => ("cpx",      3),
        0xED => ("sbc",      3),
        0xEE => ("inc",      3),
        0xEF => ("bbs",      3),
        0xF0 => ("branch",   2),
        0xF1 => ("sbc",      2),
        0xF2 => ("sbc",      2),
        0xF3 => ("nop",      1),
        0xF4 => ("nop",      2),
        0xF5 => ("sbc",      2),
        0xF6 => ("inc",      2),
        0xF7 => ("smb",      2),
        0xF8 => ("sed",      1),
        0xF9 => ("sbc",      3),
        0xFA => ("plx",      1),
        0xFB => ("nop",      1),
        0xFC => ("nop",      3),
        0xFD => ("sbc",      3),
        0xFE => ("inc",      3),
        0xFF => ("bbs",      3),
    }
}