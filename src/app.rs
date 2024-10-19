use std::cell::RefCell;
use std::future::Future;
use std::rc::Rc;
use std::sync::{mpsc, Arc};
use std::time::Duration;
use egui::Context;

use winit::application::ApplicationHandler;
use winit::event_loop::ActiveEventLoop;
#[cfg(target_arch = "wasm32")]
use winit::platform::web::EventLoopExtWebSys;
use winit::window::{Window, WindowAttributes, WindowId};

use egui_wgpu::{wgpu as wgpu, ScreenDescriptor};
use egui_wgpu::wgpu::{Features, Limits, MemoryHints};
use futures::{SinkExt, TryStreamExt};
use tracing::{debug, error, info, warn};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use winit::dpi::LogicalSize;
use winit::event::{KeyEvent, WindowEvent};
use crate::egui_renderer::EguiRenderer;
use crate::emulator::{Emulator, HEIGHT, WIDTH};
use crate::graphics::GraphicsContext;

pub struct App {
    pub emulator: Emulator,
    pub gc: Option<GraphicsContext>,
    pub window: Option<Arc<Window>>,
    pub egui_renderer: Option<EguiRenderer>,

    pub gc_tx: mpsc::Sender<GraphicsContext>,
    pub gc_rx: mpsc::Receiver<GraphicsContext>,
}

impl App {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            emulator: Emulator::init(),
            gc: None,
            window: None,
            egui_renderer: None,
            gc_tx: tx,
            gc_rx: rx,
        }
    }

    fn init_window(&mut self, event_loop: &ActiveEventLoop) {
        info!("initializing...");
        let mut window_attributes = WindowAttributes::default()
            .with_title("GameTank!")
            .with_inner_size(LogicalSize::new(WIDTH*2, HEIGHT*2))
            .with_min_inner_size(LogicalSize::new(WIDTH, HEIGHT));

        #[cfg(target_arch = "wasm32")] {
            use winit::platform::web::{EventLoopExtWebSys, WindowAttributesExtWebSys};
            use web_sys::{HtmlCanvasElement, HtmlElement};
            use wasm_bindgen::JsCast;

            let window = web_sys::window().expect("should have a Window");
            let document = window.document().expect("should have a Document");
            let canvas = document.get_element_by_id("gt-canvas").expect("should have a canvas element");
            let canvas: HtmlCanvasElement = canvas.dyn_into::<HtmlCanvasElement>().expect("failed to transmute canvas element");

            window_attributes = window_attributes.with_canvas(Some(canvas));
            info!("found canvas");
        }


        let window = Arc::new(event_loop.create_window(window_attributes).expect("failed to create window"));
        self.window = Some(window.clone());

        let window_clone = window.clone();
        let tx_clone = self.gc_tx.clone();
        crate::spawn(async move {
            let gc = GraphicsContext::new(window_clone).await;
            tx_clone.send(gc).expect("couldn't send");
        });

        self.try_graphics_context();

        info!("initialized");
    }

    fn try_graphics_context(&mut self) {
        if let Some(window) = self.window.as_ref() {
            if let Ok(mut gc) = self.gc_rx.try_recv() {
                let device = &mut gc.device;

                let fmt = gc.surface.get_current_texture().expect("ugh").texture.format();

                self.egui_renderer = Some(EguiRenderer::new(device, fmt, None, 1, &window));
                self.gc = Some(gc);
                info!("adapter has been set up");
            }
        }
    }

    fn handle_redraw(&mut self) {
        if self.gc.is_none() {
            info!("redrawing but no gc :(");
            return
        }

        let gc = self.gc.as_mut().unwrap();

        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: [gc.surface_config.width, gc.surface_config.height],
            pixels_per_point: self.window.as_ref().unwrap().scale_factor() as f32 * 1.0, // TODO: scale factor?
        };

        let surface_texture = gc
            .surface
            .get_current_texture()
            .expect("Failed to acquire next swap chain texture");

        let surface_view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = gc
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        let window = self.window.as_ref().unwrap();

        if let Some(egui_renderer) = self.egui_renderer.as_mut() {
            egui_renderer.begin_frame(window);

            egui::CentralPanel::default().show(egui_renderer.context(), |ui| {
                // ui.label("Hello, world!");
                
                // divide the screen a top half and bottom half
                ui.vertical(|ui| {
                    ui.horizontal_centered(|ui| {
                        ui.add(egui::widgets::Label::new("1"));
                        ui.vertical(|ui| {
                            // Calculate the size of the game square based on the window dimensions
                            let available_width = ui.available_width();
                            let available_height = ui.available_height();
                            let min_dimension = available_width.min(available_height);
                            let game_size = (min_dimension / 128.0).floor() * 128.0;
                            let game_rect = egui::Rect::from_min_size(ui.min_rect().min, [game_size, game_size].into());

                            // Adjust UI to allocate the square space for the game
                            ui.allocate_space(game_rect.size());

                            // Draw the game area (for illustrative purposes, we are using a label here)
                            ui.put(game_rect, egui::Label::new("game"));

                            // Draw a border around the game area
                            let stroke = egui::Stroke {
                                width: 2.0,                    // Border width
                                color: egui::Color32::WHITE,   // Border color
                            };
                            ui.painter().rect_stroke(game_rect, 0.0, stroke);

                            // Controls should occupy the rest of the vertical space
                            ui.label("controls");
                            // Or dynamically fill remaining space with a scrolling area for more controls
                            egui::ScrollArea::vertical().show(ui, |ui| {
                                ui.label("Control 1");
                                ui.label("Control 2");
                                // Add more controls as needed
                            });
                        });
                        ui.allocate_space(egui::Vec2::new(ui.available_width(), ui.available_size_before_wrap().y / 2.0));
                        ui.add(egui::widgets::Label::new("3"));
                    });
                    ui.add(egui::widgets::Separator::default());
                    ui.add(egui::widgets::Label::new("Bottom Half"));
                });
            });

            egui_renderer.end_frame_and_draw(
                &gc.device,
                &gc.queue,
                &mut encoder,
                window,
                &surface_view,
                screen_descriptor,
            );
        }

        gc.queue.submit(Some(encoder.finish()));
        surface_texture.present();
    }

    fn handle_resized(&mut self, width: u32, height: u32) {
        if self.gc.is_none() {
            warn!("resizing but no gc :(");
            return;
        }


        let gc = self.gc.as_mut().unwrap();

        gc.surface_config.width = width;
        gc.surface_config.height = height;
        gc.surface.configure(&gc.device, &gc.surface_config);
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            self.init_window(event_loop);
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        // let egui_renderer process the event first
        if let Some(egui_renderer) = self.egui_renderer.as_mut() {
            egui_renderer.handle_input(self.window.as_ref().unwrap(), &event);
        }

        if self.gc.is_none() {
            self.try_graphics_context();
        }

        match event {
            WindowEvent::CloseRequested => {
                println!("The close button was pressed; stopping");
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                self.handle_redraw();
                self.window.as_ref().unwrap().request_redraw();

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
            _ => (),
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.gc.is_none() {
            return;
        }

        let gc = self.gc.as_mut().unwrap();

        debug!("about to wait; processing");
        self.emulator.process_cycles(false);
    }
}
