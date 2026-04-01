//! DPI変換とGUIサイズ調整のユーティリティ

use clack_extensions::gui::{GuiApiType, GuiSize};
use wxp::dpi::LogicalSize;

/// GuiSizeを調整（DPI変換と制約を適用）
pub fn adjust_gui_size(
    size: GuiSize,
    scale_factor: f64,
    min: LogicalSize<f64>,
    max: LogicalSize<f64>,
) -> GuiSize {
    let converter = DpiConverter::new(scale_factor);
    let logical = converter.to_logical(size);

    // サイズを制約
    let clamped = LogicalSize {
        width: logical.width.clamp(min.width, max.width),
        height: logical.height.clamp(min.height, max.height),
    };

    converter.to_gui(clamped)
}

/// DPI対応のサイズ変換とWebView統合
pub struct DpiConverter {
    scale_factor: f64,
    uses_logical: bool,
}

impl DpiConverter {
    /// スケールファクターから `DpiConverter` を作成する。
    ///
    /// プラットフォームに応じて論理ピクセルモード／物理ピクセルモードを自動選択します。
    /// macOS・Windows は論理ピクセル、Linux（X11）は物理ピクセルになります。
    pub fn new(scale_factor: f64) -> Self {
        let uses_logical = GuiApiType::default_for_current_platform()
            .map(|api| api.uses_logical_size())
            .unwrap_or(true);

        Self {
            scale_factor,
            uses_logical,
        }
    }

    /// スケールファクターを更新
    pub fn set_scale(&mut self, scale_factor: f64) {
        self.scale_factor = scale_factor;
    }

    /// 現在のスケールファクターを取得
    pub fn scale_factor(&self) -> f64 {
        self.scale_factor
    }

    /// CLAP の `GuiSize` を論理ピクセル単位の `LogicalSize` に変換する。
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

    /// 論理ピクセル単位の `LogicalSize` を CLAP の `GuiSize` に変換する。
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

    /// WebView の配置に使う `Rect` を作成する。
    ///
    /// origin は `(0, 0)` 固定で、size のみをプラットフォームに応じて
    /// 論理／物理ピクセルに変換します。
    /// [`WxpWebViewBuilder::with_bounds`](wxp::WxpWebViewBuilder::with_bounds) に渡してください。
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
