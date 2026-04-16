//! Utilities for DPI conversion and GUI size adjustment

use clack_extensions::gui::{GuiApiType, GuiSize};
use wxp::dpi::LogicalSize;

/// Adjusts a GuiSize (applies DPI conversion and constraints)
pub fn adjust_gui_size(
    size: GuiSize,
    scale_factor: f64,
    min: LogicalSize<f64>,
    max: LogicalSize<f64>,
) -> GuiSize {
    let converter = DpiConverter::new(scale_factor);
    let logical = converter.to_logical(size);

    // Clamp the size to constraints
    let clamped = LogicalSize {
        width: logical.width.clamp(min.width, max.width),
        height: logical.height.clamp(min.height, max.height),
    };

    converter.to_gui(clamped)
}

/// DPI-aware size conversion and WebView integration
pub struct DpiConverter {
    scale_factor: f64,
    uses_logical: bool,
}

impl DpiConverter {
    /// Creates a `DpiConverter` from a scale factor.
    ///
    /// Automatically selects logical pixel mode or physical pixel mode depending on the platform.
    /// macOS and Windows use logical pixels; Linux (X11) uses physical pixels.
    pub fn new(scale_factor: f64) -> Self {
        let uses_logical = GuiApiType::default_for_current_platform()
            .map(|api| api.uses_logical_size())
            .unwrap_or(true);

        Self {
            scale_factor,
            uses_logical,
        }
    }

    /// Updates the scale factor
    pub fn set_scale(&mut self, scale_factor: f64) {
        self.scale_factor = scale_factor;
    }

    /// Returns the current scale factor
    pub fn scale_factor(&self) -> f64 {
        self.scale_factor
    }

    /// Converts a CLAP `GuiSize` to a `LogicalSize` in logical pixel units.
    pub fn to_logical(&self, size: GuiSize) -> LogicalSize<f64> {
        if self.uses_logical {
            LogicalSize {
                width: size.width as f64,
                height: size.height as f64,
            }
        } else {
            LogicalSize {
                width: size.width as f64 / self.scale_factor,
                height: size.height as f64 / self.scale_factor,
            }
        }
    }

    /// Converts a `LogicalSize` in logical pixel units to a CLAP `GuiSize`.
    pub fn to_gui(&self, size: LogicalSize<f64>) -> GuiSize {
        if self.uses_logical {
            GuiSize {
                width: size.width as u32,
                height: size.height as u32,
            }
        } else {
            GuiSize {
                width: (size.width * self.scale_factor) as u32,
                height: (size.height * self.scale_factor) as u32,
            }
        }
    }

    /// Creates a `Rect` for positioning the WebView.
    ///
    /// The origin is fixed at `(0, 0)`; only the size is converted to logical or physical
    /// pixels depending on the platform.
    /// Pass the result to [`WxpWebViewBuilder::with_bounds`](wxp::WxpWebViewBuilder::with_bounds).
    pub fn create_webview_bounds(&self, size: LogicalSize<f64>) -> wxp::Rect {
        use wxp::dpi::{LogicalPosition, Size};

        wxp::Rect {
            position: LogicalPosition::new(0, 0).into(),
            size: if self.uses_logical {
                Size::Logical(size)
            } else {
                Size::Physical(size.to_physical(self.scale_factor))
            },
        }
    }
}
