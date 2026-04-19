//! Minimal m3u8 parser for the Apple Music ALAC HLS feed.
//!
//! Apple's master playlist enumerates stream variants with `#EXT-X-STREAM-INF`
//! and nested `#EXT-X-MEDIA` alternates. Each ALAC variant's URI points at a
//! media playlist with one or more byterange segments, each carrying an
//! `#EXT-X-KEY:METHOD=SAMPLE-AES,URI="skd://..."` line.

use anyhow::{Context, Result, anyhow, bail};
use reqwest::Client;
use url::Url;

/// One selected stream variant from the master playlist.
#[derive(Debug, Clone)]
pub struct AlacVariant {
    /// Absolute URL of the variant's media playlist.
    pub media_uri: String,
    /// e.g. `"alac-stereo-44100-16"`
    pub audio_group: String,
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Bit depth (e.g. 16, 24).
    pub bit_depth: u32,
    /// Average bandwidth reported in the playlist.
    pub average_bandwidth: u64,
}

fn parse_attrs(s: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && (bytes[i] == b',' || bytes[i] == b' ') {
            i += 1;
        }
        let key_start = i;
        while i < bytes.len() && bytes[i] != b'=' {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let key = &s[key_start..i];
        i += 1;
        let val_start;
        let val_end;
        if i < bytes.len() && bytes[i] == b'"' {
            i += 1;
            val_start = i;
            while i < bytes.len() && bytes[i] != b'"' {
                i += 1;
            }
            val_end = i;
            if i < bytes.len() {
                i += 1;
            }
        } else {
            val_start = i;
            while i < bytes.len() && bytes[i] != b',' {
                i += 1;
            }
            val_end = i;
        }
        out.push((key.to_string(), s[val_start..val_end].to_string()));
    }
    out
}

/// Select the best ALAC variant with sample rate <= `max_sample_rate`.
pub async fn select_alac_variant(
    client: &Client,
    master_url: &str,
    max_sample_rate: u32,
) -> Result<AlacVariant> {
    let master = client
        .get(master_url)
        .send()
        .await
        .context("fetch master m3u8")?
        .error_for_status()?
        .text()
        .await?;

    select_alac_variant_from_text(&master, master_url, max_sample_rate)
}

fn select_alac_variant_from_text(
    master_text: &str,
    master_url: &str,
    max_sample_rate: u32,
) -> Result<AlacVariant> {
    let base = Url::parse(master_url).context("parse master URL")?;

    #[derive(Default)]
    struct Stream {
        codecs: String,
        audio_group: String,
        average_bandwidth: u64,
        uri: String,
    }

    // Collect EXT-X-MEDIA audio alternates: group-id -> uri (for when
    // the stream's variant URI is absent / we need the group's URI).
    let mut alternates: std::collections::HashMap<String, (String, String)> =
        std::collections::HashMap::new();

    let mut streams: Vec<Stream> = Vec::new();
    let mut pending_stream_attrs: Option<Vec<(String, String)>> = None;
    for line in master_text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("#EXT-X-MEDIA:") {
            let attrs = parse_attrs(rest);
            let mut type_ = String::new();
            let mut group = String::new();
            let mut name = String::new();
            let mut uri = String::new();
            for (k, v) in attrs {
                match k.as_str() {
                    "TYPE" => type_ = v,
                    "GROUP-ID" => group = v,
                    "NAME" => name = v,
                    "URI" => uri = v,
                    _ => {}
                }
            }
            if type_ == "AUDIO" && !group.is_empty() {
                alternates.insert(group, (name, uri));
            }
        } else if let Some(rest) = line.strip_prefix("#EXT-X-STREAM-INF:") {
            pending_stream_attrs = Some(parse_attrs(rest));
        } else if !line.starts_with('#') && !line.is_empty() {
            if let Some(attrs) = pending_stream_attrs.take() {
                let mut s = Stream::default();
                for (k, v) in attrs {
                    match k.as_str() {
                        "CODECS" => s.codecs = v,
                        "AUDIO" => s.audio_group = v,
                        "AVERAGE-BANDWIDTH" | "BANDWIDTH" => {
                            if s.average_bandwidth == 0 {
                                s.average_bandwidth = v.parse().unwrap_or(0);
                            }
                        }
                        _ => {}
                    }
                }
                s.uri = line.to_string();
                streams.push(s);
            }
        }
    }

    // Pick ALAC streams with sample rate <= max.
    let mut candidates: Vec<(AlacVariant, u64)> = Vec::new();
    for s in &streams {
        if !s.codecs.contains("alac") {
            continue;
        }
        // The audio group id is like "alac-stereo-44100-16". Parse.
        let parts: Vec<&str> = s.audio_group.split('-').collect();
        if parts.len() < 4 {
            continue;
        }
        let bit_depth: u32 = parts[parts.len() - 1].parse().unwrap_or(0);
        let sample_rate: u32 = parts[parts.len() - 2].parse().unwrap_or(0);
        if sample_rate == 0 || sample_rate > max_sample_rate {
            continue;
        }
        let variant_uri = base
            .join(&s.uri)
            .context("resolve variant URI")?
            .to_string();
        candidates.push((
            AlacVariant {
                media_uri: variant_uri,
                audio_group: s.audio_group.clone(),
                sample_rate,
                bit_depth,
                average_bandwidth: s.average_bandwidth,
            },
            s.average_bandwidth,
        ));
    }
    candidates.sort_by(|a, b| b.1.cmp(&a.1));
    candidates
        .into_iter()
        .next()
        .map(|(v, _)| v)
        .ok_or_else(|| anyhow!("no ALAC variant with sample rate <= {max_sample_rate} found"))
}

/// One media-playlist segment.
#[derive(Debug, Clone)]
pub struct MediaSegment {
    /// Absolute URL of the underlying file.
    pub uri: String,
    /// `#EXT-X-KEY:URI=` (e.g. `skd://...`). `None` means this segment has
    /// no encryption (effectively a passthrough).
    pub key_uri: Option<String>,
}

/// Parse an Apple Music media playlist. All segments typically share a single
/// file URI and differ only by `BYTERANGE`; we collapse them into a list in
/// document order so fragment index `i` lines up with media segment `i`.
pub async fn fetch_media_playlist(client: &Client, media_url: &str) -> Result<Vec<MediaSegment>> {
    let text = client
        .get(media_url)
        .send()
        .await
        .context("fetch media m3u8")?
        .error_for_status()?
        .text()
        .await?;
    parse_media_playlist(&text, media_url)
}

fn parse_media_playlist(text: &str, media_url: &str) -> Result<Vec<MediaSegment>> {
    let base = Url::parse(media_url).context("parse media URL")?;
    let mut segments = Vec::new();
    let mut current_key_uri: Option<String> = None;
    let mut has_extinf = false;

    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("#EXT-X-KEY:") {
            // Only honor "streamingkeydelivery" / SAMPLE-AES style keys
            // (the rest are Widevine/PlayReady which the wrapper does not speak).
            let attrs = parse_attrs(rest);
            let mut key_uri: Option<String> = None;
            let mut keyformat: Option<String> = None;
            let mut method: Option<String> = None;
            for (k, v) in attrs {
                match k.as_str() {
                    "URI" => key_uri = Some(v),
                    "KEYFORMAT" => keyformat = Some(v),
                    "METHOD" => method = Some(v),
                    _ => {}
                }
            }
            if method.as_deref() == Some("NONE") {
                current_key_uri = None;
            } else if keyformat.as_deref().map_or(true, |f| {
                f.contains("streamingkeydelivery") || f == "com.apple.streamingkeydelivery"
            }) {
                current_key_uri = key_uri;
            }
        } else if line.starts_with("#EXTINF:") {
            has_extinf = true;
        } else if !line.starts_with('#') && !line.is_empty() {
            let uri = base.join(line).context("resolve segment URI")?.to_string();
            segments.push(MediaSegment {
                uri,
                key_uri: current_key_uri.clone(),
            });
            has_extinf = false;
        }
    }
    // has_extinf only ever matters if nonzero segments; silence the unused
    // warning by ensuring the variable is exercised.
    let _ = has_extinf;

    if segments.is_empty() {
        bail!("media playlist has no segments");
    }
    Ok(segments)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_attrs_basic() {
        let v = parse_attrs(r#"BANDWIDTH=1234,CODECS="alac",AUDIO="alac-stereo-44100-16""#);
        assert_eq!(v[0], ("BANDWIDTH".into(), "1234".into()));
        assert_eq!(v[1], ("CODECS".into(), "alac".into()));
        assert_eq!(v[2], ("AUDIO".into(), "alac-stereo-44100-16".into()));
    }

    #[test]
    fn select_alac_variant_picks_highest_bitrate_under_limit() {
        let master = r#"#EXTM3U
#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID="alac-stereo-48000-24",NAME="alac",URI="alac48.m3u8"
#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID="alac-stereo-192000-24",NAME="alac",URI="alac192.m3u8"
#EXT-X-STREAM-INF:BANDWIDTH=1000000,CODECS="alac",AUDIO="alac-stereo-48000-24",AVERAGE-BANDWIDTH=1000000
alac48.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=4000000,CODECS="alac",AUDIO="alac-stereo-192000-24",AVERAGE-BANDWIDTH=4000000
alac192.m3u8
"#;
        let v = select_alac_variant_from_text(master, "https://x/y/master.m3u8", 96000).unwrap();
        assert_eq!(v.sample_rate, 48000);
        assert_eq!(v.bit_depth, 24);
        assert!(v.media_uri.ends_with("/alac48.m3u8"));
    }

    #[test]
    fn parse_media_playlist_basic() {
        let text = r#"#EXTM3U
#EXT-X-VERSION:6
#EXT-X-TARGETDURATION:12
#EXT-X-KEY:METHOD=SAMPLE-AES,URI="skd://itunes.apple.com/fps/mykey",KEYFORMAT="com.apple.streamingkeydelivery",KEYFORMATVERSIONS="1"
#EXTINF:10.0,
#EXT-X-BYTERANGE:12345@0
seg.mp4
#EXTINF:10.0,
#EXT-X-BYTERANGE:12345@12345
seg.mp4
#EXT-X-ENDLIST
"#;
        let segs = parse_media_playlist(text, "https://x/y/media.m3u8").unwrap();
        assert_eq!(segs.len(), 2);
        assert!(segs[0].uri.ends_with("/seg.mp4"));
        assert_eq!(
            segs[0].key_uri.as_deref(),
            Some("skd://itunes.apple.com/fps/mykey")
        );
        assert_eq!(segs[1].key_uri, segs[0].key_uri);
    }
}
