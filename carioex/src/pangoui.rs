mod smallkeyboard;
//use std::f64::consts::PI;

use smallkeyboard::{draw_number_keyboard, find_keycode_from_smallkeyboard};

#[derive(Debug, Default)]
pub struct PangoUi {
    width: i32,
    height: i32,
}

impl PangoUi {
    pub fn ui(&self) -> image::ImageBuffer<image::Rgba<u8>, Vec<u8>> {
        let height = self.height;
        let width = self.width;
        let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, width, height).unwrap();
        let cr = cairo::Context::new(&surface).unwrap();
        cr.set_source_rgb(1_f64, 1_f64, 1_f64);
        cr.paint().unwrap();

        draw_number_keyboard(&cr, width, height, 27);

        use std::io::Cursor;
        let mut buff = Cursor::new(Vec::new());

        surface.write_to_png(&mut buff).unwrap();
        image::load_from_memory_with_format(buff.get_ref(), image::ImageFormat::Png)
            .unwrap()
            .to_rgba8()
    }
    pub fn set_size(&mut self, (width, height): (i32, i32)) {
        self.width = width;
        self.height = height;
    }

    pub fn get_size(&self) -> (i32, i32) {
        (self.width, self.height)
    }

    pub fn get_key(&self, (pos_x, pos_y): (f64, f64)) -> Option<u32> {
        let (pos_x, pos_y) = (pos_x as i32, pos_y as i32);
        let step = self.height / 3;
        let x_1 = self.width - 4 * step;
        let x_4 = self.width - step;

        if pos_x < x_1 {
            return None;
        } else if pos_x > x_4 {
            if pos_y / step == 1 {
                return Some(11);
            } else {
                return None;
            }
        }
        Some(find_keycode_from_smallkeyboard((pos_x, pos_y), x_1, step))
    }
}
