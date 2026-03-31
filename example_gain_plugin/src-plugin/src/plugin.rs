use std::ffi::CStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use atomic_float::AtomicF32;
use clack_extensions::audio_ports::{
    AudioPortFlags, AudioPortInfo, AudioPortInfoWriter, AudioPortType, PluginAudioPorts,
    PluginAudioPortsImpl,
};
use clack_extensions::gui::PluginGui;
use clack_extensions::params::PluginParams;
use clack_extensions::state::PluginState;
use clack_plugin::factory::plugin::PluginFactoryImpl;
use clack_plugin::host::HostInfo;
use clack_plugin::plugin::PluginInstance;
use clack_plugin::prelude::*;
use novonotes_run_loop::{RunLoop, RunLoopSender};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use wxp::{Channel, WebViewRef, WxpCommandHandler, dpi::LogicalSize};
use wxp_clack::dpi::DpiConverter;

use crate::audio::WxpExampleGainAudioProcessor;

pub(crate) const PLUGIN_ID: &str = "com.novo-notes.wxp-example-gain";
pub(crate) const PLUGIN_NAME: &str = "WXP Example Gain";
pub(crate) const PARAM_GAIN_ID: ClapId = ClapId::new(1);
pub(crate) const DEFAULT_GAIN: f32 = 1.0;
pub(crate) const MIN_GAIN: f32 = 0.0;
pub(crate) const MAX_GAIN: f32 = 2.0;
pub(crate) const DEFAULT_GUI_SIZE: LogicalSize<f64> = LogicalSize::new(360.0, 360.0);

pub(crate) struct WxpExampleGainPluginFactory {
    descriptor: PluginDescriptor,
}

pub(crate) struct WxpExampleGainPlugin;

pub(crate) struct SharedState {
    pub(crate) inner: Arc<SharedStateInner>,
}

pub(crate) struct SharedStateInner {
    gain: AtomicF32,
    pending_ui: PendingUiState,
    gui_notifier: Mutex<Option<GuiNotifier>>,
}

struct PendingUiState {
    gesture_begin: AtomicBool,
    value_dirty: AtomicBool,
    gesture_end: AtomicBool,
}

#[derive(Clone)]
struct GuiNotifier {
    sender: RunLoopSender,
    channel: Channel,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct SavedPluginState {
    pub(crate) gain: f32,
}

pub(crate) struct WxpExampleGainMainThread<'a> {
    pub(crate) shared: &'a SharedState,
    pub(crate) web_view: Option<WebViewRef>,
    pub(crate) wry_context: Option<wry::WebContext>,
    pub(crate) command_handler: Arc<WxpCommandHandler>,
    pub(crate) gui_size: LogicalSize<f64>,
    pub(crate) dpi_converter: DpiConverter,
}

impl WxpExampleGainPluginFactory {
    pub(crate) fn new() -> Self {
        Self {
            descriptor: PluginDescriptor::new(PLUGIN_ID, PLUGIN_NAME).with_features([
                clack_plugin::plugin::features::AUDIO_EFFECT,
                clack_plugin::plugin::features::STEREO,
            ]),
        }
    }
}

impl PluginFactoryImpl for WxpExampleGainPluginFactory {
    fn plugin_count(&self) -> u32 {
        1
    }

    fn plugin_descriptor(&self, index: u32) -> Option<&PluginDescriptor> {
        (index == 0).then_some(&self.descriptor)
    }

    fn create_plugin<'a>(
        &'a self,
        host_info: HostInfo<'a>,
        plugin_id: &CStr,
    ) -> Option<PluginInstance<'a>> {
        if plugin_id.to_string_lossy() != PLUGIN_ID {
            return None;
        }

        Some(PluginInstance::new::<WxpExampleGainPlugin>(
            host_info,
            &self.descriptor,
            |host| WxpExampleGainPlugin::new_shared(host),
            |host, shared| WxpExampleGainPlugin::new_main_thread(host, shared),
        ))
    }
}

impl Plugin for WxpExampleGainPlugin {
    type AudioProcessor<'a> = WxpExampleGainAudioProcessor<'a>;
    type Shared<'a> = SharedState;
    type MainThread<'a> = WxpExampleGainMainThread<'a>;

    fn declare_extensions(
        builder: &mut PluginExtensions<Self>,
        _shared: Option<&Self::Shared<'_>>,
    ) {
        builder
            .register::<PluginAudioPorts>()
            .register::<PluginParams>()
            .register::<PluginState>()
            .register::<PluginGui>();
    }
}

impl DefaultPluginFactory for WxpExampleGainPlugin {
    fn get_descriptor() -> PluginDescriptor {
        PluginDescriptor::new(PLUGIN_ID, PLUGIN_NAME).with_features([
            clack_plugin::plugin::features::AUDIO_EFFECT,
            clack_plugin::plugin::features::STEREO,
        ])
    }

    fn new_shared(_host: HostSharedHandle<'_>) -> Result<Self::Shared<'_>, PluginError> {
        Ok(SharedState::new())
    }

    fn new_main_thread<'a>(
        _host: HostMainThreadHandle<'a>,
        shared: &'a Self::Shared<'a>,
    ) -> Result<Self::MainThread<'a>, PluginError> {
        let command_handler = Arc::new(WxpCommandHandler::new());
        register_commands(command_handler.clone(), shared.inner.clone());

        Ok(WxpExampleGainMainThread {
            shared,
            web_view: None,
            wry_context: None,
            command_handler,
            gui_size: DEFAULT_GUI_SIZE,
            dpi_converter: DpiConverter::new(1.0),
        })
    }
}

impl PluginShared<'_> for SharedState {}

impl<'a> PluginMainThread<'a, SharedState> for WxpExampleGainMainThread<'a> {}

impl PluginAudioPortsImpl for WxpExampleGainMainThread<'_> {
    fn count(&mut self, _is_input: bool) -> u32 {
        1
    }

    fn get(&mut self, index: u32, is_input: bool, writer: &mut AudioPortInfoWriter) {
        if index != 0 {
            return;
        }

        writer.set(&AudioPortInfo {
            id: ClapId::new(if is_input { 1 } else { 2 }),
            name: if is_input { b"Main In" } else { b"Main Out" },
            channel_count: 2,
            flags: AudioPortFlags::IS_MAIN,
            port_type: Some(AudioPortType::STEREO),
            in_place_pair: None,
        });
    }
}

impl SharedState {
    fn new() -> Self {
        Self {
            inner: Arc::new(SharedStateInner::new()),
        }
    }
}

impl SharedStateInner {
    fn new() -> Self {
        Self {
            gain: AtomicF32::new(DEFAULT_GAIN),
            pending_ui: PendingUiState {
                gesture_begin: AtomicBool::new(false),
                value_dirty: AtomicBool::new(false),
                gesture_end: AtomicBool::new(false),
            },
            gui_notifier: Mutex::new(None),
        }
    }

    pub(crate) fn gain(&self) -> f32 {
        self.gain.load(Ordering::Acquire)
    }

    pub(crate) fn set_gain_from_host(&self, gain: f64) -> f32 {
        let gain = clamp_gain(gain as f32);
        self.gain.store(gain, Ordering::Release);
        self.notify_gui();
        gain
    }

    pub(crate) fn begin_gesture_from_ui(&self) {
        self.pending_ui.gesture_begin.store(true, Ordering::Release);
    }

    pub(crate) fn set_gain_from_ui(&self, gain: f64) -> f32 {
        let gain = self.set_gain_from_host(gain);
        self.pending_ui.value_dirty.store(true, Ordering::Release);
        gain
    }

    pub(crate) fn end_gesture_from_ui(&self) {
        self.pending_ui.gesture_end.store(true, Ordering::Release);
    }

    pub(crate) fn take_ui_gesture_begin(&self) -> bool {
        self.pending_ui.gesture_begin.swap(false, Ordering::AcqRel)
    }

    pub(crate) fn take_ui_value_dirty(&self) -> bool {
        self.pending_ui.value_dirty.swap(false, Ordering::AcqRel)
    }

    pub(crate) fn take_ui_gesture_end(&self) -> bool {
        self.pending_ui.gesture_end.swap(false, Ordering::AcqRel)
    }

    pub(crate) fn set_gui_channel(&self, sender: RunLoopSender, channel: Channel) {
        *self.gui_notifier.lock() = Some(GuiNotifier { sender, channel });
    }

    pub(crate) fn clear_gui_channel(&self) {
        *self.gui_notifier.lock() = None;
    }

    fn notify_gui(&self) {
        let Some(notifier) = self.gui_notifier.lock().clone() else {
            return;
        };

        let payload = gain_payload(self.gain());
        notifier.sender.send(move || {
            let _ = notifier.channel.send(payload);
        });
    }
}

impl WxpExampleGainMainThread<'_> {
    pub(crate) fn preferred_api(&self) -> Option<clack_extensions::gui::GuiConfiguration<'static>> {
        Some(clack_extensions::gui::GuiConfiguration {
            api_type: clack_extensions::gui::GuiApiType::default_for_current_platform()?,
            is_floating: false,
        })
    }

    pub(crate) fn reset_webview(&mut self) {
        self.shared.inner.clear_gui_channel();
        self.web_view = None;
        self.wry_context = None;
    }
}

pub(crate) fn gain_payload(gain: f32) -> serde_json::Value {
    json!({
        "type": "gain-state",
        "value": gain,
        "dbText": gain_db_text(gain as f64),
    })
}

pub(crate) fn clamp_gain(gain: f32) -> f32 {
    gain.clamp(MIN_GAIN, MAX_GAIN)
}

pub(crate) fn gain_db_text(gain: f64) -> String {
    if gain <= 0.0 {
        "-inf dB".to_string()
    } else {
        format!("{:.1} dB", 20.0 * gain.log10())
    }
}

pub(crate) fn register_commands(
    command_handler: Arc<WxpCommandHandler>,
    shared: Arc<SharedStateInner>,
) {
    {
        let shared = shared.clone();
        command_handler.register_sync("get_gain_state", move |_ctx| {
            Ok::<_, String>(gain_payload(shared.gain()))
        });
    }

    {
        let shared = shared.clone();
        command_handler.register_sync("begin_parameter_gesture", move |_ctx| {
            shared.begin_gesture_from_ui();
            Ok::<_, String>(json!({ "ok": true }))
        });
    }

    {
        let shared = shared.clone();
        command_handler.register_sync("set_gain", move |ctx| {
            let value = ctx.arg::<f64>("value").map_err(|e| e.to_string())?;
            let applied = shared.set_gain_from_ui(value);
            Ok::<_, String>(gain_payload(applied))
        });
    }

    {
        let shared = shared.clone();
        command_handler.register_sync("end_parameter_gesture", move |_ctx| {
            shared.end_gesture_from_ui();
            Ok::<_, String>(json!({ "ok": true }))
        });
    }

    {
        let shared = shared.clone();
        command_handler.register_sync("subscribe_gain", move |ctx| {
            let channel = ctx.arg::<Channel>("channel").map_err(|e| e.to_string())?;
            channel
                .send(gain_payload(shared.gain()))
                .map_err(|e| e.to_string())?;

            shared.set_gui_channel(RunLoop::sender(), channel);

            Ok::<_, String>(json!({ "ok": true }))
        });
    }

    {
        let shared = shared.clone();
        command_handler.register_sync("unsubscribe_gain", move |_ctx| {
            shared.clear_gui_channel();
            Ok::<_, String>(json!({ "ok": true }))
        });
    }
}
