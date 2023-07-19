use serde::Serialize;
use swc_css_ast::{ImportHref, ImportPrelude, Url};
use swc_css_visit::{Visit, VisitWith};

pub struct Analyzer {
    pub deps: Dependencies,
}

#[derive(Debug, Serialize)]
pub struct Dependencies {
    pub imports: Vec<Import>,
    pub urls: Vec<CssUrl>,
}

#[derive(Debug, Serialize)]
pub struct Import {
    pub url: CssUrl,
}

#[derive(Debug, Serialize)]
pub struct CssUrl {}

impl Visit for Analyzer {
    fn visit_import_prelude(&mut self, n: &ImportPrelude) {
        n.layer_name.visit_with(self);
        n.import_conditions.visit_with(self);

        self.deps.imports.push(Import {
            url: normalize_import_href(&n.href),
        });
    }

    fn visit_url(&mut self, n: &Url) {
        self.deps.urls.push(normalize_url(n));
    }
}

fn normalize_import_href(n: &ImportHref) -> CssUrl {}

fn normalize_url(n: &Url) -> CssUrl {}
