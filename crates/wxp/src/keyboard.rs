/// Keyboard routing policy for WebViews embedded in plugin host windows.
///
/// The policy is intentionally key-based instead of product-specific. Hosts often reserve
/// transport or global shortcuts while WebView content still needs ordinary editing shortcuts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WxpKeyboardRouting {
    rules: Vec<WxpKeyboardRoutingRule>,
}

impl WxpKeyboardRouting {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn passthrough_to_parent(mut self, key: WxpKeyboardKey) -> Self {
        self.rules.push(WxpKeyboardRoutingRule {
            key,
            route: WxpKeyboardRoute::Parent,
        });
        self
    }

    pub fn rules(&self) -> &[WxpKeyboardRoutingRule] {
        &self.rules
    }

    #[cfg(any(target_os = "macos", test))]
    pub(crate) fn macos_parent_passthrough_key_codes(&self) -> Vec<u16> {
        self.rules
            .iter()
            .filter(|rule| rule.route == WxpKeyboardRoute::Parent)
            .filter_map(|rule| rule.key.macos_key_code())
            .collect()
    }

    #[cfg(any(windows, test))]
    pub(crate) fn windows_parent_passthrough_virtual_keys(&self) -> Vec<u32> {
        self.rules
            .iter()
            .filter(|rule| rule.route == WxpKeyboardRoute::Parent)
            .filter_map(|rule| rule.key.windows_virtual_key())
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WxpKeyboardRoutingRule {
    pub key: WxpKeyboardKey,
    pub route: WxpKeyboardRoute,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WxpKeyboardRoute {
    WebView,
    Parent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WxpKeyboardKey {
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

impl WxpKeyboardKey {
    pub fn native(macos_key_code: Option<u16>, windows_virtual_key: Option<u32>) -> Self {
        Self::Native {
            macos_key_code,
            windows_virtual_key,
        }
    }

    #[cfg(any(target_os = "macos", test))]
    fn macos_key_code(self) -> Option<u16> {
        match self {
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

#[cfg(target_os = "macos")]
pub(crate) fn install_keyboard_routing(
    webview: &crate::WxpWebView,
    routing: &WxpKeyboardRouting,
) -> crate::Result<()> {
    let key_codes = routing.macos_parent_passthrough_key_codes();
    if key_codes.is_empty() {
        return Ok(());
    }
    webview.set_parent_keyboard_passthrough_key_codes(key_codes)
}

#[cfg(windows)]
pub(crate) fn install_keyboard_routing(
    webview: &crate::WxpWebView,
    routing: &WxpKeyboardRouting,
) -> crate::Result<()> {
    let virtual_keys = routing.windows_parent_passthrough_virtual_keys();
    if virtual_keys.is_empty() {
        return Ok(());
    }
    webview.set_parent_keyboard_passthrough_virtual_keys(virtual_keys)
}

#[cfg(not(any(target_os = "macos", windows)))]
pub(crate) fn install_keyboard_routing(
    _webview: &crate::WxpWebView,
    _routing: &WxpKeyboardRouting,
) -> crate::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{WxpKeyboardKey, WxpKeyboardRoute, WxpKeyboardRouting};

    #[test]
    fn routes_common_keys_to_platform_codes() {
        let routing = WxpKeyboardRouting::new()
            .passthrough_to_parent(WxpKeyboardKey::Space)
            .passthrough_to_parent(WxpKeyboardKey::Escape);

        assert_eq!(routing.macos_parent_passthrough_key_codes(), vec![49, 53]);
        assert_eq!(
            routing.windows_parent_passthrough_virtual_keys(),
            vec![0x20, 0x1B]
        );
    }

    #[test]
    fn supports_native_platform_codes() {
        let routing = WxpKeyboardRouting::new()
            .passthrough_to_parent(WxpKeyboardKey::native(Some(12), Some(0x51)));

        assert_eq!(routing.macos_parent_passthrough_key_codes(), vec![12]);
        assert_eq!(
            routing.windows_parent_passthrough_virtual_keys(),
            vec![0x51]
        );
    }

    #[test]
    fn keeps_rules_inspectable() {
        let routing = WxpKeyboardRouting::new().passthrough_to_parent(WxpKeyboardKey::Space);

        assert_eq!(routing.rules()[0].key, WxpKeyboardKey::Space);
        assert_eq!(routing.rules()[0].route, WxpKeyboardRoute::Parent);
    }
}
