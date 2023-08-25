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

enum DocumentOrDocumentFragment {
    Document(Document),
    DocumentFragment(DocumentFragment),
}

fn create_namespace(namespace: &str) -> anyhow::Result<Namespace> {
    match &*namespace.to_lowercase() {
        "http://www.w3.org/1999/xhtml" => Ok(Namespace::HTML),
        "http://www.w3.org/1998/math/mathml" => Ok(Namespace::MATHML),
        "http://www.w3.org/2000/svg" => Ok(Namespace::SVG),
        "http://www.w3.org/1999/xlink" => Ok(Namespace::XLINK),
        "http://www.w3.org/xml/1998/namespace" => Ok(Namespace::XML),
        "http://www.w3.org/2000/xmlns/" => Ok(Namespace::XMLNS),
        _ => {
            bail!("failed to parse namespace of context element")
        }
    }
}

fn create_element(context_element: Element) -> anyhow::Result<swc_html_ast::Element> {
    let mut attributes = Vec::with_capacity(context_element.attributes.len());

    for attribute in context_element.attributes.into_iter() {
        let namespace = match attribute.namespace {
            Some(namespace) => Some(create_namespace(&namespace)?),
            _ => None,
        };

        attributes.push(swc_html_ast::Attribute {
            span: DUMMY_SP,
            namespace,
            prefix: attribute.prefix.map(|value| value.into()),
            name: attribute.name.into(),
            raw_name: None,
            value: attribute.value.map(|value| value.into()),
            raw_value: None,
        })
    }

    Ok(swc_html_ast::Element {
        span: DUMMY_SP,
        tag_name: context_element.tag_name.into(),
        namespace: create_namespace(&context_element.namespace)?,
        attributes,
        children: vec![],
        content: None,
        is_self_closing: context_element.is_self_closing,
    })
}

fn minify_inner(
    code: &str,
    opts: MinifyOptions,
    is_fragment: bool,
) -> anyhow::Result<TransformOutput> {
    swc_common::GLOBALS.set(&swc_common::Globals::new(), || {
        try_with(|cm, handler| {
            let filename = match opts.filename {
                Some(v) => FileName::Real(v.into()),
                None => FileName::Anon,
            };

            let fm = cm.new_source_file(filename, code.into());

            let scripting_enabled = opts.scripting_enabled;
            let mut errors = vec![];

            let (mut document_or_document_fragment, context_element) = if is_fragment {
                let context_element = match opts.context_element {
                    Some(context_element) => create_element(context_element)?,
                    _ => swc_html_ast::Element {
                        span: DUMMY_SP,
                        tag_name: js_word!("template"),
                        namespace: Namespace::HTML,
                        attributes: vec![],
                        children: vec![],
                        content: None,
                        is_self_closing: false,
                    },
                };
                let mode = match opts.mode {
                    Some(mode) => mode,
                    _ => DocumentMode::NoQuirks,
                };
                let form_element = match opts.form_element {
                    Some(form_element) => Some(create_element(form_element)?),
                    _ => None,
                };
                let document_fragment = parse_file_as_document_fragment(
                    &fm,
                    &context_element,
                    mode,
                    form_element.as_ref(),
                    swc_html::parser::parser::ParserConfig {
                        scripting_enabled,
                        iframe_srcdoc: opts.iframe_srcdoc,
                    },
                    &mut errors,
                );

                let document_fragment = match document_fragment {
                    Ok(v) => v,
                    Err(err) => {
                        err.to_diagnostics(handler).emit();

                        for err in errors {
                            err.to_diagnostics(handler).emit();
                        }

                        bail!("failed to parse input as document fragment")
                    }
                };

                (
                    DocumentOrDocumentFragment::DocumentFragment(document_fragment),
                    Some(context_element),
                )
            } else {
                let document = parse_file_as_document(
                    &fm,
                    swc_html::parser::parser::ParserConfig {
                        scripting_enabled,
                        iframe_srcdoc: opts.iframe_srcdoc,
                    },
                    &mut errors,
                );

                let document = match document {
                    Ok(v) => v,
                    Err(err) => {
                        err.to_diagnostics(handler).emit();

                        for err in errors {
                            err.to_diagnostics(handler).emit();
                        }

                        bail!("failed to parse input as document")
                    }
                };

                (DocumentOrDocumentFragment::Document(document), None)
            };

            let mut returned_errors = None;

            if !errors.is_empty() {
                returned_errors = Some(Vec::with_capacity(errors.len()));

                for err in errors {
                    let mut buf = vec![];

                    err.to_diagnostics(handler).buffer(&mut buf);

                    for i in buf {
                        returned_errors.as_mut().unwrap().push(Diagnostic {
                            level: i.level.to_string(),
                            message: i.message(),
                            span: serde_json::to_value(&i.span)?,
                        });
                    }
                }
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
                merge_metadata_elements: opts.merge_metadata_elements,
            };

            match document_or_document_fragment {
                DocumentOrDocumentFragment::Document(ref mut document) => {
                    minify_document(document, &options);
                }
                DocumentOrDocumentFragment::DocumentFragment(ref mut document_fragment) => {
                    minify_document_fragment(
                        document_fragment,
                        context_element.as_ref().unwrap(),
                        &options,
                    );
                }
            }

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
                            context_element: context_element.as_ref(),
                            tag_omission: opts.tag_omission,
                            self_closing_void_elements: opts.self_closing_void_elements,
                            quotes: opts.quotes,
                        },
                    );

                    match document_or_document_fragment {
                        DocumentOrDocumentFragment::Document(document) => {
                            gen.emit(&document).context("failed to emit")?;
                        }
                        DocumentOrDocumentFragment::DocumentFragment(document_fragment) => {
                            gen.emit(&document_fragment).context("failed to emit")?;
                        }
                    }
                }

                buf
            };

            Ok(TransformOutput {
                code,
                errors: returned_errors,
            })
        })
    })
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
