use egui::{Button, Color32, ColorImage, Context, ImageOptions, TextureHandle, TextureOptions, Ui, Vec2, Widget};
use image::GenericImageView;
use crate::egui_renderer::EguiRenderer;
use crate::graphics::GraphicsContext;

const MIN_GAME_SIZE: f32 = 128.0;

fn calculate_game_size(width: f32, height: f32, min_size: f32) -> f32 {
    let min_dimension = width.min(height);
    (min_dimension / min_size).floor() * min_size
}


pub struct GameTankBoyUI {
    screen: TextureHandle,
    // a: [TextureHandle; 2],
    // b: [TextureHandle; 2],
    // c: [TextureHandle; 2],
    // start: TextureHandle,
    // up: TextureHandle,
    // down: TextureHandle,
    // left: TextureHandle,
    // right: TextureHandle,
    // power: [TextureHandle; 2],
    // reset: [TextureHandle; 2],
}

fn load_png_to_image(path: &str) -> ColorImage {
    // Load the image using the image crate
    let img = image::open(path).expect("Failed to load image");
    let rgb_image = img.to_rgba8();

    // Get the dimensions of the image
    let dimensions = img.dimensions();
    let size = [dimensions.0 as usize, dimensions.1 as usize];

    // Convert the image to egui::ColorImage
    let pixels = rgb_image.as_raw();

    ColorImage::from_rgba_unmultiplied(size, pixels)
}

impl GameTankBoyUI {
    pub fn init(context: &Context, color_image: ColorImage) -> Self {
        let options = TextureOptions::NEAREST;

        let game_texture = context.load_texture("game_texture", color_image, TextureOptions::NEAREST);

        // TODO: can't load from file on web, include_bytes!()
        // let power1 = egui_renderer.context().load_texture("power_released", load_png_to_image("src/assets/POWER1.png"), options);
        // let power2 = egui_renderer.context().load_texture("power_pressed", load_png_to_image("src/assets/POWER2.png"), options);

        Self {
            // power: [power1, power2],
            screen: game_texture,
        }
    }

    pub fn update_screen(&mut self, context: &Context, color_image: ColorImage) {
        let game_texture = context.load_texture("game_texture", color_image, TextureOptions::NEAREST);
        self.screen = game_texture;
    }

    pub fn draw(&mut self, ui: &mut Ui) {
        ui.vertical(|ui| {
            let panel_rect = ui.available_rect_before_wrap();
            ui.painter().rect_filled(panel_rect, 0.0, Color32::from_rgb(227, 190, 69));

            let available_width = ui.available_width();
            let available_height = ui.available_height();

            let game_size = calculate_game_size(available_width, available_height, MIN_GAME_SIZE);
            let game_rect = egui::Rect::from_min_size(ui.min_rect().min, [game_size, game_size].into());

            let sized_texture = egui::load::SizedTexture::new(self.screen.id(), game_rect.size());

            ui.allocate_space(game_rect.size());
            ui.put(game_rect, egui::Image::new(sized_texture));

            // let btn_sized_texture = egui::load::SizedTexture::new(self.power[0].id(), Vec2::new(48.0, 48.0));
            // let button = Button::image(egui::Image::new(btn_sized_texture)).frame(false);
            //
            // if button.ui(ui).clicked() {
            //     println!("Power 1");
            //     // self.power.swap(0, 1);
            // }

            // ui.add(button);

            // TODO: button layout
            if ui.button("A").clicked() {
                println!("A");
            }
        });
    }
}