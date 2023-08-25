#[macro_use]
extern crate napi_derive;

mod util;

use std::{backtrace::Backtrace, env, panic::set_hook};

use anyhow::{bail, Context};
use napi::{bindgen_prelude::*, Task};
use serde::{Deserialize, Serialize};
use swc_atoms::js_word;
use swc_cached::regex::CachedRegex;
use swc_common::{FileName, DUMMY_SP};
use swc_html::{
    ast::{DocumentMode, Namespace},
    codegen::{
        writer::basic::{BasicHtmlWriter, BasicHtmlWriterConfig},
        CodeGenerator, CodegenConfig, Emit,
    },
    parser::{parse_file_as_document, parse_file_as_document_fragment},
};
use swc_html_ast::{Document, DocumentFragment};
use swc_html_minifier::{
    minify_document, minify_document_fragment,
    option::{
        CollapseWhitespaces, MinifierType, MinifyCssOption, MinifyJsOption, MinifyJsonOption,
        RemoveRedundantAttributes,
    },
};
use swc_nodejs_common::{deserialize_json, get_deserialized, MapErr};

use crate::util::try_with;

#[napi::module_init]
fn init() {
    if cfg!(debug_assertions) || env::var("SWC_DEBUG").unwrap_or_default() == "1" {
        set_hook(Box::new(|panic_info| {
            let backtrace = Backtrace::force_capture();
            println!("Panic: {:?}\nBacktrace: {:?}", panic_info, backtrace);
        }));
    }
}

pub struct PluginVersionInfo {}

struct GetPluginVersionTask {
    wasm: Vec<u8>,
    options: String,
}

#[napi]
impl Task for GetPluginVersionTask {
    type JsValue = PluginVersionInfo;
    type Output = PluginVersionInfo;

    fn compute(&mut self) -> napi::Result<Self::Output> {
        let result: PluginSerializedBytes<PluginCorePkgDiagnostics> = {};
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
) -> napi::Result<TransformOutput> {
    let code = String::from_utf8_lossy(code.as_ref()).to_string();
    let options = String::from_utf8_lossy(opts.as_ref()).to_string();

    let task = MinifyTask {
        code,
        options,
        is_fragment: true,
    };

    AsyncTask::with_optional_signal(task, signal)
}
