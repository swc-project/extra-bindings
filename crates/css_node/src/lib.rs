#[macro_use]
extern crate napi_derive;

use std::{
    backtrace::Backtrace, collections::HashMap, env, fmt::Write, panic::set_hook, sync::Arc,
};

use anyhow::{bail, Context};
use napi::{bindgen_prelude::*, Task};
use serde::{Deserialize, Serialize};
use swc_atoms::JsWord;
use swc_common::FileName;
use swc_css_codegen::{
    writer::basic::{BasicCssWriter, BasicCssWriterConfig, IndentType, LineFeed},
    CodeGenerator, CodegenConfig, Emit,
};
use swc_css_compat::{
    compiler::{Compiler, Config},
    feature::Features,
};
use swc_css_visit::{VisitMutWith, VisitWith};
use swc_nodejs_common::{deserialize_json, get_deserialized, MapErr};

use crate::util::try_with;

mod deps;
mod util;

#[napi::module_init]
fn init() {
    if cfg!(debug_assertions) || env::var("SWC_DEBUG").unwrap_or_default() == "1" {
        set_hook(Box::new(|panic_info| {
            let backtrace = Backtrace::force_capture();
            println!("Panic: {:?}\nBacktrace: {:?}", panic_info, backtrace);
        }));
    }
}

#[napi_derive::napi(object)]
#[derive(Debug, Serialize)]
pub struct Diagnostic {
    pub level: String,
    pub message: String,
    pub span: serde_json::Value,
}

#[napi_derive::napi(object)]
#[derive(Debug, Serialize)]
pub struct TransformOutput {
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<Diagnostic>>,

    /// JSON string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deps: Option<String>,

    /// JSON string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modules_mapping: Option<String>,
}

struct MinifyTask {
    code: String,
    options: String,
}

struct TransformTask {
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransformOptions {
    #[serde(default)]
    filename: Option<String>,

    #[serde(default)]
    source_map: bool,

    #[serde(default)]
    css_modules: Option<CssModulesConfig>,

    #[serde(default)]
    minify: bool,

    #[serde(default)]
    analyze_dependencies: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CssModulesConfig {
    pattern: String,
}

#[derive(Debug)]
struct CssModuleTransformConfig {
    file_name: Arc<FileName>,
    file_name_hash: u8,
    pattern: Vec<CssClassNameSegment>,
}

#[derive(Debug)]
enum CssClassNameSegment {
    /// A literal string segment.
    Literal(JsWord),
    /// The base file name.
    Name,
    /// The original class name.
    Local,
    /// A hash of the file name.
    Hash,
}

#[derive(Serialize)]
#[serde(tag = "type")]
pub enum CssClassName {
    Local { name: JsWord },
    Global { name: JsWord },
    Import { name: JsWord, from: JsWord },
}

impl swc_css_modules::TransformConfig for CssModuleTransformConfig {
    fn new_name_for(&self, local: &JsWord) -> JsWord {
        let mut buf = String::new();

        for segment in &self.pattern {
            match segment {
                CssClassNameSegment::Literal(s) => buf.push_str(s),
                CssClassNameSegment::Name => match &*self.file_name {
                    FileName::Real(f) => {
                        write!(buf, "{}", f.file_stem().unwrap().to_str().unwrap()).unwrap();
                    }
                    FileName::Anon => buf.push_str("[anon]"),
                    _ => {
                        unreachable!("CssModuleTransformConfig::new_name_for: invalid file name")
                    }
                },
                CssClassNameSegment::Local => buf.push_str(local),
                CssClassNameSegment::Hash => {
                    write!(buf, "{:x}", self.file_name_hash).unwrap();
                }
            }
        }

        buf.into()
    }
}

impl CssModulesConfig {
    /// Adapted from lightningcss
    fn parse_pattern(&self) -> anyhow::Result<Vec<CssClassNameSegment>> {
        let mut res = Vec::with_capacity(2);

        let mut idx = 0;

        let mut s = &*self.pattern;

        while !s.is_empty() {
            if s.starts_with('[') {
                if let Some(end_idx) = s.find(']') {
                    let segment = match &s[0..=end_idx] {
                        "[name]" => CssClassNameSegment::Name,
                        "[local]" => CssClassNameSegment::Local,
                        "[hash]" => CssClassNameSegment::Hash,
                        s => {
                            bail!(
                                "Unknown placeholder {} at {} in CSS Modules pattern: {}",
                                s,
                                idx,
                                self.pattern
                            )
                        }
                    };
                    res.push(segment);
                    idx += end_idx + 1;
                    s = &s[end_idx + 1..];
                } else {
                    bail!(
                        "Unclosed brackets at {} in CSS Modules pattern: {}",
                        idx,
                        self.pattern
                    )
                }
            } else {
                let end_idx = s.find('[').unwrap_or(s.len());
                res.push(CssClassNameSegment::Literal(s[0..end_idx].into()));
                idx += end_idx;
                s = &s[end_idx..];
            }
        }

        Ok(res)
    }
}

#[napi]
impl Task for TransformTask {
    type JsValue = TransformOutput;
    type Output = TransformOutput;

    fn compute(&mut self) -> napi::Result<Self::Output> {
        let opts = deserialize_json(&self.options)
            .context("failed to deserialize transform options")
            .convert_err()?;

        transform_inner(&self.code, opts).convert_err()
    }

    fn resolve(&mut self, _env: napi::Env, output: Self::Output) -> napi::Result<Self::JsValue> {
        Ok(output)
    }
}

#[napi]
impl Task for MinifyTask {
    type JsValue = TransformOutput;
    type Output = TransformOutput;

    fn compute(&mut self) -> napi::Result<Self::Output> {
        let opts = deserialize_json(&self.options)
            .context("failed to deserialize minifier options")
            .convert_err()?;

        minify_inner(&self.code, opts).convert_err()
    }

    fn resolve(&mut self, _env: napi::Env, output: Self::Output) -> napi::Result<Self::JsValue> {
        Ok(output)
    }
}

fn minify_inner(code: &str, opts: MinifyOptions) -> anyhow::Result<TransformOutput> {
    swc_common::GLOBALS.set(&swc_common::Globals::new(), || {
        try_with(|cm, handler| {
            let filename = match opts.filename {
                Some(v) => FileName::Real(v.into()),
                None => FileName::Anon,
            };

            let fm = cm.new_source_file(filename, code.into());

            let mut errors = vec![];
            let ss = swc_css_parser::parse_file::<swc_css_ast::Stylesheet>(
                &fm,
                None,
                swc_css_parser::parser::ParserConfig {
                    allow_wrong_line_comments: false,
                    css_modules: false,
                    legacy_nesting: false,
                    legacy_ie: false,
                },
                &mut errors,
            );

            let mut ss = match ss {
                Ok(v) => v,
                Err(err) => {
                    err.to_diagnostics(handler).emit();

                    for err in errors {
                        err.to_diagnostics(handler).emit();
                    }

                    bail!("failed to parse input as stylesheet")
                }
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

            swc_css_minifier::minify(&mut ss, Default::default());

            let mut src_map = vec![];
            let code = {
                let mut buf = String::new();
                {
                    let wr = BasicCssWriter::new(
                        &mut buf,
                        if opts.source_map {
                            Some(&mut src_map)
                        } else {
                            None
                        },
                        BasicCssWriterConfig {
                            indent_type: IndentType::Space,
                            indent_width: 0,
                            linefeed: LineFeed::LF,
                        },
                    );
                    let mut gen = CodeGenerator::new(wr, CodegenConfig { minify: true });

                    gen.emit(&ss).context("failed to emit")?;
                }

                buf
            };

            let map = if opts.source_map {
                let map = cm.build_source_map(&src_map);
                let mut buf = vec![];
                map.to_writer(&mut buf)
                    .context("failed to generate sourcemap")?;
                Some(String::from_utf8(buf).context("the generated source map is not utf8")?)
            } else {
                None
            };

            Ok(TransformOutput {
                code,
                map,
                errors: returned_errors,
                deps: Default::default(),
                modules_mapping: Default::default(),
            })
        })
    })
}

fn transform_inner(code: &str, opts: TransformOptions) -> anyhow::Result<TransformOutput> {
    try_with(|cm, handler| {
        let filename = match opts.filename {
            Some(v) => FileName::Real(v.into()),
            None => FileName::Anon,
        };

        let fm = cm.new_source_file(filename, code.into());

        let mut errors = vec![];
        let ss = swc_css_parser::parse_file::<swc_css_ast::Stylesheet>(
            &fm,
            None,
            swc_css_parser::parser::ParserConfig {
                allow_wrong_line_comments: false,
                css_modules: opts.css_modules.is_some(),
                legacy_nesting: false,
                legacy_ie: false,
            },
            &mut errors,
        );

        let mut ss = match ss {
            Ok(v) => v,
            Err(err) => {
                err.to_diagnostics(handler).emit();

                for err in errors {
                    err.to_diagnostics(handler).emit();
                }

                bail!("failed to parse input as stylesheet")
            }
        };

        let deps = if opts.analyze_dependencies {
            let mut v = deps::Analyzer::default();

            ss.visit_with(&mut v);

            Some(v.deps)
        } else {
            None
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

        let modules_mapping = if let Some(config) = opts.css_modules {
            let result = swc_css_modules::compile(
                &mut ss,
                CssModuleTransformConfig {
                    file_name: Arc::new(fm.name.clone()),
                    file_name_hash: fm.name_hash as _,
                    pattern: config
                        .parse_pattern()
                        .context("failed to parse the pattern for CSS Modules")?,
                },
            );
            let map: HashMap<_, _> = result
                .renamed
                .into_iter()
                .map(|(k, v)| {
                    (
                        k,
                        v.into_iter()
                            .map(|v| match v {
                                swc_css_modules::CssClassName::Local { name } => {
                                    CssClassName::Local { name: name.value }
                                }
                                swc_css_modules::CssClassName::Global { name } => {
                                    CssClassName::Global { name: name.value }
                                }
                                swc_css_modules::CssClassName::Import { name, from } => {
                                    CssClassName::Import {
                                        name: name.value,
                                        from,
                                    }
                                }
                            })
                            .collect::<Vec<_>>(),
                    )
                })
                .collect();
            Some(
                serde_json::to_string(&map)
                    .context("failed to serialize the mapping for CSS Modules")?,
            )
        } else {
            None
        };

        ss.visit_mut_with(&mut Compiler::new(Config {
            // TODO: preset-env
            process: Features::all(),
        }));

        let mut src_map = vec![];
        let code = {
            let mut buf = String::new();
            {
                let wr = BasicCssWriter::new(
                    &mut buf,
                    if opts.source_map {
                        Some(&mut src_map)
                    } else {
                        None
                    },
                    if opts.minify {
                        BasicCssWriterConfig {
                            indent_type: IndentType::Space,
                            indent_width: 0,
                            linefeed: LineFeed::LF,
                        }
                    } else {
                        BasicCssWriterConfig::default()
                    },
                );
                let mut gen = CodeGenerator::new(
                    wr,
                    CodegenConfig {
                        minify: opts.minify,
                    },
                );

                gen.emit(&ss).context("failed to emit")?;
            }

            buf
        };

        let map = if opts.source_map {
            let map = cm.build_source_map(&src_map);
            let mut buf = vec![];
            map.to_writer(&mut buf)
                .context("failed to generate sourcemap")?;
            Some(String::from_utf8(buf).context("the generated source map is not utf8")?)
        } else {
            None
        };

        Ok(TransformOutput {
            code,
            map,
            errors: returned_errors,
            deps: deps.map(|v| serde_json::to_string(&v).unwrap()),
            modules_mapping,
        })
    })
}

#[allow(unused)]
#[napi]
fn minify(code: Buffer, opts: Buffer, signal: Option<AbortSignal>) -> AsyncTask<MinifyTask> {
    let code = String::from_utf8_lossy(code.as_ref()).to_string();
    let options = String::from_utf8_lossy(opts.as_ref()).to_string();

    let task = MinifyTask { code, options };

    AsyncTask::with_optional_signal(task, signal)
}

#[allow(unused)]
#[napi]
pub fn minify_sync(code: Buffer, opts: Buffer) -> napi::Result<TransformOutput> {
    let code = String::from_utf8_lossy(code.as_ref());
    let opts = get_deserialized(opts)?;

    minify_inner(&code, opts).convert_err()
}

#[allow(unused)]
#[napi]
fn transform(code: Buffer, opts: Buffer, signal: Option<AbortSignal>) -> AsyncTask<TransformTask> {
    let code = String::from_utf8_lossy(code.as_ref()).to_string();
    let options = String::from_utf8_lossy(opts.as_ref()).to_string();

    let task = TransformTask { code, options };

    AsyncTask::with_optional_signal(task, signal)
}

#[allow(unused)]
#[napi]
pub fn transform_sync(code: Buffer, opts: Buffer) -> napi::Result<TransformOutput> {
    let code = String::from_utf8_lossy(code.as_ref());
    let opts = get_deserialized(opts)?;

    transform_inner(&code, opts).convert_err()
}
