use serde::Serialize;

pub struct Analyzer {
    pub deps: Dependencies,
}

#[derive(Debug, Serialize)]
pub struct Dependencies {
    pub imports: Vec<Import>,
    pub urls: Vec<CssUrl>,
}

#[derive(Debug, Serialize)]
pub struct Import {}

#[derive(Debug, Serialize)]
pub struct CssUrl {}
