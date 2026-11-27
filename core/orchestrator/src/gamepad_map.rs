//! Pure-Rust Zero-Bloat Gamepad -> Keyboard/Mouse `uinput` Mapper for Classic 98/XP/DOS Games.
//!
//! Reads standard Linux joystick events from `/dev/input/js*` and injects keyboard/mouse
//! input through `/dev/uinput` so vintage games without XInput/DirectInput support work
//! effortlessly with Xbox, PlayStation, Nintendo Switch, and 8BitDo controllers.

use std::collections::BTreeMap;

use crate::json_codec::{to_json_pretty, JsonValue};

// Standard Linux input event keycodes (<linux/input-event-codes.h>)
pub const KEY_ESC: u16 = 1;
pub const KEY_TAB: u16 = 15;
pub const KEY_W: u16 = 17;
pub const KEY_E: u16 = 18;
pub const KEY_R: u16 = 19;
pub const KEY_ENTER: u16 = 28;
pub const KEY_LEFTCTRL: u16 = 29;
pub const KEY_A: u16 = 30;
pub const KEY_S: u16 = 31;
pub const KEY_D: u16 = 32;
pub const KEY_LEFTSHIFT: u16 = 42;
pub const KEY_Z: u16 = 44;
pub const KEY_X: u16 = 45;
pub const KEY_M: u16 = 50;
pub const KEY_LEFTALT: u16 = 56;
pub const KEY_SPACE: u16 = 57;
pub const KEY_UP: u16 = 103;
pub const KEY_LEFT: u16 = 105;
pub const KEY_RIGHT: u16 = 106;
pub const KEY_DOWN: u16 = 108;
pub const BTN_LEFT: u16 = 0x110;
pub const BTN_RIGHT: u16 = 0x111;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GamepadProfile {
    WasdMouse,
    ArrowsSpace,
    DosClassic,
}

impl GamepadProfile {
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "wasd_mouse" | "wasd" | "fps" | "3d" => Ok(Self::WasdMouse),
            "arrows_space" | "arrows" | "2d" | "rts" => Ok(Self::ArrowsSpace),
            "dos_classic" | "dos" | "retro" => Ok(Self::DosClassic),
            other => Err(format!(
                "Unknown gamepad mapper profile '{other}'. Choose wasd_mouse, arrows_space, or dos_classic."
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::WasdMouse => "wasd_mouse",
            Self::ArrowsSpace => "arrows_space",
            Self::DosClassic => "dos_classic",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::WasdMouse => {
                "Classic 3D FPS/TPS: Left Stick=WASD, Right Stick=Mouse, A=Space, B=Ctrl, X=R, Y=E, LB=Shift, RB=Tab, Start=Esc"
            }
            Self::ArrowsSpace => {
                "Classic 2D/RTS: Left Stick/D-Pad=Arrows, Right Stick=Mouse, A=Enter, B=Esc, X=Z, Y=X, LB=RightClick, RB=LeftClick"
            }
            Self::DosClassic => {
                "90s MS-DOS: Left Stick/D-Pad=Arrows, A=Ctrl, B=Alt, X=Space, Y=Enter, Start=Esc"
            }
        }
    }

    /// Map a standard Xbox/PS/Switch joystick button index (0..=10) to a Linux key/mouse code.
    pub fn map_button(self, button: u8) -> Option<u16> {
        match self {
            Self::WasdMouse => match button {
                0 => Some(KEY_SPACE),     // A / Cross
                1 => Some(KEY_LEFTCTRL),  // B / Circle
                2 => Some(KEY_R),         // X / Square
                3 => Some(KEY_E),         // Y / Triangle
                4 => Some(KEY_LEFTSHIFT), // LB / L1
                5 => Some(KEY_TAB),       // RB / R1
                6 => Some(KEY_M),         // Back / Select
                7 => Some(KEY_ESC),       // Start / Options
                9 => Some(BTN_RIGHT),     // L3
                10 => Some(BTN_LEFT),     // R3
                _ => None,
            },
            Self::ArrowsSpace => match button {
                0 => Some(KEY_ENTER),
                1 => Some(KEY_ESC),
                2 => Some(KEY_Z),
                3 => Some(KEY_X),
                4 => Some(BTN_RIGHT),
                5 => Some(BTN_LEFT),
                6 => Some(KEY_TAB),
                7 => Some(KEY_ESC),
                _ => None,
            },
            Self::DosClassic => match button {
                0 => Some(KEY_LEFTCTRL),
                1 => Some(KEY_LEFTALT),
                2 => Some(KEY_SPACE),
                3 => Some(KEY_ENTER),
                4 => Some(KEY_LEFTSHIFT),
                5 => Some(KEY_SPACE),
                6 => Some(KEY_TAB),
                7 => Some(KEY_ESC),
                _ => None,
            },
        }
    }

    /// Directional keys (left, right, up, down) bound to Left Stick & D-Pad.
    pub fn move_keys(self) -> (u16, u16, u16, u16) {
        match self {
            Self::WasdMouse => (KEY_A, KEY_D, KEY_W, KEY_S),
            Self::ArrowsSpace | Self::DosClassic => (KEY_LEFT, KEY_RIGHT, KEY_UP, KEY_DOWN),
        }
    }

    pub fn supports_mouse_stick(self) -> bool {
        matches!(self, Self::WasdMouse | Self::ArrowsSpace)
    }

    pub fn to_json(self, device: &str) -> JsonValue {
        let mut root = BTreeMap::new();
        root.insert("device".to_string(), JsonValue::Str(device.to_string()));
        root.insert(
            "description".to_string(),
            JsonValue::Str(self.description().to_string()),
        );
        root.insert("profile".to_string(), JsonValue::Str(self.as_str().to_string()));
        root.insert(
            "supports_mouse_stick".to_string(),
            JsonValue::Bool(self.supports_mouse_stick()),
        );
        JsonValue::Object(root)
    }
}

/// Convert raw signed 16-bit stick value into digital direction (-1, 0, +1) with deadzone.
pub fn axis_to_digital(value: i16, deadzone: i16) -> i8 {
    if value <= -deadzone {
        -1
    } else if value >= deadzone {
        1
    } else {
        0
    }
}

/// Convert raw signed 16-bit right-stick value into smooth relative mouse delta.
pub fn axis_to_mouse_delta(value: i16, deadzone: i16, sensitivity: i32) -> i32 {
    let v = value as i32;
    let dz = deadzone as i32;
    if v.abs() < dz {
        return 0;
    }
    let scaled = (v * sensitivity) / 32767;
    if scaled == 0 {
        v.signum()
    } else {
        scaled
    }
}

pub fn describe_plan_json(profile: GamepadProfile, device: &str) -> String {
    to_json_pretty(&profile.to_json(device))
}

#[cfg(not(target_os = "linux"))]
pub fn run_gamepad_mapper(profile: GamepadProfile, device: &str) -> Result<(), String> {
    Err(format!(
        "Live uinput gamepad mapping ({} on {}) requires UniCore OS Linux.",
        profile.as_str(),
        device
    ))
}

#[cfg(target_os = "linux")]
pub fn run_gamepad_mapper(profile: GamepadProfile, device: &str) -> Result<(), String> {
    use std::fs::OpenOptions;
    use std::io::{Read, Write};
    use std::os::unix::io::AsRawFd;
    use std::time::Duration;

    const EV_SYN: u16 = 0x00;
    const EV_KEY: u16 = 0x01;
    const EV_REL: u16 = 0x02;
    const SYN_REPORT: u16 = 0x00;
    const REL_X: u16 = 0x00;
    const REL_Y: u16 = 0x01;

    const UI_SET_EVBIT: usize = 0x4004_5564;
    const UI_SET_KEYBIT: usize = 0x4004_5565;
    const UI_SET_RELBIT: usize = 0x4004_5566;
    const UI_DEV_CREATE: usize = 0x5501;
    const UI_DEV_DESTROY: usize = 0x5502;
    const UI_DEV_SETUP: usize = 0x405c_5503;

    const JS_EVENT_BUTTON: u8 = 0x01;
    const JS_EVENT_AXIS: u8 = 0x02;
    const JS_EVENT_INIT: u8 = 0x80;

    #[repr(C)]
    struct InputId {
        bustype: u16,
        vendor: u16,
        product: u16,
        version: u16,
    }

    #[repr(C)]
    struct UinputSetup {
        id: InputId,
        name: [u8; 80],
        ff_effects_max: u32,
    }

    #[repr(C)]
    struct InputEvent {
        tv_sec: isize,
        tv_usec: isize,
        ev_type: u16,
        code: u16,
        value: i32,
    }

    extern "C" {
        fn ioctl(fd: i32, request: usize, ...) -> i32;
    }

    let mut js = OpenOptions::new()
        .read(true)
        .open(device)
        .map_err(|e| format!("Failed to open gamepad device '{device}': {e}"))?;

    let mut uinput = OpenOptions::new()
        .write(true)
        .open("/dev/uinput")
        .map_err(|e| format!("Failed to open /dev/uinput (check input group permissions): {e}"))?;
    let ufd = uinput.as_raw_fd();

    let all_keys: [u16; 20] = [
        KEY_ESC, KEY_TAB, KEY_W, KEY_E, KEY_R, KEY_ENTER, KEY_LEFTCTRL, KEY_A, KEY_S, KEY_D,
        KEY_LEFTSHIFT, KEY_Z, KEY_X, KEY_M, KEY_LEFTALT, KEY_SPACE, KEY_UP, KEY_LEFT, KEY_RIGHT,
        KEY_DOWN,
    ];
    let mouse_btns: [u16; 2] = [BTN_LEFT, BTN_RIGHT];

    unsafe {
        if ioctl(ufd, UI_SET_EVBIT, EV_KEY as usize) < 0 {
            return Err("uinput UI_SET_EVBIT(EV_KEY) failed".to_string());
        }
        for k in all_keys {
            let _ = ioctl(ufd, UI_SET_KEYBIT, k as usize);
        }
        for b in mouse_btns {
            let _ = ioctl(ufd, UI_SET_KEYBIT, b as usize);
        }
        if profile.supports_mouse_stick() {
            let _ = ioctl(ufd, UI_SET_EVBIT, EV_REL as usize);
            let _ = ioctl(ufd, UI_SET_RELBIT, REL_X as usize);
            let _ = ioctl(ufd, UI_SET_RELBIT, REL_Y as usize);
        }
        let mut setup = UinputSetup {
            id: InputId {
                bustype: 0x03, // BUS_USB
                vendor: 0x1209,
                product: 0x0900,
                version: 1,
            },
            name: [0u8; 80],
            ff_effects_max: 0,
        };
        let label = b"UniCore Virtual Gamepad Keyboard/Mouse";
        setup.name[..label.len()].copy_from_slice(label);
        if ioctl(ufd, UI_DEV_SETUP, &setup as *const UinputSetup) < 0 {
            return Err("uinput UI_DEV_SETUP failed".to_string());
        }
        if ioctl(ufd, UI_DEV_CREATE) < 0 {
            return Err("uinput UI_DEV_CREATE failed".to_string());
        }
    }

    let emit = |file: &mut std::fs::File, ev_type: u16, code: u16, value: i32| {
        let ev = InputEvent {
            tv_sec: 0,
            tv_usec: 0,
            ev_type,
            code,
            value,
        };
        let bytes = unsafe {
            std::slice::from_raw_parts(
                (&ev as *const InputEvent).cast::<u8>(),
                std::mem::size_of::<InputEvent>(),
            )
        };
        let _ = file.write_all(bytes);
    };

    let syn = |file: &mut std::fs::File| {
        emit(file, EV_SYN, SYN_REPORT, 0);
    };

    let (k_left, k_right, k_up, k_down) = profile.move_keys();
    let mut cur_x: i8 = 0;
    let mut cur_y: i8 = 0;
    let mut lt_down = false;
    let mut rt_down = false;
    let mut buf = [0u8; 8];

    loop {
        match js.read_exact(&mut buf) {
            Ok(()) => {
                let value = i16::from_le_bytes([buf[4], buf[5]]);
                let ev_type = buf[6] & !JS_EVENT_INIT;
                let number = buf[7];

                if ev_type == JS_EVENT_BUTTON {
                    if let Some(code) = profile.map_button(number) {
                        emit(&mut uinput, EV_KEY, code, if value != 0 { 1 } else { 0 });
                        syn(&mut uinput);
                    }
                } else if ev_type == JS_EVENT_AXIS {
                    match number {
                        0 | 6 => {
                            let next = axis_to_digital(value, 12000);
                            if next != cur_x {
                                if cur_x < 0 { emit(&mut uinput, EV_KEY, k_left, 0); }
                                if cur_x > 0 { emit(&mut uinput, EV_KEY, k_right, 0); }
                                if next < 0 { emit(&mut uinput, EV_KEY, k_left, 1); }
                                if next > 0 { emit(&mut uinput, EV_KEY, k_right, 1); }
                                cur_x = next;
                                syn(&mut uinput);
                            }
                        }
                        1 | 7 => {
                            let next = axis_to_digital(value, 12000);
                            if next != cur_y {
                                if cur_y < 0 { emit(&mut uinput, EV_KEY, k_up, 0); }
                                if cur_y > 0 { emit(&mut uinput, EV_KEY, k_down, 0); }
                                if next < 0 { emit(&mut uinput, EV_KEY, k_up, 1); }
                                if next > 0 { emit(&mut uinput, EV_KEY, k_down, 1); }
                                cur_y = next;
                                syn(&mut uinput);
                            }
                        }
                        3 if profile.supports_mouse_stick() => {
                            let dx = axis_to_mouse_delta(value, 5000, 14);
                            if dx != 0 {
                                emit(&mut uinput, EV_REL, REL_X, dx);
                                syn(&mut uinput);
                            }
                        }
                        4 if profile.supports_mouse_stick() => {
                            let dy = axis_to_mouse_delta(value, 5000, 14);
                            if dy != 0 {
                                emit(&mut uinput, EV_REL, REL_Y, dy);
                                syn(&mut uinput);
                            }
                        }
                        2 => {
                            let down = value > 8000;
                            if down != lt_down {
                                emit(&mut uinput, EV_KEY, BTN_RIGHT, if down { 1 } else { 0 });
                                lt_down = down;
                                syn(&mut uinput);
                            }
                        }
                        5 => {
                            let down = value > 8000;
                            if down != rt_down {
                                emit(&mut uinput, EV_KEY, BTN_LEFT, if down { 1 } else { 0 });
                                rt_down = down;
                                syn(&mut uinput);
                            }
                        }
                        _ => {}
                    }
                }
            }
            Err(_) => {
                std::thread::sleep(Duration::from_millis(50));
                break;
            }
        }
    }

    unsafe {
        let _ = ioctl(ufd, UI_DEV_DESTROY);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gamepad_profiles_and_deadzone_math_work() {
        assert_eq!(GamepadProfile::parse("wasd_mouse").unwrap(), GamepadProfile::WasdMouse);
        assert_eq!(GamepadProfile::parse("arrows_space").unwrap(), GamepadProfile::ArrowsSpace);
        assert_eq!(GamepadProfile::parse("dos_classic").unwrap(), GamepadProfile::DosClassic);
        assert!(GamepadProfile::parse("bad").is_err());

        assert_eq!(GamepadProfile::WasdMouse.map_button(0), Some(KEY_SPACE));
        assert_eq!(GamepadProfile::DosClassic.map_button(0), Some(KEY_LEFTCTRL));
        assert_eq!(axis_to_digital(0, 12000), 0);
        assert_eq!(axis_to_digital(-20000, 12000), -1);
        assert_eq!(axis_to_digital(20000, 12000), 1);
        assert_eq!(axis_to_mouse_delta(1000, 5000, 14), 0);
        assert!(axis_to_mouse_delta(30000, 5000, 14) > 0);
    }
}
