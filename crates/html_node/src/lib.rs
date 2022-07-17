#![feature(backtrace)]

#[macro_use]
extern crate napi_derive;

mod util;

use std::{backtrace::Backtrace, env, panic::set_hook};

use anyhow::{bail, Context};
use napi::{bindgen_prelude::*, JsString, Task};
use serde::{Deserialize, Serialize};
use swc_common::FileName;
use swc_html::{
    codegen::{
        writer::basic::{BasicHtmlWriter, BasicHtmlWriterConfig},
        CodeGenerator, CodegenConfig, Emit,
    },
    parser::parse_file_as_document,
};
use swc_html_minifier::minify_document;
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

struct MinifyTask {
    code: String,
    options: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MinifyOptions {
    #[serde(default)]
    filename: Option<String>,

    #[serde(default)]
    source_map: bool,
}

#[napi]
impl Task for MinifyTask {
    type JsValue = JsString;
    type Output = String;

    fn compute(&mut self) -> napi::Result<Self::Output> {
        let opts = deserialize_json(&self.options)
            .context("failed to deserialize minifier options")
            .convert_err()?;

        minify_inner(&self.code, opts).convert_err()
    }

    fn resolve(&mut self, env: napi::Env, output: Self::Output) -> napi::Result<Self::JsValue> {
        Ok(env.create_string(&output)?)
    }
}

fn minify_inner(code: &str, opts: MinifyOptions) -> anyhow::Result<String> {
    try_with(|cm, handler| {
        let filename = match opts.filename {
            Some(v) => FileName::Real(v.into()),
            None => FileName::Anon,
        };

        let fm = cm.new_source_file(filename, code.into());

        let mut errors = vec![];
        let doc = parse_file_as_document(
            &fm,
            swc_html::parser::parser::ParserConfig {
                ..Default::default()
            },
            &mut errors,
        );

        let mut doc = match doc {
            Ok(v) => v,
            Err(err) => {
                err.to_diagnostics(handler).emit();

                for err in errors {
                    err.to_diagnostics(handler).emit();
                }

                bail!("failed to parse input as stylesheet")
            }
        };

        if !errors.is_empty() {
            for err in errors {
                err.to_diagnostics(handler).emit();
            }
            bail!("failed to parse input as stylesheet (recovered)")
        }

        minify_document(&mut doc, &Default::default());

        let code = {
            let mut buf = String::new();
            {
                let mut wr = BasicHtmlWriter::new(
                    &mut buf,
                    None,
                    BasicHtmlWriterConfig {
                        ..Default::default()
                    },
                );
                let mut gen = CodeGenerator::new(
                    &mut wr,
                    CodegenConfig {
                        ..Default::default()
                    },
                );

                gen.emit(&doc).context("failed to emit")?;
            }

            buf
        };

        Ok(code)
    })
}

#[allow(unused)]
#[napi]
fn minify(code: Buffer, opts: Buffer, signal: Option<AbortSignal>) -> AsyncTask<MinifyTask> {
    swc_nodejs_common::init_default_trace_subscriber();
    let code = String::from_utf8_lossy(code.as_ref()).to_string();
    let options = String::from_utf8_lossy(opts.as_ref()).to_string();

    let task = MinifyTask { code, options };

    AsyncTask::with_optional_signal(task, signal)
}

#[allow(unused)]
#[napi]
pub fn minify_sync(code: Buffer, opts: Buffer) -> napi::Result<String> {
    swc_nodejs_common::init_default_trace_subscriber();
    let code = String::from_utf8_lossy(code.as_ref()).to_string();
    let opts = get_deserialized(opts)?;

    minify_inner(&code, opts).convert_err()
}
