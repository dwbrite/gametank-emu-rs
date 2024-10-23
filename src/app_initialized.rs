use std::sync::Arc;
use egui::{epaint, Color32, Pos2, Rect, Vec2};
use egui_wgpu::ScreenDescriptor;
use tracing::warn;
use winit::application::ApplicationHandler;
use winit::event::{KeyEvent, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};
use crate::app_ui::gametankboy::GameTankBoyUI;
use crate::app_uninit::App;
use crate::emulator::color_map::COLOR_MAP;
use crate::egui_renderer::EguiRenderer;
use crate::emulator::emulator::Emulator;
use crate::graphics::GraphicsContext;

pub struct AppInitialized {
    pub emulator: Emulator,
    pub gc: GraphicsContext,
    pub window: Arc<Window>,
    pub egui_renderer: EguiRenderer,
    pub console_gui: GameTankBoyUI,
    show_left_pane: bool,
    show_right_pane: bool,
    show_bottom_pane: bool,
}

impl From<&mut App> for AppInitialized {
    fn from(app: &mut App) -> Self {
        let emulator = app.emulator.take().unwrap();
        let gc = app.gc.take().unwrap();
        let window = app.window.take().unwrap();
        let egui_renderer = app.egui_renderer.take().unwrap();
        let console_gui = GameTankBoyUI::init(egui_renderer.context(), Self::framebuffer_to_color_image(&emulator.cpu_bus.read_full_framebuffer()));

        Self {
            emulator,
            gc,
            window,
            egui_renderer,
            console_gui,
            show_left_pane: true,
            show_right_pane: true,
            show_bottom_pane: true,
        }
    }
}

impl AppInitialized {
    fn handle_redraw(&mut self) {
        // Fetch the framebuffer data from the emulator
        let framebuffer = self.emulator.cpu_bus.read_full_framebuffer();

        // Convert framebuffer to ColorImage
        let color_image = Self::framebuffer_to_color_image(&framebuffer);
        self.console_gui.update_screen(color_image);


        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: [self.gc.surface_config.width, self.gc.surface_config.height],
            pixels_per_point: self.window.scale_factor() as f32 * 1.0, // TODO: scale factor?
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
            inner_margin: egui::Margin::same(0.0),
            outer_margin: egui::Margin::same(0.0),
            rounding: egui::Rounding::same(0.0),
            shadow: epaint::Shadow::default(),
            ..Default::default()
        };

        egui::CentralPanel::default().frame(frame).show(self.egui_renderer.context(), |ui| {
            // Set the minimum size for the center pane
            let center_min_size = egui::vec2(128.0, 256.0);

            ui.vertical(|ui| {
                // Horizontal layout for three columns
                ui.horizontal(|ui| {
                    // screen width
                    let available_width = ui.available_width();
                    let mut center_width = 128.0;
                    let mut left_min_width = 256.0;
                    let mut right_min_width = 256.0;

                    let show_right = self.show_right_pane && available_width > center_width + right_min_width + ((self.show_left_pane) as usize as f32 * left_min_width);
                    let show_left = self.show_left_pane && available_width > center_width + left_min_width + ((show_right && self.show_right_pane) as usize as f32 * right_min_width);

                    if !show_left {
                        left_min_width = 0.0;
                    }
                    if !show_right {
                        right_min_width = 0.0;
                    }

                    if available_width > center_width + left_min_width + right_min_width {
                        center_width = available_width - left_min_width - right_min_width;
                    }


                    // // Determine the width of the center pane based on the available width
                    // let center_width = available_width.min(available_width - (self.show_left_pane as usize * 128) as f32 - (self.show_right_pane as usize * 128) as f32);

                    // Optional left pane (only render if there's enough space)
                    if show_left {
                        let height = ui.available_height();
                        ui.group(|ui| {
                            ui.set_width_range(left_min_width ..= 512.0);
                            ui.set_height(height);
                            ui.label("Left Pane");
                            // Add your left pane content here
                        });
                    }

                    ui.horizontal_centered(|ui| {
                        ui.set_height(ui.available_height());
                        let max_width = if center_width > 512.0 { 512.0 } else { center_width };
                        ui.set_width_range(center_min_size.x ..= max_width);
                        self.console_gui.draw(ui);
                    });

                    // Optional right pane (only render if there's enough space)
                    if show_right {
                        ui.group(|ui| {
                            ui.set_min_size(egui::vec2(128.0, ui.available_height())); // Fixed width for right pane
                            // ui.set_height(ui.available_height());
                            ui.label("Right Pane");
                        });
                    }
                });

                // Optional bottom pane
                // if self.show_bottom_pane {
                //     ui.add(egui::widgets::Separator::default());
                //     ui.horizontal(|ui| {
                //         ui.label("Bottom Pane");
                //         // Add your bottom pane content here
                //     });
                // }
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


    fn framebuffer_to_color_image(framebuffer: &[u8; 128*128]) -> egui::ColorImage {
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
            _ => (),
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.emulator.process_cycles(false);
        // self.window.request_redraw();
    }
}