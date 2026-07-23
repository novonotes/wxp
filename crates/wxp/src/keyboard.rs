//! Native keyboard routing for embedded WebViews.
//!
//! Routing is expressed as platform-neutral chords. A chord matches one key plus an explicit
//! modifier condition; by default, `KeyboardChord::new` matches only when no modifiers are held.

/// Keyboard routing policy for WebViews embedded in plugin host windows.
///
/// Regular key events and accelerators use separate rule tables because a WebView accelerator also
/// needs an explicit delivery contract. This keeps platform behavior out of the application layer:
/// macOS may use responder actions while Windows keeps the original WebView2 message path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyboardRouting {
    key_event_default: KeyEventDestination,
    accelerator_default: AcceleratorDestination,
    key_event_rules: Vec<KeyEventRoutingRule>,
    accelerator_rules: Vec<AcceleratorRoutingRule>,
}

impl KeyboardRouting {
    pub fn new(
        key_event_default: KeyEventDestination,
        accelerator_default: AcceleratorDestination,
    ) -> Self {
        Self {
            key_event_default,
            accelerator_default,
            key_event_rules: Vec::new(),
            accelerator_rules: Vec::new(),
        }
    }

    pub fn route_key_event(
        mut self,
        chord: KeyboardChord,
        destination: KeyEventDestination,
    ) -> Self {
        self.key_event_rules
            .push(KeyEventRoutingRule { chord, destination });
        self
    }

    pub fn route_accelerator(
        mut self,
        chord: KeyboardChord,
        destination: AcceleratorDestination,
    ) -> Self {
        self.accelerator_rules
            .push(AcceleratorRoutingRule { chord, destination });
        self
    }

    pub fn key_event_default(&self) -> KeyEventDestination {
        self.key_event_default
    }

    pub fn accelerator_default(&self) -> AcceleratorDestination {
        self.accelerator_default
    }

    pub fn key_event_rules(&self) -> &[KeyEventRoutingRule] {
        &self.key_event_rules
    }

    pub fn accelerator_rules(&self) -> &[AcceleratorRoutingRule] {
        &self.accelerator_rules
    }

    #[cfg(any(target_os = "macos", test))]
    pub(crate) fn macos_routing(&self) -> wry::KeyboardEventRouting<u16> {
        wry::KeyboardEventRouting {
            key_event_default: to_wry_key_event_destination(self.key_event_default),
            accelerator_default: to_wry_accelerator_destination(self.accelerator_default),
            key_event_routes: self
                .key_event_rules
                .iter()
                .filter_map(|rule| {
                    Some(wry::KeyboardEventRoute {
                        chord: wry::KeyboardEventChord {
                            key_code: rule.chord.key.macos_key_code()?,
                            modifiers: rule.chord.modifiers.to_macos_modifiers(),
                        },
                        destination: to_wry_key_event_destination(rule.destination),
                    })
                })
                .collect(),
            accelerator_routes: self
                .accelerator_rules
                .iter()
                .filter_map(|rule| {
                    Some(wry::KeyboardAcceleratorRoute {
                        chord: wry::KeyboardEventChord {
                            key_code: rule.chord.key.macos_key_code()?,
                            modifiers: rule.chord.modifiers.to_macos_modifiers(),
                        },
                        destination: to_wry_accelerator_destination(rule.destination),
                    })
                })
                .collect(),
        }
    }

    #[cfg(any(windows, test))]
    pub(crate) fn windows_routing(&self) -> wry::KeyboardEventRouting<u32> {
        wry::KeyboardEventRouting {
            key_event_default: to_wry_key_event_destination(self.key_event_default),
            accelerator_default: to_wry_accelerator_destination(self.accelerator_default),
            key_event_routes: self
                .key_event_rules
                .iter()
                .filter_map(|rule| {
                    Some(wry::KeyboardEventRoute {
                        chord: wry::KeyboardEventChord {
                            key_code: rule.chord.key.windows_virtual_key()?,
                            modifiers: rule.chord.modifiers.to_windows_modifiers(),
                        },
                        destination: to_wry_key_event_destination(rule.destination),
                    })
                })
                .collect(),
            accelerator_routes: self
                .accelerator_rules
                .iter()
                .filter_map(|rule| {
                    Some(wry::KeyboardAcceleratorRoute {
                        chord: wry::KeyboardEventChord {
                            key_code: rule.chord.key.windows_virtual_key()?,
                            modifiers: rule.chord.modifiers.to_windows_modifiers(),
                        },
                        destination: to_wry_accelerator_destination(rule.destination),
                    })
                })
                .collect(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyEventDestination {
    WebView,
    Parent,
    WebViewAndParent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcceleratorDestination {
    WebView(WebViewAcceleratorDelivery),
    Parent,
    WebViewAndParent(WebViewAcceleratorDelivery),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WebViewAcceleratorDelivery {
    /// Preserve the active platform WebView's standard shortcut behavior.
    PlatformDefault,
    /// Guarantee delivery through the DOM key event path.
    KeyEvent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyEventRoutingRule {
    pub chord: KeyboardChord,
    pub destination: KeyEventDestination,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcceleratorRoutingRule {
    pub chord: KeyboardChord,
    pub destination: AcceleratorDestination,
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
fn to_wry_key_event_destination(value: KeyEventDestination) -> wry::KeyboardEventDestination {
    match value {
        KeyEventDestination::WebView => wry::KeyboardEventDestination::WebView,
        KeyEventDestination::Parent => wry::KeyboardEventDestination::Parent,
        KeyEventDestination::WebViewAndParent => wry::KeyboardEventDestination::WebViewAndParent,
    }
}

#[cfg(any(target_os = "macos", windows, test))]
fn to_wry_accelerator_destination(
    value: AcceleratorDestination,
) -> wry::KeyboardAcceleratorDestination {
    match value {
        AcceleratorDestination::WebView(delivery) => {
            wry::KeyboardAcceleratorDestination::WebView(to_wry_accelerator_delivery(delivery))
        }
        AcceleratorDestination::Parent => wry::KeyboardAcceleratorDestination::Parent,
        AcceleratorDestination::WebViewAndParent(delivery) => {
            wry::KeyboardAcceleratorDestination::WebViewAndParent(to_wry_accelerator_delivery(
                delivery,
            ))
        }
    }
}

#[cfg(any(target_os = "macos", windows, test))]
fn to_wry_accelerator_delivery(
    value: WebViewAcceleratorDelivery,
) -> wry::WebViewAcceleratorDelivery {
    match value {
        WebViewAcceleratorDelivery::PlatformDefault => {
            wry::WebViewAcceleratorDelivery::PlatformDefault
        }
        WebViewAcceleratorDelivery::KeyEvent => wry::WebViewAcceleratorDelivery::KeyEvent,
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
        AcceleratorDestination, KeyEventDestination, KeyboardChord, KeyboardKey, KeyboardRouting,
        WebViewAcceleratorDelivery,
    };

    #[test]
    fn routes_common_keys_to_platform_codes() {
        let routing = KeyboardRouting::new(
            KeyEventDestination::WebView,
            AcceleratorDestination::WebView(WebViewAcceleratorDelivery::PlatformDefault),
        )
        .route_key_event(
            KeyboardChord::new(KeyboardKey::Space),
            KeyEventDestination::Parent,
        )
        .route_accelerator(
            KeyboardChord::new(KeyboardKey::Escape),
            AcceleratorDestination::WebViewAndParent(WebViewAcceleratorDelivery::KeyEvent),
        );

        assert_eq!(
            routing.macos_routing().key_event_routes[0].chord.key_code,
            49
        );
        assert_eq!(
            routing.windows_routing().key_event_routes[0].chord.key_code,
            0x20
        );
        assert_eq!(
            routing.macos_routing().accelerator_routes[0].destination,
            wry::KeyboardAcceleratorDestination::WebViewAndParent(
                wry::WebViewAcceleratorDelivery::KeyEvent
            )
        );
    }

    #[test]
    fn supports_primary_modifier_mapping() {
        let routing = KeyboardRouting::new(
            KeyEventDestination::WebView,
            AcceleratorDestination::WebView(WebViewAcceleratorDelivery::PlatformDefault),
        )
        .route_accelerator(
            KeyboardChord::new(KeyboardKey::A).with_primary_modifier(),
            AcceleratorDestination::Parent,
        );

        let macos = routing.macos_routing();
        assert!(macos.accelerator_routes[0].chord.modifiers.meta);
        assert!(!macos.accelerator_routes[0].chord.modifiers.control);

        let windows = routing.windows_routing();
        assert!(windows.accelerator_routes[0].chord.modifiers.control);
        assert!(!windows.accelerator_routes[0].chord.modifiers.meta);
    }

    #[test]
    fn keeps_defaults_and_rule_kinds_inspectable() {
        let routing = KeyboardRouting::new(
            KeyEventDestination::WebView,
            AcceleratorDestination::WebView(WebViewAcceleratorDelivery::PlatformDefault),
        )
        .route_key_event(
            KeyboardChord::new(KeyboardKey::Space),
            KeyEventDestination::Parent,
        )
        .route_accelerator(
            KeyboardChord::new(KeyboardKey::C).with_primary_modifier(),
            AcceleratorDestination::WebView(WebViewAcceleratorDelivery::KeyEvent),
        );

        assert_eq!(routing.key_event_default(), KeyEventDestination::WebView);
        assert_eq!(
            routing.accelerator_default(),
            AcceleratorDestination::WebView(WebViewAcceleratorDelivery::PlatformDefault)
        );
        assert_eq!(routing.key_event_rules()[0].chord.key(), KeyboardKey::Space);
        assert_eq!(
            routing.accelerator_rules()[0].destination,
            AcceleratorDestination::WebView(WebViewAcceleratorDelivery::KeyEvent)
        );
    }
}
