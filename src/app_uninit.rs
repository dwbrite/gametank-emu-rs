use std::sync::{mpsc, Arc};
use egui::{epaint, Color32, TextureHandle, TextureOptions, Ui};
use egui::UiKind::CentralPanel;
use winit::application::ApplicationHandler;
use winit::event_loop::ActiveEventLoop;
#[cfg(target_arch = "wasm32")]
use winit::platform::web::EventLoopExtWebSys;
use winit::window::{Window, WindowAttributes, WindowId};

use egui_wgpu::{wgpu as wgpu, ScreenDescriptor};
use egui_wgpu::wgpu::{Limits, MemoryHints};
use tracing::{debug, info, warn};
use winit::dpi::LogicalSize;
use winit::event::{DeviceEvent, DeviceId, KeyEvent, StartCause, WindowEvent};
use crate::app_initialized::AppInitialized;
use crate::app_ui::gametankboy::GameTankBoyUI;
// use crate::app_ui::ui_gametank;
use crate::color_map::COLOR_MAP;
use crate::egui_renderer::EguiRenderer;
use crate::emulator::{Emulator, HEIGHT, WIDTH};
use crate::graphics::GraphicsContext;

pub struct App {
    pub emulator: Option<Emulator>,
    pub gc: Option<GraphicsContext>,
    pub window: Option<Arc<Window>>,
    pub egui_renderer: Option<EguiRenderer>,

    pub app_initialized: Option<AppInitialized>,

    pub gc_tx: mpsc::Sender<GraphicsContext>,
    pub gc_rx: mpsc::Receiver<GraphicsContext>,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            emulator: Some(Emulator::init()),
            gc: None,
            window: None,
            egui_renderer: None,
            gc_tx: tx,
            gc_rx: rx,
            app_initialized: None,
        }
    }

    fn init_window(&mut self, event_loop: &ActiveEventLoop) {
        info!("initializing...");
        #[allow(unused_mut)]
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
                // let color_image = self.framebuffer_to_color_image(&self.emulator.cpu_bus.read_full_framebuffer());
                self.gc = Some(gc);
                info!("adapter has been set up");
            }
        }
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
        if self.gc.is_none() {
            self.try_graphics_context();
        }
    }

    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        if self.gc.is_none() {
            return;
        }

        if self.gc.is_some() && self.egui_renderer.is_some() && self.window.is_some() && self.emulator.is_some() {
            warn!("initialized app");
            let app_init = AppInitialized::from(&mut *self);
            app_init.window.request_redraw();
            self.app_initialized = Some(app_init);
        }
    }
}