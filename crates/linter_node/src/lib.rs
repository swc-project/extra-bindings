#[macro_use]
extern crate napi_derive;

mod util;

use std::{backtrace::Backtrace, env, panic::set_hook};

use anyhow::{bail, Context};
use napi::{bindgen_prelude::*, Task};
use serde::{Deserialize, Serialize};
use swc_common::{FileName, Mark, SyntaxContext};
use swc_ecma_ast::*;
use swc_ecma_lints::{config::LintConfig, rule::Rule, rules::LintParams};
use swc_ecma_parser::Syntax;
use swc_ecma_transforms_base::resolver;
use swc_ecma_visit::VisitMutWith;
use swc_nodejs_common::{deserialize_json, get_deserialized, MapErr};

use crate::util::try_with;

// parse it
// apply resolver
// apply lints, maybe in parallel
// emit diagnostics

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<Diagnostic>>,
}

struct LintTask {
    code: String,
    options: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LintOptions {
    #[serde(default)]
    filename: Option<String>,
    #[serde(default)]
    rules: LintConfig,
    #[serde(flatten)]
    pub syntax: Syntax,
    #[serde(default)]
    pub target: EsVersion,
}

#[napi]
impl Task for LintTask {
    type JsValue = TransformOutput;
    type Output = TransformOutput;

    fn compute(&mut self) -> napi::Result<Self::Output> {
        let opts = deserialize_json(&self.options)
            .context("failed to deserialize linter options")
            .convert_err()?;

        lint_inner(&self.code, opts).convert_err()
    }

    fn resolve(&mut self, _env: napi::Env, output: Self::Output) -> napi::Result<Self::JsValue> {
        Ok(output)
    }
}

fn lint_inner(code: &str, opts: LintOptions) -> anyhow::Result<TransformOutput> {
    try_with(|cm, handler| {
        let filename = match opts.filename {
            Some(v) => FileName::Real(v.into()),
            None => FileName::Anon,
        };

        let fm = cm.new_source_file(filename, code.into());

        let mut errors = vec![];

        let module = swc_ecma_parser::parse_file_as_module(
            &fm,
            Syntax::default(),
            opts.target,
            None,
            &mut errors,
        );

        let mut module = match module {
            Ok(module) => module,
            Err(err) => {
                err.into_diagnostic(handler).emit();

                for err in errors {
                    err.into_diagnostic(handler).emit();
                }

                bail!("Failed to parse input as module")
            }
        };

        let unresolved_mark = Mark::new();
        let top_level_mark = Mark::new();
        let unresolved_ctxt = SyntaxContext::empty().apply_mark(unresolved_mark);
        let top_level_ctxt = SyntaxContext::empty().apply_mark(top_level_mark);

        module.visit_mut_with(&mut resolver(unresolved_mark, top_level_mark, false));

        let mut rules = swc_ecma_lints::rules::all(LintParams {
            program: &Program::Module(module.clone()),
            lint_config: &opts.rules,
            unresolved_ctxt,
            top_level_ctxt,
            es_version: opts.target,
            source_map: cm.clone(),
        });

        rules.lint_module(&module);

        Ok(())
    })
    .convert_err()?;

    Ok(TransformOutput { errors: None })
}

#[allow(unused)]
#[napi]
fn lint(code: Buffer, opts: Buffer, signal: Option<AbortSignal>) -> AsyncTask<LintTask> {
    let code = String::from_utf8_lossy(code.as_ref()).to_string();
    let options = String::from_utf8_lossy(opts.as_ref()).to_string();

    let task = LintTask { code, options };

    AsyncTask::with_optional_signal(task, signal)
}

#[allow(unused)]
#[napi]
pub fn lint_sync(code: Buffer, opts: Buffer) -> napi::Result<TransformOutput> {
    let code = String::from_utf8_lossy(code.as_ref());
    let opts = get_deserialized(opts)?;

    lint_inner(&code, opts).convert_err()
}
