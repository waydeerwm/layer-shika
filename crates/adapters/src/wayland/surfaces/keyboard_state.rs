use slint::SharedString;
use slint::platform::Key;
use xkbcommon::xkb;

pub struct KeyboardState {
    pub(crate) xkb_context: xkb::Context,
    pub(crate) xkb_keymap: Option<xkb::Keymap>,
    pub(crate) xkb_state: Option<xkb::State>,
    pub(crate) repeat_rate: i32,
    pub(crate) repeat_delay: i32,
}

impl KeyboardState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            xkb_context: xkb::Context::new(xkb::CONTEXT_NO_FLAGS),
            xkb_keymap: None,
            xkb_state: None,
            repeat_rate: 25,
            repeat_delay: 600,
        }
    }

    pub fn set_keymap(&mut self, keymap: xkb::Keymap) {
        self.xkb_state = Some(xkb::State::new(&keymap));
        self.xkb_keymap = Some(keymap);
    }
}

pub(crate) fn keysym_to_text(keysym: xkb::Keysym) -> Option<SharedString> {
    let key = match keysym.raw() {
        xkb::keysyms::KEY_Return | xkb::keysyms::KEY_KP_Enter => Key::Return,
        xkb::keysyms::KEY_BackSpace => Key::Backspace,
        xkb::keysyms::KEY_Tab => Key::Tab,
        xkb::keysyms::KEY_BackTab => Key::Backtab,
        xkb::keysyms::KEY_Escape => Key::Escape,
        xkb::keysyms::KEY_Delete => Key::Delete,
        xkb::keysyms::KEY_Insert => Key::Insert,
        xkb::keysyms::KEY_Home => Key::Home,
        xkb::keysyms::KEY_End => Key::End,
        xkb::keysyms::KEY_Page_Up => Key::PageUp,
        xkb::keysyms::KEY_Page_Down => Key::PageDown,
        xkb::keysyms::KEY_Left => Key::LeftArrow,
        xkb::keysyms::KEY_Right => Key::RightArrow,
        xkb::keysyms::KEY_Up => Key::UpArrow,
        xkb::keysyms::KEY_Down => Key::DownArrow,
        xkb::keysyms::KEY_space => Key::Space,
        xkb::keysyms::KEY_Shift_L => Key::Shift,
        xkb::keysyms::KEY_Shift_R => Key::ShiftR,
        xkb::keysyms::KEY_Control_L => Key::Control,
        xkb::keysyms::KEY_Control_R => Key::ControlR,
        xkb::keysyms::KEY_Alt_L | xkb::keysyms::KEY_Alt_R => Key::Alt,
        xkb::keysyms::KEY_Mode_switch => Key::AltGr,
        xkb::keysyms::KEY_Meta_L => Key::Meta,
        xkb::keysyms::KEY_Meta_R => Key::MetaR,
        xkb::keysyms::KEY_Caps_Lock => Key::CapsLock,
        xkb::keysyms::KEY_F1 => Key::F1,
        xkb::keysyms::KEY_F2 => Key::F2,
        xkb::keysyms::KEY_F3 => Key::F3,
        xkb::keysyms::KEY_F4 => Key::F4,
        xkb::keysyms::KEY_F5 => Key::F5,
        xkb::keysyms::KEY_F6 => Key::F6,
        xkb::keysyms::KEY_F7 => Key::F7,
        xkb::keysyms::KEY_F8 => Key::F8,
        xkb::keysyms::KEY_F9 => Key::F9,
        xkb::keysyms::KEY_F10 => Key::F10,
        xkb::keysyms::KEY_F11 => Key::F11,
        xkb::keysyms::KEY_F12 => Key::F12,
        _ => return None,
    };

    Some(SharedString::from(key))
}
