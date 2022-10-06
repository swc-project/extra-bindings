#![feature(backtrace)]

#[macro_use]
extern crate napi_derive;

mod util;

use std::{backtrace::Backtrace, env, panic::set_hook};

use anyhow::{bail, Context};
use napi::{bindgen_prelude::*, JsString, Task};
use serde::Deserialize;
use swc_cached::regex::CachedRegex;
use swc_common::FileName;
use swc_html::{
    codegen::{
        writer::basic::{BasicHtmlWriter, BasicHtmlWriterConfig},
        CodeGenerator, CodegenConfig, Emit,
    },
    parser::parse_file_as_document,
};
use swc_html_minifier::{
    minify_document,
    option::{
        CollapseWhitespaces, MinifierType, MinifyCssOption, MinifyJsOption, MinifyJsonOption,
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

struct MinifyTask {
    code: String,
    options: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MinifyOptions {
    #[serde(default)]
    filename: Option<String>,

    // Parser options
    #[serde(default)]
    iframe_srcdoc: bool,
    #[serde(default)]
    scripting_enabled: bool,

    // Minification options
    #[serde(default)]
    force_set_html5_doctype: bool,
    #[serde(default = "default_collapse_whitespaces")]
    collapse_whitespaces: CollapseWhitespaces,
    // Remove safe empty elements with metadata content, i.e. the `script` and `style` element
    // without content and attributes, `meta` and `link` elements without attributes and etc
    #[serde(default = "true_by_default")]
    remove_empty_metadata_elements: bool,
    #[serde(default = "true_by_default")]
    remove_comments: bool,
    #[serde(default = "default_preserve_comments")]
    preserve_comments: Option<Vec<CachedRegex>>,
    #[serde(default = "true_by_default")]
    minify_conditional_comments: bool,
    #[serde(default = "true_by_default")]
    remove_empty_attributes: bool,
    #[serde(default = "true_by_default")]
    remove_redundant_attributes: bool,
    #[serde(default = "true_by_default")]
    collapse_boolean_attributes: bool,
    #[serde(default = "true_by_default")]
    normalize_attributes: bool,
    #[serde(default = "minify_json_by_default")]
    minify_json: MinifyJsonOption,
    #[serde(default = "minify_js_by_default")]
    minify_js: MinifyJsOption,
    #[serde(default = "minify_css_by_default")]
    minify_css: MinifyCssOption,
    #[serde(default)]
    minify_additional_scripts_content: Option<Vec<(CachedRegex, MinifierType)>>,
    #[serde(default)]
    minify_additional_attributes: Option<Vec<(CachedRegex, MinifierType)>>,
    #[serde(default = "true_by_default")]
    sort_space_separated_attribute_values: bool,
    #[serde(default)]
    sort_attributes: bool,

    // Codegen options
    #[serde(default)]
    tag_omission: Option<bool>,
    #[serde(default)]
    self_closing_void_elements: Option<bool>,
    #[serde(default)]
    quotes: Option<bool>,
}

const fn true_by_default() -> bool {
    true
}

const fn minify_json_by_default() -> MinifyJsonOption {
    MinifyJsonOption::Bool(true)
}

const fn minify_js_by_default() -> MinifyJsOption {
    MinifyJsOption::Bool(true)
}

const fn minify_css_by_default() -> MinifyCssOption {
    MinifyCssOption::Bool(true)
}

fn default_preserve_comments() -> Option<Vec<CachedRegex>> {
    Some(vec![
        // License comments
        CachedRegex::new("@preserve").unwrap(),
        CachedRegex::new("@copyright").unwrap(),
        CachedRegex::new("@lic").unwrap(),
        CachedRegex::new("@cc_on").unwrap(),
        // Allow to keep custom comments
        CachedRegex::new("^!").unwrap(),
        // Server-side comments
        CachedRegex::new("^\\s*#").unwrap(),
        // Conditional IE comments
        CachedRegex::new("^\\[if\\s[^\\]+]").unwrap(),
        CachedRegex::new("\\[endif]").unwrap(),
    ])
}

const fn default_collapse_whitespaces() -> CollapseWhitespaces {
    CollapseWhitespaces::OnlyMetadata
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
        env.create_string(&output)
    }
}

fn minify_inner(code: &str, opts: MinifyOptions) -> anyhow::Result<String> {
    try_with(|cm, handler| {
        let filename = match opts.filename {
            Some(v) => FileName::Real(v.into()),
            None => FileName::Anon,
        };

        let fm = cm.new_source_file(filename, code.into());

        let scripting_enabled = opts.scripting_enabled;
        let mut errors = vec![];
        let doc = parse_file_as_document(
            &fm,
            swc_html::parser::parser::ParserConfig {
                scripting_enabled,
                iframe_srcdoc: opts.iframe_srcdoc,
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

        let options = swc_html_minifier::option::MinifyOptions {
            force_set_html5_doctype: opts.force_set_html5_doctype,
            collapse_whitespaces: opts.collapse_whitespaces,
            remove_empty_metadata_elements: opts.remove_empty_metadata_elements,
            remove_comments: opts.remove_comments,
            preserve_comments: opts.preserve_comments,
            minify_conditional_comments: opts.minify_conditional_comments,
            remove_empty_attributes: opts.remove_empty_attributes,
            remove_redundant_attributes: opts.remove_redundant_attributes,
            collapse_boolean_attributes: opts.collapse_boolean_attributes,
            normalize_attributes: opts.normalize_attributes,
            minify_json: opts.minify_json,
            minify_js: opts.minify_js,
            minify_css: opts.minify_css,
            minify_additional_scripts_content: opts.minify_additional_scripts_content,
            minify_additional_attributes: opts.minify_additional_attributes,
            sort_space_separated_attribute_values: opts.sort_space_separated_attribute_values,
            sort_attributes: opts.sort_attributes,
        };

        minify_document(&mut doc, &options);

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
                        minify: true,
                        scripting_enabled,
                        tag_omission: opts.tag_omission,
                        self_closing_void_elements: opts.self_closing_void_elements,
                        quotes: opts.quotes,
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
