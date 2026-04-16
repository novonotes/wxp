# wxp_clack

A utility crate for integrating [clack](https://github.com/prokopyl/clack), the CLAP plugin framework, with wxp.

wxp is a general-purpose WebView UI framework that does not depend on any specific plugin framework. Conversion is needed between clack types (such as `GuiSize` and `Window`) and the types used by wxp / wry. This crate handles that bridging.

For concrete usage examples, see [`wxp-gain-example`](https://github.com/novonotes/wxp-gain-example/blob/main/README.md).
