mod dispatch;
mod keyboardlayouts;
#[allow(unused)]
mod otherkeys;
mod pangoui;
use std::{ffi::CString, fs::File, io::Write, os::unix::prelude::AsRawFd, path::PathBuf};

use keyboardlayouts::Layouts;

use wayland_client::{
    protocol::{
        wl_buffer,
        wl_keyboard::{self, KeyState},
        wl_output, wl_seat, wl_shm, wl_surface,
    },
    Connection, QueueHandle,
};

use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::{self, Layer},
    zwlr_layer_surface_v1::{self, Anchor},
};

use wayland_protocols::xdg::shell::client::xdg_wm_base;

use wayland_protocols::xdg::xdg_output::zv1::client::zxdg_output_manager_v1;

use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::{
    zwp_virtual_keyboard_manager_v1, zwp_virtual_keyboard_v1,
};

use xkbcommon::xkb;

use pangoui::PangoUi;

use bitflags::bitflags;

bitflags! {
    #[allow(unused)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    struct KeyModifierType : u32 {
        const NoMod = 0;
        const Shift = 1;
        const CapsLock = 2;
        const Ctrl = 4;
        const Alt = 8;
        const Super = 64;
        const AltGr = 128;
    }
}
fn main() {
    let conn = Connection::connect_to_env().unwrap();

    let mut event_queue = conn.new_event_queue();
    let qhandle = event_queue.handle();

    let display = conn.display();
    display.get_registry(&qhandle, ());

    let mut state = State::init();

    event_queue.blocking_dispatch(&mut state).unwrap();
    let mut displays: usize = 0;
    while displays < state.wl_output.len() + 1 {
        event_queue.blocking_dispatch(&mut state).unwrap();
        displays = state.wl_output.len() + 1;
    }
    for index in 0..state.wl_output.len() {
        state.xdg_output_manager.as_ref().unwrap().get_xdg_output(
            &state.wl_output[index],
            &qhandle,
            (),
        );
        event_queue.blocking_dispatch(&mut state).unwrap();
    }

    if state.layer_shell.is_some() && state.wm_base.is_some() {
        state.init_virtual_keyboard(&qhandle);
        state.pangoui.set_size(state.get_size_from_display(0));
        state.set_buffer(&qhandle);
        state.init_layer_surface(
            &qhandle,
            state.get_size_from_display(0),
            Some(&state.wl_output[0].clone()),
        );
    }

    while state.running {
        event_queue.blocking_dispatch(&mut state).unwrap();
    }
}

struct State {
    running: bool,
    wl_output: Vec<wl_output::WlOutput>,
    wl_size: Vec<(i32, i32)>,
    wl_shm: Option<wl_shm::WlShm>,
    wl_seat: Option<wl_seat::WlSeat>,
    base_surface: Option<wl_surface::WlSurface>,
    layer_shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
    layer_surface: Option<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1>,
    tempfile: File,
    buffer: Option<wl_buffer::WlBuffer>,
    wm_base: Option<xdg_wm_base::XdgWmBase>,
    xdg_output_manager: Option<zxdg_output_manager_v1::ZxdgOutputManagerV1>,
    virtual_keyboard_manager: Option<zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1>,
    virtual_keyboard: Option<zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1>,
    xkb_state: xkb::State,
    pangoui: PangoUi,
    position: (f64, f64),
    touch_pos: (f64, f64),
}

impl State {
    fn init() -> Self {
        let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);

        let keymap = xkb::Keymap::new_from_names(
            &context,
            "",
            "",
            Layouts::EnglishUs.to_layout_name(), // if no , it is norwegian
            "",
            None,
            xkb::KEYMAP_COMPILE_NO_FLAGS,
        )
        .expect("xkbcommon keymap panicked!");
        State {
            running: true,
            wl_output: vec![],
            wl_size: vec![],
            wl_shm: None,
            wl_seat: None,
            base_surface: None,
            layer_shell: None,
            layer_surface: None,
            tempfile: tempfile::tempfile().unwrap(),
            buffer: None,
            wm_base: None,
            xdg_output_manager: None,
            virtual_keyboard_manager: None,
            virtual_keyboard: None,
            xkb_state: xkb::State::new(&keymap),
            pangoui: PangoUi::default(),
            position: (0.0, 0.0),
            touch_pos: (0.0, 0.0),
        }
    }

    fn set_buffer(&mut self, qh: &QueueHandle<Self>) {
        let (width, height) = self.pangoui.get_size();
        self.draw(KeyModifierType::NoMod);
        let shm = self.wl_shm.as_ref().unwrap();
        let file = &self.tempfile;
        let pool = shm.create_pool(file.as_raw_fd(), width * height * 4, qh, ());
        let buffer = pool.create_buffer(
            0,
            width,
            height,
            width * 4,
            wl_shm::Format::Argb8888,
            qh,
            (),
        );
        self.buffer = Some(buffer);
    }

    fn get_size_from_display(&self, index: usize) -> (i32, i32) {
        (self.wl_size[index].0, 300)
    }

    fn init_layer_surface(
        &mut self,
        qh: &QueueHandle<State>,
        (_width, height): (i32, i32),
        output: Option<&wl_output::WlOutput>,
    ) {
        let layer = self.layer_shell.as_ref().unwrap().get_layer_surface(
            self.base_surface.as_ref().unwrap(),
            output,
            Layer::Overlay,
            "precure".to_string(),
            qh,
            (),
        );
        layer.set_anchor(Anchor::Bottom | Anchor::Right | Anchor::Left);
        layer.set_keyboard_interactivity(zwlr_layer_surface_v1::KeyboardInteractivity::None);
        layer.set_exclusive_zone(height);
        layer.set_size(0, height as u32);
        self.base_surface.as_ref().unwrap().commit();

        self.layer_surface = Some(layer);
    }

    fn get_keymap_as_file(&mut self) -> (File, u32) {
        let keymap = self
            .xkb_state
            .get_keymap()
            .get_as_string(xkb::KEYMAP_FORMAT_TEXT_V1);
        let keymap = CString::new(keymap).expect("Keymap should not contain interior nul bytes");
        let keymap = keymap.as_bytes_with_nul();
        let dir = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let mut file = tempfile::tempfile_in(dir).expect("File could not be created!");
        file.write_all(keymap).unwrap();
        file.flush().unwrap();
        (file, keymap.len() as u32)
    }

    fn init_virtual_keyboard(&mut self, qh: &QueueHandle<Self>) {
        let virtual_keyboard_manager = self.virtual_keyboard_manager.as_ref().unwrap();
        let seat = self.wl_seat.as_ref().unwrap();
        let virtual_keyboard = virtual_keyboard_manager.create_virtual_keyboard(seat, qh, ());
        let (file, size) = self.get_keymap_as_file();
        virtual_keyboard.keymap(
            wl_keyboard::KeymapFormat::XkbV1.into(),
            file.as_raw_fd(),
            size,
        );
        self.virtual_keyboard = Some(virtual_keyboard);
    }

    fn key_press(&mut self, key: u32) {
        let virtual_keyboard = self.virtual_keyboard.as_ref().unwrap();
        //virtual_keyboard.modifiers(1, 0, 0, 0);
        virtual_keyboard.key(1, key, KeyState::Pressed.into());
    }

    fn key_release(&self, key: u32) {
        let virtual_keyboard = self.virtual_keyboard.as_ref().unwrap();
        virtual_keyboard.key(1, key, KeyState::Released.into());
    }

    fn draw(&mut self, key_type: KeyModifierType) {
        let tmp = &self.tempfile;
        let mut buf = std::io::BufWriter::new(tmp);

        for index in self.pangoui.ui(key_type).pixels() {
            let [mut r, mut g, mut b, mut a] = index.0;
            // NOTE: transparent
            if r == 255 && g == 255 && b == 255 {
                a = 50;
                r = 50;
                g = 50;
                b = 50;
            }
            buf.write_all(&[r, g, b, a]).unwrap();
        }
        buf.flush().unwrap();
    }

    fn get_key_point(&self) -> Option<u32> {
        self.pangoui.get_key(self.position)
    }

    fn get_key_touch(&self) -> Option<u32> {
        self.pangoui.get_key(self.touch_pos)
    }
}
