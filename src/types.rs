use clap::arg_enum;

arg_enum! {
    #[allow(non_camel_case_types)]
    #[derive(Debug,PartialEq)]
    pub enum Template {
        stdenv,
        python,
        mkshell,
        //Go,
        //Rust,
        //Qt,
    }
}

arg_enum! {
    #[allow(non_camel_case_types)]
    #[derive(Debug)]
    pub enum Fetcher {
        github,
        gitlab,
        url,
        zip,
        pypi,
    }
}

#[derive(Debug)]
pub struct ExpressionInfo {
    pub pname: String,
    pub version: String,
    pub license: String,
    pub maintainer: String,
    pub fetcher: Fetcher,
    pub template: Template,
    pub path_to_write: std::path::PathBuf,
    pub top_level_path: std::path::PathBuf,
}
