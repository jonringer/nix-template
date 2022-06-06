// Example code that deserializes and serializes the model.
// extern crate serde;
// #[macro_use]
// extern crate serde_derive;
// extern crate serde_json;
//
// use generated_module::[object Object];
//
// fn main() {
//     let json = r#"{"answer": 42}"#;
//     let model: [object Object] = serde_json::from_str(&json).unwrap();
// }

use serde;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
pub struct PypiResponse {
    #[serde(rename = "info")]
    pub info: Info,

    #[serde(rename = "last_serial")]
    last_serial: i64,

    #[serde(rename = "releases")]
    pub releases: HashMap<String, Vec<Url>>,

    #[serde(rename = "urls")]
    urls: Vec<Url>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Info {
    #[serde(rename = "author")]
    author: String,

    #[serde(rename = "author_email")]
    author_email: String,

    #[serde(rename = "bugtrack_url")]
    bugtrack_url: Option<serde_json::Value>,

    #[serde(rename = "classifiers")]
    classifiers: Vec<String>,

    #[serde(rename = "description")]
    description: String,

    #[serde(rename = "description_content_type")]
    description_content_type: String,

    #[serde(rename = "docs_url")]
    docs_url: Option<serde_json::Value>,

    #[serde(rename = "download_url")]
    download_url: String,

    #[serde(rename = "home_page")]
    pub home_page: String,

    #[serde(rename = "license")]
    pub license: String,

    #[serde(rename = "name")]
    name: String,

    #[serde(rename = "package_url")]
    package_url: String,

    #[serde(rename = "platform")]
    platform: Option<String>,

    #[serde(rename = "project_url")]
    project_url: String,

    #[serde(rename = "project_urls")]
    project_urls: Option<ProjectUrls>,

    #[serde(rename = "release_url")]
    release_url: String,

    #[serde(rename = "requires_dist")]
    requires_dist: Option<Vec<String>>,

    #[serde(rename = "requires_python")]
    requires_python: String,

    #[serde(rename = "summary")]
    pub summary: String,

    #[serde(rename = "version")]
    version: String,

    #[serde(rename = "yanked")]
    yanked: bool,

    #[serde(rename = "yanked_reason")]
    yanked_reason: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectUrls {
    #[serde(rename = "Bug Tracker")]
    bug_tracker: Option<String>,

    #[serde(rename = "Documentation")]
    documentation: Option<String>,

    #[serde(rename = "Homepage")]
    homepage: String,

    #[serde(rename = "Source Code")]
    source_code: Option<String>,

    #[serde(rename = "Source")]
    source: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Url {
    #[serde(rename = "comment_text")]
    comment_text: String,

    #[serde(rename = "digests")]
    pub digests: Digests,

    #[serde(rename = "downloads")]
    downloads: i64,

    #[serde(rename = "filename")]
    filename: String,

    #[serde(rename = "has_sig")]
    has_sig: bool,

    #[serde(rename = "md5_digest")]
    md5_digest: String,

    #[serde(rename = "packagetype")]
    pub packagetype: String,

    #[serde(rename = "python_version")]
    python_version: String,

    #[serde(rename = "requires_python")]
    requires_python: Option<String>,

    #[serde(rename = "size")]
    size: i64,

    #[serde(rename = "upload_time")]
    upload_time: String,

    #[serde(rename = "upload_time_iso_8601")]
    upload_time_iso_8601: String,

    #[serde(rename = "url")]
    url: String,

    #[serde(rename = "yanked")]
    yanked: bool,

    #[serde(rename = "yanked_reason")]
    yanked_reason: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Digests {
    #[serde(rename = "md5")]
    md5: String,

    #[serde(rename = "sha256")]
    pub sha256: String,
}
