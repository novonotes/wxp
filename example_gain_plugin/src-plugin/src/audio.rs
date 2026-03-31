use clack_extensions::params::PluginAudioProcessorParams;
use clack_plugin::prelude::*;
use clack_plugin::process::audio::{ChannelPair, SampleType};

use crate::params::{apply_host_parameter_events, drain_ui_events};
use crate::plugin::{SharedState, WxpExampleGainMainThread};

pub(crate) struct WxpExampleGainAudioProcessor<'a> {
    shared: &'a SharedState,
}

impl<'a> PluginAudioProcessor<'a, SharedState, WxpExampleGainMainThread<'a>>
    for WxpExampleGainAudioProcessor<'a>
{
    fn activate(
        _host: HostAudioProcessorHandle<'a>,
        _main_thread: &mut WxpExampleGainMainThread<'a>,
        shared: &'a SharedState,
        _audio_config: PluginAudioConfiguration,
    ) -> Result<Self, PluginError> {
        Ok(Self { shared })
    }

    fn deactivate(self, _main_thread: &mut WxpExampleGainMainThread<'a>) {}

    fn process(
        &mut self,
        _process: Process,
        mut audio: Audio,
        events: Events,
    ) -> Result<ProcessStatus, PluginError> {
        drain_ui_events(&self.shared.inner, events.output);
        apply_host_parameter_events(&self.shared.inner, events.input);

        let gain = self.shared.inner.gain();
        let Some(mut port_pair) = audio.port_pair(0) else {
            return Ok(ProcessStatus::ContinueIfNotQuiet);
        };

        match port_pair.channels()? {
            SampleType::F32(mut channels) => process_channels_f32(&mut channels, gain),
            SampleType::F64(mut channels) => process_channels_f64(&mut channels, gain as f64),
            SampleType::Both(_, mut channels) => process_channels_f64(&mut channels, gain as f64),
        }

        Ok(ProcessStatus::ContinueIfNotQuiet)
    }
}

impl PluginAudioProcessorParams for WxpExampleGainAudioProcessor<'_> {
    fn flush(
        &mut self,
        input_parameter_changes: &InputEvents,
        output_parameter_changes: &mut OutputEvents,
    ) {
        drain_ui_events(&self.shared.inner, output_parameter_changes);
        apply_host_parameter_events(&self.shared.inner, input_parameter_changes);
    }
}

fn process_channels_f32(
    channels: &mut clack_plugin::process::audio::PairedChannels<'_, f32>,
    gain: f32,
) {
    for pair in channels.iter_mut() {
        match pair {
            ChannelPair::InputOnly(_) => {}
            ChannelPair::OutputOnly(output) => output.fill(0.0),
            ChannelPair::InputOutput(input, output) => {
                for (src, dst) in input.iter().zip(output.iter_mut()) {
                    *dst = *src * gain;
                }
            }
            ChannelPair::InPlace(buffer) => {
                for sample in buffer.iter_mut() {
                    *sample *= gain;
                }
            }
        }
    }
}

fn process_channels_f64(
    channels: &mut clack_plugin::process::audio::PairedChannels<'_, f64>,
    gain: f64,
) {
    for pair in channels.iter_mut() {
        match pair {
            ChannelPair::InputOnly(_) => {}
            ChannelPair::OutputOnly(output) => output.fill(0.0),
            ChannelPair::InputOutput(input, output) => {
                for (src, dst) in input.iter().zip(output.iter_mut()) {
                    *dst = *src * gain;
                }
            }
            ChannelPair::InPlace(buffer) => {
                for sample in buffer.iter_mut() {
                    *sample *= gain;
                }
            }
        }
    }
}
