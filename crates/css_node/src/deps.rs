use serde::Serialize;
use swc_atoms::JsWord;
use swc_common::Spanned;
use swc_css_ast::{ImportHref, ImportPrelude, Url, UrlValue};
use swc_css_codegen::{
    writer::basic::{BasicCssWriter, BasicCssWriterConfig, IndentType, LineFeed},
    CodeGenerator, CodegenConfig, Emit,
};
use swc_css_visit::{Visit, VisitWith};

#[derive(Default)]
pub struct Analyzer {
    pub deps: Dependencies,
}

#[derive(Debug, Default, Serialize)]
pub struct Dependencies {
    pub imports: Vec<Import>,
    pub urls: Vec<CssUrl>,
}

#[derive(Debug, Serialize)]
pub struct Import {
    pub url: CssUrl,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct CssUrl {
    pub value: JsWord,
}

impl Visit for Analyzer {
    fn visit_import_prelude(&mut self, n: &ImportPrelude) {
        n.layer_name.visit_with(self);
        n.import_conditions.visit_with(self);

        let url = normalize_import_href(&n.href);

        if let Some(url) = url {
            self.deps.imports.push(Import {
                url,
                supports: n
                    .import_conditions
                    .as_deref()
                    .and_then(|n| n.supports.as_deref())
                    .map(print_node),
                layer: n.layer_name.as_deref().map(print_node),
                media: n
                    .import_conditions
                    .as_deref()
                    .and_then(|n| n.media.as_deref())
                    .map(|n| n.queries.iter().map(print_node).collect()),
            });
        }
    }

    fn visit_url(&mut self, n: &Url) {
        self.deps.urls.extend(normalize_url(n));
    }
}

fn normalize_import_href(n: &ImportHref) -> Option<CssUrl> {
    match n {
        ImportHref::Url(n) => normalize_url(n),
        ImportHref::Str(n) => Some(parse_url(&n.value)),
    }
}

fn normalize_url(n: &Url) -> Option<CssUrl> {
    let v = n.value.as_deref()?;

    Some(match v {
        UrlValue::Str(v) => parse_url(&v.value),
        UrlValue::Raw(v) => parse_url(&v.value),
    })
}

fn parse_url(s: &JsWord) -> CssUrl {
    CssUrl { value: s.clone() }
}

fn print_node<N>(n: N) -> String
where
    N: Spanned,
    for<'a> CodeGenerator<BasicCssWriter<'a, &'a mut String>>: Emit<N>,
{
    let mut buf = String::new();
    let wr = BasicCssWriter::new(
        &mut buf,
        None,
        BasicCssWriterConfig {
            indent_type: IndentType::Space,
            indent_width: 0,
            linefeed: LineFeed::LF,
        },
    );
    let mut gen = CodeGenerator::new(wr, CodegenConfig { minify: true });

    gen.emit(&n)
        .expect("failed to print node for dependency analysis");

    buf
}
