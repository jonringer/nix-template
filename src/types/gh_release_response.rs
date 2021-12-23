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

pub type GhReleaseResponse = Vec<GhReleaseResponseElement>;

#[derive(Debug, Serialize, Deserialize)]
pub struct GhReleaseResponseElement {
    #[serde(rename = "url")]
    url: String,

    #[serde(rename = "assets_url")]
    assets_url: String,

    #[serde(rename = "upload_url")]
    upload_url: String,

    #[serde(rename = "html_url")]
    html_url: String,

    #[serde(rename = "id")]
    id: i64,

    #[serde(rename = "author")]
    author: Author,

    #[serde(rename = "node_id")]
    node_id: String,

    #[serde(rename = "tag_name")]
    pub tag_name: String,

    #[serde(rename = "target_commitish")]
    target_commitish: String,

    #[serde(rename = "draft")]
    draft: bool,

    #[serde(rename = "prerelease")]
    pub prerelease: bool,

    #[serde(rename = "created_at")]
    created_at: String,

    #[serde(rename = "published_at")]
    published_at: String,

    #[serde(rename = "assets")]
    assets: Vec<Asset>,

    #[serde(rename = "tarball_url")]
    tarball_url: String,

    #[serde(rename = "zipball_url")]
    zipball_url: String,

    #[serde(rename = "reactions")]
    reactions: Option<Reactions>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Asset {
    #[serde(rename = "url")]
    url: String,

    #[serde(rename = "id")]
    id: i64,

    #[serde(rename = "node_id")]
    node_id: String,

    #[serde(rename = "name")]
    name: String,

    #[serde(rename = "label")]
    label: Option<serde_json::Value>,

    #[serde(rename = "uploader")]
    uploader: Author,

    #[serde(rename = "content_type")]
    content_type: String,

    #[serde(rename = "state")]
    state: String,

    #[serde(rename = "size")]
    size: i64,

    #[serde(rename = "download_count")]
    download_count: i64,

    #[serde(rename = "created_at")]
    created_at: String,

    #[serde(rename = "updated_at")]
    updated_at: String,

    #[serde(rename = "browser_download_url")]
    browser_download_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Reactions {
    #[serde(rename = "url")]
    url: String,

    #[serde(rename = "total_count")]
    total_count: i64,

    #[serde(rename = "+1")]
    the_1: i64,

    #[serde(rename = "-1")]
    reactions_1: i64,

    #[serde(rename = "laugh")]
    laugh: i64,

    #[serde(rename = "hooray")]
    hooray: i64,

    #[serde(rename = "confused")]
    confused: i64,

    #[serde(rename = "heart")]
    heart: i64,

    #[serde(rename = "rocket")]
    rocket: i64,

    #[serde(rename = "eyes")]
    eyes: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Author {
    #[serde(rename = "login")]
    login: String,

    #[serde(rename = "id")]
    id: i64,

    #[serde(rename = "node_id")]
    node_id: String,

    #[serde(rename = "avatar_url")]
    avatar_url: String,

    #[serde(rename = "gravatar_id")]
    gravatar_id: String,

    #[serde(rename = "url")]
    url: String,

    #[serde(rename = "html_url")]
    html_url: String,

    #[serde(rename = "followers_url")]
    followers_url: String,

    #[serde(rename = "following_url")]
    following_url: String,

    #[serde(rename = "gists_url")]
    gists_url: String,

    #[serde(rename = "starred_url")]
    starred_url: String,

    #[serde(rename = "subscriptions_url")]
    subscriptions_url: String,

    #[serde(rename = "organizations_url")]
    organizations_url: String,

    #[serde(rename = "repos_url")]
    repos_url: String,

    #[serde(rename = "events_url")]
    events_url: String,

    #[serde(rename = "received_events_url")]
    received_events_url: String,

    #[serde(rename = "type")]
    author_type: String,

    #[serde(rename = "site_admin")]
    site_admin: bool,
}
