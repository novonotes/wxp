/// Keyboard routing policy for WebViews embedded in plugin host windows.
///
/// The policy is intentionally key-based instead of product-specific. Products decide when a
/// key should go to the host, while wxp keeps the native event routing cross-platform.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WxpKeyboardRouting {
    rules: Vec<WxpKeyboardRoutingRule>,
}

impl WxpKeyboardRouting {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn route(mut self, key: WxpKeyboardKey, destination: WxpKeyboardDestination) -> Self {
        self.rules.push(WxpKeyboardRoutingRule { key, destination });
        self
    }

    pub fn rules(&self) -> &[WxpKeyboardRoutingRule] {
        &self.rules
    }

    #[cfg(any(target_os = "macos", test))]
    pub(crate) fn macos_routes(&self) -> Vec<(u16, WxpKeyboardDestination)> {
        self.rules
            .iter()
            .filter_map(|rule| Some((rule.key.macos_key_code()?, rule.destination)))
            .collect()
    }

    #[cfg(any(windows, test))]
    pub(crate) fn windows_routes(&self) -> Vec<(u32, WxpKeyboardDestination)> {
        self.rules
            .iter()
            .filter_map(|rule| Some((rule.key.windows_virtual_key()?, rule.destination)))
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WxpKeyboardRoutingRule {
    pub key: WxpKeyboardKey,
    pub destination: WxpKeyboardDestination,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WxpKeyboardDestination {
    WebView,
    Parent,
    WebViewAndParent,
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
pub(crate) fn apply_keyboard_routing(
    webview: &crate::WxpWebView,
    routing: WxpKeyboardRouting,
) -> crate::Result<()> {
    webview.set_keyboard_routing(routing)
}

#[cfg(windows)]
pub(crate) fn apply_keyboard_routing(
    webview: &crate::WxpWebView,
    routing: WxpKeyboardRouting,
) -> crate::Result<()> {
    webview.set_keyboard_routing(routing)
}

#[cfg(not(any(target_os = "macos", windows)))]
pub(crate) fn apply_keyboard_routing(
    _webview: &crate::WxpWebView,
    _routing: WxpKeyboardRouting,
) -> crate::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{WxpKeyboardDestination, WxpKeyboardKey, WxpKeyboardRouting};

    #[test]
    fn routes_common_keys_to_platform_codes() {
        let routing = WxpKeyboardRouting::new()
            .route(WxpKeyboardKey::Space, WxpKeyboardDestination::Parent)
            .route(
                WxpKeyboardKey::Escape,
                WxpKeyboardDestination::WebViewAndParent,
            );

        assert_eq!(
            routing.macos_routes(),
            vec![
                (49, WxpKeyboardDestination::Parent),
                (53, WxpKeyboardDestination::WebViewAndParent),
            ]
        );
        assert_eq!(
            routing.windows_routes(),
            vec![
                (0x20, WxpKeyboardDestination::Parent),
                (0x1B, WxpKeyboardDestination::WebViewAndParent),
            ]
        );
    }

    #[test]
    fn supports_native_platform_codes() {
        let routing = WxpKeyboardRouting::new().route(
            WxpKeyboardKey::native(Some(12), Some(0x51)),
            WxpKeyboardDestination::Parent,
        );

        assert_eq!(
            routing.macos_routes(),
            vec![(12, WxpKeyboardDestination::Parent)]
        );
        assert_eq!(
            routing.windows_routes(),
            vec![(0x51, WxpKeyboardDestination::Parent)]
        );
    }

    #[test]
    fn keeps_rules_inspectable() {
        let routing =
            WxpKeyboardRouting::new().route(WxpKeyboardKey::Space, WxpKeyboardDestination::Parent);

        assert_eq!(routing.rules()[0].key, WxpKeyboardKey::Space);
        assert_eq!(
            routing.rules()[0].destination,
            WxpKeyboardDestination::Parent
        );
    }
}
