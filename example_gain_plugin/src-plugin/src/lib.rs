mod audio;
mod gui;
mod params;
mod plugin;

use std::ffi::CStr;

use clack_plugin::{clack_export_entry, entry::prelude::*};
use plugin::WxpExampleGainPluginFactory;

pub struct WxpExampleGainEntry {
    plugin_factory: PluginFactoryWrapper<WxpExampleGainPluginFactory>,
}

impl Entry for WxpExampleGainEntry {
    fn new(_bundle_path: &CStr) -> Result<Self, EntryLoadError> {
        novonotes_run_loop::RunLoop::init().map_err(|_| EntryLoadError)?;

        Ok(Self {
            plugin_factory: PluginFactoryWrapper::new(WxpExampleGainPluginFactory::new()),
        })
    }

    fn declare_factories<'a>(&'a self, builder: &mut EntryFactories<'a>) {
        builder.register_factory(&self.plugin_factory);
    }
}

impl Drop for WxpExampleGainEntry {
    fn drop(&mut self) {
        novonotes_run_loop::RunLoop::deinit();
    }
}

clack_export_entry!(WxpExampleGainEntry);

#[unsafe(no_mangle)]
pub extern "C" fn get_clap_entry() -> EntryDescriptor {
    clap_entry
}
