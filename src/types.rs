use clap::arg_enum;

arg_enum! {
    #[allow(non_camel_case_types)]
    #[derive(Debug)]
    pub enum Template {
        stdenv,
        python,
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

