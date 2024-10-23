use egui::{vec2, Button, Color32, ColorImage, Context, Frame, ImageOptions, Rounding, Shadow, Style, TextureHandle, TextureOptions, Ui, Vec2, Widget};
use image::GenericImageView;
use tracing::warn;
use crate::egui_renderer::EguiRenderer;
use crate::graphics::GraphicsContext;

const MIN_GAME_SIZE: f32 = 128.0;

fn calculate_game_size(width: f32, height: f32, min_size: f32) -> f32 {
    let min_dimension = 128.0;
    (min_dimension / min_size).floor() * min_size
}


pub struct GameTankBoyUI {
    screen: Box<TextureHandle>,
    // a: [TextureHandle; 2],
    // b: [TextureHandle; 2],
    // c: [TextureHandle; 2],
    // start: TextureHandle,
    // up: TextureHandle,
    // down: TextureHandle,
    // left: TextureHandle,
    // right: TextureHandle,
    power: [TextureHandle; 2],
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
        let power1 = context.load_texture("power_released", load_png_to_image("src/assets/POWER1.png"), options);
        let power2 = context.load_texture("power_pressed", load_png_to_image("src/assets/POWER2.png"), options);

        Self {
            power: [power1, power2],
            screen: Box::new(game_texture),
        }
    }

    pub fn update_screen(&mut self, color_image: ColorImage) {
        self.screen.set_partial([0, 0], color_image, TextureOptions::NEAREST);
    }

    pub fn draw(&mut self, ui: &mut Ui) {
        let available_width = ui.available_width();
        let available_height = ui.available_height();

        let game_size = calculate_game_size(available_width, available_height, MIN_GAME_SIZE);
        let mut game_rect = egui::Rect::from_min_size([128.0, 128.0].into(), [game_size, game_size].into());

        let sized_texture = egui::load::SizedTexture::new(self.screen.id(), game_rect.size());

        let c = Color32::from_rgb(227, 190, 69);
        let frame = Frame {
            fill: c,
            ..Default::default()
        };

        frame.show(ui, |ui| {
            ui.vertical_centered(|ui| {
                // ui.style_mut().debug.debug_on_hover = true;
                ui.visuals_mut().widgets.active.bg_fill = c;

                let available_width = ui.available_width();
                // let available_height = ui.available_height();

                let margin_x = game_rect.width() * 0.2;
                let mut margin_y = game_rect.height() * 0.05;

                // if available_height < game_rect.height() + margin_y * 2.0 {
                //     margin_y = (available_height - game_rect.height()) / 2.0;
                // }

                ui.set_width(available_width);
                ui.set_height(512.0);
                game_rect.extend_with_x(64.0);

                // this is the screen:
                let frame_color = Color32::from_gray(8); // Light gray color for the frame
                let game_frame = Frame {
                    inner_margin: vec2(margin_x, margin_y).into(),
                    rounding: Rounding::same(margin_y),
                    fill: frame_color,
                    outer_margin: vec2(0.0, 0.0).into(),
                    shadow: Shadow {
                        offset: vec2(0.0, 0.0),
                        blur: 2.0,
                        spread: 0.5,
                        color: Color32::from_rgb((c.r() as f32 * 0.4) as u8, (c.g() as f32 * 0.4) as u8, (c.b() as f32 * 0.2) as u8),
                    },
                    stroke: Default::default(),
                };

                game_frame.show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.set_width(game_rect.width());
                        ui.set_height_range(0.0 ..= game_rect.height());
                        ui.add(egui::Image::new(sized_texture));
                    })
                });
                // ui.add(egui::Image::new(sized_texture));

                let btn_sized_texture = egui::load::SizedTexture::new(self.power[0].id(), Vec2::new(48.0, 48.0));
                let button = Button::image(egui::Image::new(btn_sized_texture)).frame(false);

                if button.ui(ui).clicked() {
                    self.power.swap(0, 1);
                }
            });
        });
    }
}