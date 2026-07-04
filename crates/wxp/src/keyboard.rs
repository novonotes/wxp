//! Native keyboard routing for embedded WebViews.
//!
//! Routing is expressed as platform-neutral chords. A chord matches one key plus an explicit
//! modifier condition; by default, `KeyboardChord::new` matches only when no modifiers are held.

/// Keyboard routing policy for WebViews embedded in plugin host windows.
///
/// Defaults are explicit because plugin integrations often need to choose whether host-owned
/// accelerators or WebView-owned application shortcuts should win when no route matches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyboardRouting {
    defaults: KeyboardDefaults,
    rules: Vec<KeyboardRoutingRule>,
}

impl KeyboardRouting {
    pub fn new(defaults: KeyboardDefaults) -> Self {
        Self {
            defaults,
            rules: Vec::new(),
        }
    }

    pub fn route(mut self, chord: KeyboardChord, destination: KeyboardDestination) -> Self {
        self.rules.push(KeyboardRoutingRule { chord, destination });
        self
    }

    pub fn defaults(&self) -> KeyboardDefaults {
        self.defaults
    }

    pub fn rules(&self) -> &[KeyboardRoutingRule] {
        &self.rules
    }

    #[cfg(any(target_os = "macos", test))]
    pub(crate) fn macos_routing(&self) -> wry::KeyboardEventRouting<u16> {
        wry::KeyboardEventRouting {
            defaults: to_wry_defaults(self.defaults),
            routes: self
                .rules
                .iter()
                .filter_map(|rule| {
                    Some(wry::KeyboardEventRoute {
                        chord: wry::KeyboardEventChord {
                            key_code: rule.chord.key.macos_key_code()?,
                            modifiers: rule.chord.modifiers.to_macos_modifiers(),
                        },
                        destination: to_wry_destination(rule.destination),
                    })
                })
                .collect(),
        }
    }

    #[cfg(any(windows, test))]
    pub(crate) fn windows_routing(&self) -> wry::KeyboardEventRouting<u32> {
        wry::KeyboardEventRouting {
            defaults: to_wry_defaults(self.defaults),
            routes: self
                .rules
                .iter()
                .filter_map(|rule| {
                    Some(wry::KeyboardEventRoute {
                        chord: wry::KeyboardEventChord {
                            key_code: rule.chord.key.windows_virtual_key()?,
                            modifiers: rule.chord.modifiers.to_windows_modifiers(),
                        },
                        destination: to_wry_destination(rule.destination),
                    })
                })
                .collect(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyboardDefaults {
    /// Default destination for regular keyDown/keyUp-style events.
    pub key_events: KeyboardDestination,
    /// Default destination for platform accelerator/key-equivalent events.
    pub accelerators: KeyboardDestination,
}

impl KeyboardDefaults {
    pub const WEBVIEW: Self = Self {
        key_events: KeyboardDestination::WebView,
        accelerators: KeyboardDestination::WebView,
    };

    pub const PARENT: Self = Self {
        key_events: KeyboardDestination::Parent,
        accelerators: KeyboardDestination::Parent,
    };
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyboardRoutingRule {
    pub chord: KeyboardChord,
    pub destination: KeyboardDestination,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyboardDestination {
    WebView,
    Parent,
    WebViewAndParent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyboardChord {
    key: KeyboardKey,
    modifiers: KeyboardModifiers,
}

impl KeyboardChord {
    pub fn new(key: KeyboardKey) -> Self {
        Self {
            key,
            modifiers: KeyboardModifiers::none(),
        }
    }

    /// Matches the platform's primary application shortcut modifier:
    /// Command on macOS and Control on Windows/Linux.
    pub fn with_primary_modifier(mut self) -> Self {
        self.modifiers.primary = true;
        self
    }

    pub fn with_shift(mut self) -> Self {
        self.modifiers.shift = true;
        self
    }

    pub fn with_control(mut self) -> Self {
        self.modifiers.control = true;
        self
    }

    pub fn with_alt(mut self) -> Self {
        self.modifiers.alt = true;
        self
    }

    pub fn with_command(mut self) -> Self {
        self.modifiers.command = true;
        self
    }

    pub fn with_any_modifiers(mut self) -> Self {
        self.modifiers.any = true;
        self
    }

    pub fn key(&self) -> KeyboardKey {
        self.key
    }

    pub fn modifiers(&self) -> KeyboardModifiers {
        self.modifiers
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyboardModifiers {
    primary: bool,
    shift: bool,
    control: bool,
    alt: bool,
    command: bool,
    any: bool,
}

impl KeyboardModifiers {
    pub fn none() -> Self {
        Self {
            primary: false,
            shift: false,
            control: false,
            alt: false,
            command: false,
            any: false,
        }
    }

    pub fn is_any(self) -> bool {
        self.any
    }

    #[cfg(any(target_os = "macos", test))]
    fn to_macos_modifiers(self) -> wry::KeyboardEventModifiers {
        wry::KeyboardEventModifiers {
            shift: self.shift,
            control: self.control,
            alt: self.alt,
            meta: self.command || self.primary,
            any: self.any,
        }
    }

    #[cfg(any(windows, test))]
    fn to_windows_modifiers(self) -> wry::KeyboardEventModifiers {
        wry::KeyboardEventModifiers {
            shift: self.shift,
            control: self.control || self.primary,
            alt: self.alt,
            meta: self.command,
            any: self.any,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyboardKey {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Space,
    Escape,
    Enter,
    Tab,
    Backspace,
    Delete,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    Native {
        macos_key_code: Option<u16>,
        windows_virtual_key: Option<u32>,
    },
}

impl KeyboardKey {
    pub fn native(macos_key_code: Option<u16>, windows_virtual_key: Option<u32>) -> Self {
        Self::Native {
            macos_key_code,
            windows_virtual_key,
        }
    }

    #[cfg(any(target_os = "macos", test))]
    fn macos_key_code(self) -> Option<u16> {
        match self {
            Self::A => Some(0),
            Self::S => Some(1),
            Self::D => Some(2),
            Self::F => Some(3),
            Self::H => Some(4),
            Self::G => Some(5),
            Self::Z => Some(6),
            Self::X => Some(7),
            Self::C => Some(8),
            Self::V => Some(9),
            Self::B => Some(11),
            Self::Q => Some(12),
            Self::W => Some(13),
            Self::E => Some(14),
            Self::R => Some(15),
            Self::Y => Some(16),
            Self::T => Some(17),
            Self::O => Some(31),
            Self::U => Some(32),
            Self::I => Some(34),
            Self::P => Some(35),
            Self::L => Some(37),
            Self::J => Some(38),
            Self::K => Some(40),
            Self::N => Some(45),
            Self::M => Some(46),
            Self::Space => Some(49),
            Self::Escape => Some(53),
            Self::Enter => Some(36),
            Self::Tab => Some(48),
            Self::Backspace => Some(51),
            Self::Delete => Some(117),
            Self::ArrowLeft => Some(123),
            Self::ArrowRight => Some(124),
            Self::ArrowDown => Some(125),
            Self::ArrowUp => Some(126),
            Self::Native { macos_key_code, .. } => macos_key_code,
        }
    }

    #[cfg(any(windows, test))]
    fn windows_virtual_key(self) -> Option<u32> {
        match self {
            Self::A => Some(0x41),
            Self::B => Some(0x42),
            Self::C => Some(0x43),
            Self::D => Some(0x44),
            Self::E => Some(0x45),
            Self::F => Some(0x46),
            Self::G => Some(0x47),
            Self::H => Some(0x48),
            Self::I => Some(0x49),
            Self::J => Some(0x4A),
            Self::K => Some(0x4B),
            Self::L => Some(0x4C),
            Self::M => Some(0x4D),
            Self::N => Some(0x4E),
            Self::O => Some(0x4F),
            Self::P => Some(0x50),
            Self::Q => Some(0x51),
            Self::R => Some(0x52),
            Self::S => Some(0x53),
            Self::T => Some(0x54),
            Self::U => Some(0x55),
            Self::V => Some(0x56),
            Self::W => Some(0x57),
            Self::X => Some(0x58),
            Self::Y => Some(0x59),
            Self::Z => Some(0x5A),
            Self::Space => Some(0x20),
            Self::Escape => Some(0x1B),
            Self::Enter => Some(0x0D),
            Self::Tab => Some(0x09),
            Self::Backspace => Some(0x08),
            Self::Delete => Some(0x2E),
            Self::ArrowLeft => Some(0x25),
            Self::ArrowUp => Some(0x26),
            Self::ArrowRight => Some(0x27),
            Self::ArrowDown => Some(0x28),
            Self::Native {
                windows_virtual_key,
                ..
            } => windows_virtual_key,
        }
    }
}

#[cfg(any(target_os = "macos", windows, test))]
fn to_wry_defaults(defaults: KeyboardDefaults) -> wry::KeyboardEventDefaults {
    wry::KeyboardEventDefaults {
        key_events: to_wry_destination(defaults.key_events),
        accelerators: to_wry_destination(defaults.accelerators),
    }
}

#[cfg(any(target_os = "macos", windows, test))]
fn to_wry_destination(value: KeyboardDestination) -> wry::KeyboardEventDestination {
    match value {
        KeyboardDestination::WebView => wry::KeyboardEventDestination::WebView,
        KeyboardDestination::Parent => wry::KeyboardEventDestination::Parent,
        KeyboardDestination::WebViewAndParent => wry::KeyboardEventDestination::WebViewAndParent,
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn apply_keyboard_routing(
    webview: &crate::WxpWebView,
    routing: KeyboardRouting,
) -> crate::Result<()> {
    webview.set_keyboard_routing(routing)
}

#[cfg(windows)]
pub(crate) fn apply_keyboard_routing(
    webview: &crate::WxpWebView,
    routing: KeyboardRouting,
) -> crate::Result<()> {
    webview.set_keyboard_routing(routing)
}

#[cfg(not(any(target_os = "macos", windows)))]
pub(crate) fn apply_keyboard_routing(
    _webview: &crate::WxpWebView,
    _routing: KeyboardRouting,
) -> crate::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        KeyboardChord, KeyboardDefaults, KeyboardDestination, KeyboardKey, KeyboardRouting,
    };

    #[test]
    fn routes_common_keys_to_platform_codes() {
        let routing = KeyboardRouting::new(KeyboardDefaults::WEBVIEW)
            .route(
                KeyboardChord::new(KeyboardKey::Space),
                KeyboardDestination::Parent,
            )
            .route(
                KeyboardChord::new(KeyboardKey::Escape),
                KeyboardDestination::WebViewAndParent,
            );

        assert_eq!(routing.macos_routing().routes[0].chord.key_code, 49);
        assert_eq!(routing.windows_routing().routes[0].chord.key_code, 0x20);
        assert_eq!(
            routing.macos_routing().routes[1].destination,
            wry::KeyboardEventDestination::WebViewAndParent
        );
    }

    #[test]
    fn supports_primary_modifier_mapping() {
        let routing = KeyboardRouting::new(KeyboardDefaults::WEBVIEW).route(
            KeyboardChord::new(KeyboardKey::A).with_primary_modifier(),
            KeyboardDestination::Parent,
        );

        let macos = routing.macos_routing();
        assert!(macos.routes[0].chord.modifiers.meta);
        assert!(!macos.routes[0].chord.modifiers.control);

        let windows = routing.windows_routing();
        assert!(windows.routes[0].chord.modifiers.control);
        assert!(!windows.routes[0].chord.modifiers.meta);
    }

    #[test]
    fn keeps_rules_inspectable() {
        let routing = KeyboardRouting::new(KeyboardDefaults::WEBVIEW).route(
            KeyboardChord::new(KeyboardKey::C),
            KeyboardDestination::Parent,
        );

        assert_eq!(routing.defaults(), KeyboardDefaults::WEBVIEW);
        assert_eq!(routing.rules()[0].chord.key(), KeyboardKey::C);
        assert_eq!(routing.rules()[0].destination, KeyboardDestination::Parent);
    }
}
