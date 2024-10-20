use std::sync::Arc;
use winit::window::Window;
use wgpu::{Features, Limits, MemoryHints};
use crate::emulator::{HEIGHT, WIDTH};

pub struct GraphicsContext {
    pub _instance: wgpu::Instance,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface_config: wgpu::SurfaceConfiguration,
    pub surface: wgpu::Surface<'static>,
}

impl GraphicsContext {
    pub async fn new(window: Arc<Window>) -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
        let surface = instance.create_surface(window.clone()).unwrap();

        let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
        }).await.unwrap();

        let features = Features::default();
        
        let mut limits = Limits::downlevel_webgl2_defaults();
        limits.max_texture_dimension_1d = 16384;
        limits.max_texture_dimension_2d = 16384;

        let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor {
            label: None,
            required_features: features,
            required_limits: limits,
            memory_hints: MemoryHints::default(),
        }, None).await.unwrap();

        let swapchain_capabilities = surface.get_capabilities(&adapter);

        let swapchain_format = swapchain_capabilities
            .formats.iter()
            .find(|&&fmt| fmt == wgpu::TextureFormat::Rgba8Unorm || fmt == wgpu::TextureFormat::Bgra8Unorm)
            .expect("failed to select proper surface texture format!");

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: *swapchain_format,
            width: WIDTH*2,
            height: HEIGHT*2,
            present_mode: wgpu::PresentMode::AutoVsync,
            desired_maximum_frame_latency: 0,
            alpha_mode: swapchain_capabilities.alpha_modes[0],
            view_formats: vec![],
        };

        surface.configure(&device, &surface_config);

        Self {
            _instance: instance,
            device,
            queue,
            surface_config,
            surface,
        }
    }
}