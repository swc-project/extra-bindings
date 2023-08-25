#[macro_use]
extern crate napi_derive;

mod util;

use std::{backtrace::Backtrace, env, panic::set_hook};

use napi::{
    bindgen_prelude::{AbortSignal, AsyncTask, Buffer},
    Task,
};
use swc_common::plugin::{
    diagnostics::PluginCorePkgDiagnostics, serialized::PluginSerializedBytes,
};

#[napi::module_init]
fn init() {
    if cfg!(debug_assertions) || env::var("SWC_DEBUG").unwrap_or_default() == "1" {
        set_hook(Box::new(|panic_info| {
            let backtrace = Backtrace::force_capture();
            println!("Panic: {:?}\nBacktrace: {:?}", panic_info, backtrace);
        }));
    }
}

#[napi(object)]
pub struct PluginVersionInfo {
    pub pkg_version: String,
    pub git_sha: String,
    pub cargo_features: String,
    pub ast_schema_version: u32,
}

struct GetPluginVersionTask {
    wasm: Vec<u8>,
    options: String,
}

fn invoke_get_version(
    wasm_bytes: &[u8],
) -> anyhow::Result<PluginSerializedBytes<PluginCorePkgDiagnostics>> {
}

#[napi]
impl Task for GetPluginVersionTask {
    type JsValue = PluginVersionInfo;
    type Output = PluginVersionInfo;

    fn compute(&mut self) -> napi::Result<Self::Output> {
        let result = invoke_get_version(&self.wasm).map_err(|err| {
            napi::Error::new(
                napi::Status::GenericFailure,
                format!("Failed to get plugin version: {:?}", err),
            )
        })?;
    }

    fn resolve(&mut self, _env: napi::Env, output: Self::Output) -> napi::Result<Self::JsValue> {
        Ok(output)
    }
}

#[allow(unused)]
#[napi]
pub fn get_plugin_version(
    wasm: Buffer,
    opts: Buffer,
    signal: Option<AbortSignal>,
) -> AsyncTask<GetPluginVersionTask> {
    let options = String::from_utf8_lossy(opts.as_ref()).to_string();

    let task = GetPluginVersionTask {
        wasm: wasm.as_ref().to_vec(),
        options,
    };

    AsyncTask::with_optional_signal(task, signal)
}
