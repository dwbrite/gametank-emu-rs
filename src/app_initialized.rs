use std::cell::{Cell, OnceCell};
use std::fs::File;
use std::io::Read;
use std::sync::Arc;
use egui::{epaint, vec2, Align, Button, Color32, Frame, Id, LayerId, Layout, Pos2, Rect, ResizeDirection, ScrollArea, TextureOptions, Ui, UiBuilder, Vec2, ViewportCommand};
use egui_wgpu::ScreenDescriptor;
use tracing::{error, info, warn};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{KeyEvent, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};
use crate::app_ui::gametankboy::GameTankBoyUI;
use crate::app_ui::ram_inspector::MemoryInspector;
use crate::app_ui::vram_viewer::{VRAMViewer, VRAMViewerLayout};
use crate::app_uninit::App;
use crate::emulator::color_map::{COLOR_MAP, COLOR_MAP_PERCEPTUALLY_AUTOMAPPED, COLOR_MAP_WRONG};
use crate::egui_renderer::EguiRenderer;
use crate::emulator::emulator::{Emulator, HEIGHT, WIDTH};
use crate::graphics::GraphicsContext;

pub struct AppInitialized {
    pub emulator: Emulator,
    pub gc: GraphicsContext,
    pub window: Arc<Window>,
    pub egui_renderer: EguiRenderer,

    pub console_gui: GameTankBoyUI,
    pub vram_viewer: VRAMViewer,
    pub mem_inspector: MemoryInspector,

    show_left_pane: bool,
    show_right_pane: bool,
    show_bottom_pane: bool,
}

impl From<&mut App> for AppInitialized {
    fn from(app: &mut App) -> Self {
        let mut emulator = app.emulator.take().unwrap();
        let mut gc = app.gc.take().unwrap();
        let window = app.window.take().unwrap();
        let egui_renderer = app.egui_renderer.take().unwrap();
        let console_gui = GameTankBoyUI::init(egui_renderer.context(), Self::buffer_to_color_image(&emulator.cpu_bus.read_full_framebuffer()));
        let vram_viewer = VRAMViewer::new(VRAMViewerLayout::Pages, egui_renderer.context(), &mut emulator);

        gc.surface_config.width = window.inner_size().width;
        gc.surface_config.height = window.inner_size().height;
        gc.surface.configure(&gc.device, &gc.surface_config);

        Self {
            emulator,
            gc,
            window,
            egui_renderer,
            console_gui,
            vram_viewer,
            mem_inspector: MemoryInspector {},
            show_left_pane: true,
            show_right_pane: true,
            show_bottom_pane: true,
        }
    }
}

impl AppInitialized {
    fn handle_redraw(&mut self) {
        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: [self.gc.surface_config.width, self.gc.surface_config.height],
            pixels_per_point: self.window.scale_factor() as f32 * 1.0,
        };

        let surface_texture = self.gc
            .surface
            .get_current_texture()
            .expect("Failed to acquire next swap chain texture");

        let surface_view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self.gc
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        self.egui_renderer.begin_frame(&self.window);
        let frame = egui::Frame {
            inner_margin: egui::Margin::same(0),
            outer_margin: egui::Margin::same(0),
            shadow: epaint::Shadow::default(),
            ..Default::default()
        };

        #[cfg(not(target_arch = "wasm32"))]
        {
            egui::TopBottomPanel::bottom("bottom_pane_2").resizable(false).show_separator_line(true).show_animated(self.egui_renderer.context(), self.show_bottom_pane, |ui| {
                ui.vertical(|ui| {
                    ui.vertical_centered(|ui| {
                        ui.allocate_space(vec2(ui.available_width(), 0.0));
                        self.vram_viewer.draw(ui, &mut self.emulator);
                        ui.allocate_space(vec2(ui.available_width(), 0.0));
                    });
                });
            });

            egui::TopBottomPanel::bottom("bottom_pane_1").resizable(false).show_separator_line(true).show(self.egui_renderer.context(), |ui| {
                ui.horizontal(|ui| {
                    ui.toggle_value(&mut self.show_left_pane, "show left panel");
                    ui.toggle_value(&mut self.show_bottom_pane, "show bottom panel");
                    ui.toggle_value(&mut self.show_right_pane, "show right panel");
                });
            });

            let mut left_size = 0.0;
            let mut right_size = 0.0;

            egui::SidePanel::left("left_pane").resizable(true).min_width(0.0).show_separator_line(true).frame(Frame {
                inner_margin: vec2(0.0, 0.0).into(),
                outer_margin: vec2(0.0, 0.0).into(),
                fill: Color32::from_gray(24),
                ..Default::default()
            }).show_animated(self.egui_renderer.context(), self.show_left_pane, |ui| {
                left_size = ui.available_width();

                if self.show_left_pane {
                    self.mem_inspector.draw(ui, &mut self.emulator);
                }
            });

            egui::SidePanel::right("right_pane").resizable(true).min_width(0.0).show_separator_line(true).frame(Frame {
                inner_margin: vec2(0.0, 0.0).into(),
                outer_margin: vec2(0.0, 0.0).into(),
                fill: Color32::from_gray(24),
                ..Default::default()
            }).show_animated(self.egui_renderer.context(), self.show_right_pane, |ui| {
                right_size = ui.available_width();

                if self.show_right_pane {
                    let sa = ScrollArea::both().enable_scrolling(true).min_scrolled_width(0.0).show(ui, |ui| {
                        ui.with_layout(Layout::top_down_justified(Align::RIGHT), |ui| {
                            Frame::default().show(ui, |ui| {
                                ui.set_min_width(24.0);
                                // ui.set_width(ui.available_width());
                                ui.set_height(ui.available_height());
                                ui.label("here's some gui shit");
                            })
                        });

                        ui.allocate_space(ui.available_size());
                    });
                }
            });
        }

        egui::CentralPanel::default().frame(frame).show(self.egui_renderer.context(), |ui| {
            // Set the minimum size for the center pane
            let center_min_size = egui::vec2(128.0, 128.0);
            ui.set_min_size(center_min_size);
            ui.horizontal_centered(|ui| {
                ui.set_height(ui.available_height());
                self.console_gui.draw(ui, &mut self.emulator);
            });
        });

        self.egui_renderer.end_frame_and_draw(
            &self.gc.device,
            &self.gc.queue,
            &mut encoder,
            &self.window,
            &surface_view,
            screen_descriptor,
        );

        self.gc.queue.submit(Some(encoder.finish()));
        surface_texture.present();
    }


    pub fn buffer_to_color_image(framebuffer: &[u8; 128*128]) -> egui::ColorImage {
        let mut pixels: Vec<u8> = Vec::with_capacity(128 * 128 * 4); // 4 channels per pixel (RGBA)

        for &index in framebuffer.iter() {
            let (r, g, b, a) = COLOR_MAP[index as usize];
            pixels.push(r);
            pixels.push(g);
            pixels.push(b);
            pixels.push(a);
        }

        egui::ColorImage::from_rgba_unmultiplied([128, 128], &pixels)
    }


    fn handle_resized(&mut self, width: u32, height: u32) {
        self.gc.surface_config.width = width;
        self.gc.surface_config.height = height;
        self.gc.surface.configure(&self.gc.device, &self.gc.surface_config);
    }
}

use std::cell::RefCell;
use wasm_bindgen::prelude::*;
use crate::PlayState::{Paused, Playing, WasmInit};

// Use `thread_local!` to store per-thread global data in WASM
thread_local! {
    static ROM_DATA: RefCell<Option<Vec<u8>>> = RefCell::new(None);
    static SHOULD_SHUTDOWN: Cell<bool> = Cell::new(false);
}

// Function to update the ROM data from JavaScript
#[wasm_bindgen]
pub fn update_rom_data(data: &[u8]) {
    warn!("Loading new ROM into rust memory");
    ROM_DATA.with(|storage| {
        *storage.borrow_mut() = Some(data.to_vec());
    });
}

#[wasm_bindgen]
pub fn request_close() {
    warn!("Closing egui");
    SHOULD_SHUTDOWN.with(|flag| flag.set(true));
}

impl ApplicationHandler for AppInitialized {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        todo!()
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
        self.emulator.process_cycles(false);
        self.egui_renderer.handle_input(&self.window, &event);

        match event {
            WindowEvent::CloseRequested => {
                println!("The close button was pressed; stopping");
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                self.handle_redraw();
                self.window.request_redraw();

            }
            WindowEvent::Resized(new_size) => {
                self.handle_resized(new_size.width, new_size.height);
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let KeyEvent {  logical_key,   state,  .. } = event;
                self.emulator.set_input_state(logical_key, state);
            },
            WindowEvent::MouseInput { .. } => { self.emulator.wasm_init(); }
            WindowEvent::Touch(_) => { self.emulator.wasm_init(); }
            WindowEvent::DroppedFile(path) => {
                warn!("reading file from path...");
                // check if filename ends in .gtr and load file into slice
                let filename = path.file_name().unwrap().to_str().unwrap();
                if !filename.ends_with(".gtr") {
                    error!("not a valid gtr");
                    return
                }

                let mut file = File::open(&path).unwrap();
                let mut bytes = Vec::new();
                file.read_to_end(&mut bytes).unwrap();

                self.emulator.load_rom(bytes.as_slice());
                warn!("successfully loaded {}", filename);
            }
            _ => (),
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Check if a new ROM is waiting
        if let Some(data) = &ROM_DATA.take() {
            warn!("got rom data!");
            if !data.is_empty() {
                self.emulator.load_rom(data);
            }
        }

        if SHOULD_SHUTDOWN.with(|flag| flag.get()) {
            event_loop.exit();
        }

        self.emulator.process_cycles(false);
    }
}