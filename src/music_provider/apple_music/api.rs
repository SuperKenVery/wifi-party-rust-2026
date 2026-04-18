//! Apple Music Web API client.
//!
//! All endpoints are public (no user account required) except for features we
//! don't use here (lyrics, aac-lc, etc.). The Bearer token is scraped from
//! the public `music.apple.com` JS bundle.

use anyhow::{Context, Result, anyhow, bail};
use regex::Regex;
use reqwest::Client;
use serde::Deserialize;

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36";

/// Fetch the anonymous Apple Music web bearer token.
///
/// Technique: GET `https://music.apple.com`, find the `/assets/index~*.js`
/// bundle, GET that, extract the first `eyJh...` JWT.
pub async fn get_token(client: &Client) -> Result<String> {
    let home = client
        .get("https://music.apple.com")
        .header("User-Agent", USER_AGENT)
        .send()
        .await
        .context("fetch music.apple.com")?
        .error_for_status()?
        .text()
        .await?;

    let index_re = Regex::new(r#"/assets/index-[^/\s"']+\.js"#)?;
    let fallback_re = Regex::new(r#"/assets/index~[^/\s"']+\.js"#)?;
    let index_path = index_re
        .find(&home)
        .or_else(|| fallback_re.find(&home))
        .ok_or_else(|| anyhow!("could not locate index JS in music.apple.com HTML"))?
        .as_str()
        .to_string();

    let js = client
        .get(format!("https://music.apple.com{index_path}"))
        .header("User-Agent", USER_AGENT)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    let tok_re = Regex::new(r#"eyJh[A-Za-z0-9._\-]+"#)?;
    let token = tok_re
        .find(&js)
        .ok_or_else(|| anyhow!("could not locate bearer token in {index_path}"))?
        .as_str()
        .to_string();

    Ok(token)
}

#[derive(Debug, Deserialize, Clone)]
pub struct SearchResponse {
    pub results: SearchResults,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct SearchResults {
    #[serde(default)]
    pub songs: Option<SongResults>,
    #[serde(default)]
    pub albums: Option<AlbumResults>,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct SongResults {
    #[serde(default)]
    pub data: Vec<SongData>,
    #[serde(default)]
    pub next: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct AlbumResults {
    #[serde(default)]
    pub data: Vec<AlbumData>,
    #[serde(default)]
    pub next: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SongData {
    pub id: String,
    pub attributes: SongAttributes,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SongAttributes {
    pub name: String,
    pub artist_name: String,
    #[serde(default)]
    pub album_name: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub duration_in_millis: Option<u64>,
    #[serde(default)]
    pub artwork: Option<Artwork>,
    #[serde(default)]
    pub extended_asset_urls: Option<ExtendedAssetUrls>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AlbumData {
    pub id: String,
    pub attributes: AlbumAttributes,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AlbumAttributes {
    pub name: String,
    pub artist_name: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub release_date: Option<String>,
    #[serde(default)]
    pub track_count: Option<u32>,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct Artwork {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
}

#[derive(Debug, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct ExtendedAssetUrls {
    #[serde(default)]
    pub enhanced_hls: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SongResp {
    data: Vec<SongData>,
}

/// Search the public catalog.
///
/// `types` is an Apple Music resource type like `"songs"` or `"albums"`.
pub async fn search(
    client: &Client,
    token: &str,
    storefront: &str,
    term: &str,
    types: &str,
    limit: u32,
    offset: u32,
) -> Result<SearchResponse> {
    let url = format!("https://amp-api.music.apple.com/v1/catalog/{storefront}/search");
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {token}"))
        .header("User-Agent", USER_AGENT)
        .header("Origin", "https://music.apple.com")
        .query(&[
            ("term", term),
            ("types", types),
            ("limit", &limit.to_string()),
            ("offset", &offset.to_string()),
            ("l", ""),
        ])
        .send()
        .await
        .context("search request")?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!("search API status {status}: {body}");
    }
    let parsed: SearchResponse = resp.json().await?;
    Ok(parsed)
}

/// Fetch full song info including the HLS master playlist URL
/// (`extended_asset_urls.enhanced_hls`).
pub async fn get_song(
    client: &Client,
    token: &str,
    storefront: &str,
    song_id: &str,
) -> Result<SongData> {
    let url = format!("https://amp-api.music.apple.com/v1/catalog/{storefront}/songs/{song_id}");
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {token}"))
        .header("User-Agent", USER_AGENT)
        .header("Origin", "https://music.apple.com")
        .query(&[
            ("include", "albums,artists"),
            ("extend", "extendedAssetUrls"),
            ("l", ""),
        ])
        .send()
        .await
        .context("song API request")?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!("song API status {status}: {body}");
    }
    let parsed: SongResp = resp.json().await?;
    parsed
        .data
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("empty song API response"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client() -> Client {
        Client::builder().build().unwrap()
    }

    #[tokio::test]
    async fn search_read_you_cn() {
        // Test that CN storefront search for "读你" returns results.
        let c = client();
        let token = get_token(&c).await.expect("token");
        assert!(token.starts_with("eyJh"), "token shape: {token}");

        let resp = search(&c, &token, "cn", "读你", "songs", 5, 0)
            .await
            .expect("search");
        let songs = resp
            .results
            .songs
            .expect("songs block present in search results");
        assert!(
            !songs.data.is_empty(),
            "expected at least one song for 读你"
        );
        println!("Found {} songs for 读你", songs.data.len());
        for s in &songs.data {
            println!(
                "  - {} / {} ({})",
                s.attributes.name, s.attributes.artist_name, s.attributes.url
            );
        }
    }
}
