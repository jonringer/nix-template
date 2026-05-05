use serde::{Deserialize, Serialize};

/// GitLab Releases API response type
pub type GitlabReleaseResponse = Vec<GitlabReleaseElement>;

/// Single release from GitLab's /api/v4/projects/:id/releases endpoint
#[derive(Debug, Serialize, Deserialize)]
pub struct GitlabReleaseElement {
    #[serde(rename = "tag_name")]
    pub tag_name: String,

    #[serde(rename = "name")]
    pub name: Option<String>,

    #[serde(rename = "description")]
    pub description: Option<String>,

    #[serde(rename = "created_at")]
    pub created_at: String,

    #[serde(rename = "released_at")]
    pub released_at: String,

    #[serde(rename = "author")]
    pub author: Option<GitlabAuthor>,

    #[serde(rename = "commit")]
    pub commit: Option<GitlabCommit>,

    #[serde(rename = "assets")]
    pub assets: Option<GitlabAssets>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitlabAuthor {
    #[serde(rename = "name")]
    pub name: String,

    #[serde(rename = "username")]
    pub username: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitlabCommit {
    #[serde(rename = "id")]
    pub id: String,

    #[serde(rename = "short_id")]
    pub short_id: String,

    #[serde(rename = "title")]
    pub title: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitlabAssets {
    #[serde(rename = "count")]
    pub count: i64,

    #[serde(rename = "sources")]
    pub sources: Vec<GitlabAssetSource>,

    #[serde(rename = "links")]
    pub links: Vec<GitlabAssetLink>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitlabAssetSource {
    #[serde(rename = "format")]
    pub format: String,

    #[serde(rename = "url")]
    pub url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitlabAssetLink {
    #[serde(rename = "name")]
    pub name: String,

    #[serde(rename = "url")]
    pub url: String,

    #[serde(rename = "link_type")]
    pub link_type: Option<String>,
}

/// GitLab Repository Tags API response type
pub type GitlabTagsResponse = Vec<GitlabTagElement>;

/// Single tag from GitLab's /api/v4/projects/:id/repository/tags endpoint
#[derive(Debug, Serialize, Deserialize)]
pub struct GitlabTagElement {
    #[serde(rename = "name")]
    pub name: String,

    #[serde(rename = "message")]
    pub message: Option<String>,

    #[serde(rename = "target")]
    pub target: String,

    #[serde(rename = "commit")]
    pub commit: Option<GitlabCommit>,

    #[serde(rename = "release")]
    pub release: Option<serde_json::Value>,
}

/// GitLab Project API response
#[derive(Debug, Serialize, Deserialize)]
pub struct GitlabProjectResponse {
    #[serde(rename = "id")]
    pub id: i64,

    #[serde(rename = "description")]
    pub description: Option<String>,

    #[serde(rename = "name")]
    pub name: String,

    #[serde(rename = "path")]
    pub path: String,

    #[serde(rename = "path_with_namespace")]
    pub path_with_namespace: String,

    #[serde(rename = "web_url")]
    pub web_url: String,

    #[serde(rename = "http_url_to_repo")]
    pub http_url_to_repo: Option<String>,

    #[serde(rename = "ssh_url_to_repo")]
    pub ssh_url_to_repo: Option<String>,

    #[serde(rename = "default_branch")]
    pub default_branch: Option<String>,

    // License might be a nested object or null
    #[serde(rename = "license")]
    pub license: Option<GitlabLicense>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitlabLicense {
    #[serde(rename = "key")]
    pub key: String,

    #[serde(rename = "name")]
    pub name: String,

    #[serde(rename = "nickname")]
    pub nickname: Option<String>,
}
