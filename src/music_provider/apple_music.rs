use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use dioxus::prelude::*;
use serde::Deserialize;
use tracing::error;

use crate::music_provider::MusicProvider;
use crate::state::AppState;

// ── Constants ────────────────────────────────────────────────────────

const STOREFRONT: &str = "cn";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36";
const DECRYPT_ADDR: &str = "100.69.234.108:10020";
const PREFETCH_KEY: &str = "skd://itunes.apple.com/P000000000/s1/e1";

// ── API response types ──────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct SearchResponse {
    results: SearchResults,
}

#[derive(Debug, Deserialize)]
struct SearchResults {
    songs: Option<SongResults>,
}

#[derive(Debug, Deserialize)]
struct SongResults {
    data: Vec<SongData>,
}

#[derive(Debug, Clone, Deserialize)]
struct SongData {
    id: String,
    attributes: SongAttributes,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct SongAttributes {
    name: String,
    artist_name: String,
    album_name: String,
    artwork: Artwork,
    duration_in_millis: Option<u64>,
    #[serde(default)]
    previews: Vec<Preview>,
    extended_asset_urls: Option<ExtendedAssetUrls>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct Artwork {
    url: String,
    width: Option<u32>,
    height: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct Preview {
    url: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExtendedAssetUrls {
    enhanced_hls: Option<String>,
}

// ── Token fetching ──────────────────────────────────────────────────

pub(crate) async fn fetch_bearer_token() -> Result<String> {
    let client = reqwest::Client::new();

    let html = client
        .get("https://music.apple.com")
        .header("User-Agent", USER_AGENT)
        .send()
        .await?
        .text()
        .await?;

    let index_js_re = regex::Regex::new(r"/assets/index~[^/]+\.js")?;
    let index_js_path = index_js_re
        .find(&html)
        .context("Could not find index JS path in Apple Music HTML")?
        .as_str();

    let js_url = format!("https://music.apple.com{}", index_js_path);
    let js_body = client
        .get(&js_url)
        .header("User-Agent", USER_AGENT)
        .send()
        .await?
        .text()
        .await?;

    let token_re = regex::Regex::new(r#"eyJh([^"]*)"#)?;
    let token = token_re
        .find(&js_body)
        .context("Could not find bearer token in Apple Music JS")?
        .as_str()
        .to_string();

    Ok(token)
}

// ── Search API ──────────────────────────────────────────────────────

pub(crate) async fn search_songs(token: &str, query: &str, limit: u32) -> Result<Vec<SongData>> {
    let client = reqwest::Client::new();
    let url = format!(
        "https://amp-api.music.apple.com/v1/catalog/{}/search",
        STOREFRONT
    );

    let resp: SearchResponse = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .header("User-Agent", USER_AGENT)
        .header("Origin", "https://music.apple.com")
        .query(&[
            ("term", query),
            ("types", "songs"),
            ("limit", &limit.to_string()),
        ])
        .send()
        .await?
        .json()
        .await?;

    Ok(resp
        .results
        .songs
        .map(|s| s.data)
        .unwrap_or_default())
}

// ── Song details (get m3u8 URL) ─────────────────────────────────────

pub(crate) async fn get_song_enhanced_hls(token: &str, song_id: &str) -> Result<String> {
    let client = reqwest::Client::new();
    let url = format!(
        "https://amp-api.music.apple.com/v1/catalog/{}/songs/{}",
        STOREFRONT, song_id
    );

    let resp: SongResponse = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .header("User-Agent", USER_AGENT)
        .header("Origin", "https://music.apple.com")
        .query(&[
            ("include", "albums,artists"),
            ("extend", "extendedAssetUrls"),
        ])
        .send()
        .await?
        .json()
        .await?;

    let song = resp.data.into_iter().next().context("No song data returned")?;
    let enhanced_hls = song
        .attributes
        .extended_asset_urls
        .and_then(|u| u.enhanced_hls)
        .context("No enhancedHls URL available for this song")?;

    Ok(enhanced_hls)
}

#[derive(Deserialize)]
struct SongResponse {
    data: Vec<SongData>,
}

// ── m3u8 parsing ────────────────────────────────────────────────────

#[derive(Debug)]
#[allow(dead_code)]
struct M3u8Variant {
    uri: String,
    codecs: String,
    audio: String,
    bandwidth: u64,
}

#[derive(Debug)]
struct M3u8KeyInfo {
    uri: String,
}

#[derive(Debug)]
#[allow(dead_code)]
struct M3u8Segment {
    byte_range_length: u64,
    byte_range_offset: u64,
    key: Option<M3u8KeyInfo>,
}

/// Parse a master m3u8 playlist and return all variants.
fn parse_master_m3u8(body: &str) -> Vec<M3u8Variant> {
    let mut variants = Vec::new();
    let lines: Vec<&str> = body.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if line.starts_with("#EXT-X-STREAM-INF:") {
            let attrs = &line["#EXT-X-STREAM-INF:".len()..];
            let codecs = extract_attr(attrs, "CODECS").unwrap_or_default();
            let audio = extract_attr(attrs, "AUDIO").unwrap_or_default();
            let bandwidth = extract_attr(attrs, "AVERAGE-BANDWIDTH")
                .or_else(|| extract_attr(attrs, "BANDWIDTH"))
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0);
            if i + 1 < lines.len() {
                let uri = lines[i + 1].trim().to_string();
                if !uri.starts_with('#') {
                    variants.push(M3u8Variant {
                        uri,
                        codecs,
                        audio,
                        bandwidth,
                    });
                    i += 2;
                    continue;
                }
            }
        }
        i += 1;
    }
    variants
}

/// Parse a media m3u8 playlist and return key info + segments.
/// A segment's `key` field is Some only when a NEW key tag appeared for that segment.
fn parse_media_m3u8(body: &str) -> Vec<M3u8Segment> {
    let mut segments = Vec::new();
    let mut pending_key_change: Option<M3u8KeyInfo> = None;
    let mut pending_byte_range: Option<(u64, u64)> = None;
    let mut running_offset: u64 = 0;

    for line in body.lines() {
        let line = line.trim();
        if line.starts_with("#EXT-X-KEY:") {
            let attrs = &line["#EXT-X-KEY:".len()..];
            let key_format = extract_attr(attrs, "KEYFORMAT").unwrap_or_default();
            if key_format.contains("streamingkeydelivery") {
                if let Some(uri) = extract_attr(attrs, "URI") {
                    pending_key_change = Some(M3u8KeyInfo { uri });
                }
            }
        } else if line.starts_with("#EXT-X-BYTERANGE:") {
            let range_str = &line["#EXT-X-BYTERANGE:".len()..];
            if let Some((len_str, off_str)) = range_str.split_once('@') {
                let length: u64 = len_str.parse().unwrap_or(0);
                let offset: u64 = off_str.parse().unwrap_or(0);
                pending_byte_range = Some((length, offset));
                running_offset = offset + length;
            } else if let Ok(length) = range_str.parse::<u64>() {
                pending_byte_range = Some((length, running_offset));
                running_offset += length;
            }
        } else if !line.starts_with('#') && !line.is_empty() {
            if let Some((length, offset)) = pending_byte_range.take() {
                segments.push(M3u8Segment {
                    byte_range_length: length,
                    byte_range_offset: offset,
                    key: pending_key_change.take(),
                });
            }
        }
    }
    segments
}

/// Extract an attribute value from an m3u8 attribute string.
fn extract_attr(attrs: &str, key: &str) -> Option<String> {
    let search = format!("{}=", key);
    let pos = attrs.find(&search)?;
    let rest = &attrs[pos + search.len()..];
    if rest.starts_with('"') {
        let end = rest[1..].find('"')?;
        Some(rest[1..1 + end].to_string())
    } else {
        let end = rest.find(',').unwrap_or(rest.len());
        Some(rest[..end].to_string())
    }
}

fn resolve_url(base: &str, relative: &str) -> String {
    if relative.starts_with("http://") || relative.starts_with("https://") {
        return relative.to_string();
    }
    if let Some(base_dir) = base.rsplit_once('/') {
        format!("{}/{}", base_dir.0, relative)
    } else {
        relative.to_string()
    }
}

// ── fMP4 download and decrypt ───────────────────────────────────────

/// Select the best audio variant from the master playlist.
fn select_best_variant(variants: &[M3u8Variant]) -> Option<&M3u8Variant> {
    // Prefer ALAC (lossless), then AAC
    let mut alac: Vec<&M3u8Variant> = variants
        .iter()
        .filter(|v| v.codecs == "alac")
        .collect();
    alac.sort_by(|a, b| b.bandwidth.cmp(&a.bandwidth));
    if let Some(v) = alac.first() {
        return Some(v);
    }

    let mut aac: Vec<&M3u8Variant> = variants
        .iter()
        .filter(|v| v.codecs.starts_with("mp4a"))
        .collect();
    aac.sort_by(|a, b| b.bandwidth.cmp(&a.bandwidth));
    aac.first().copied()
}

/// Download the entire MP4 file as bytes.
async fn download_mp4_data(media_playlist_url: &str, body: &str) -> Result<(Vec<u8>, String)> {
    let segments = parse_media_m3u8(body);
    if segments.is_empty() {
        bail!("No segments found in media playlist");
    }

    // All segments reference the same URI in Apple Music fMP4 playlists.
    // The segment URI is found by parsing the non-comment, non-empty lines.
    let segment_uri = body
        .lines()
        .filter(|l| {
            let l = l.trim();
            !l.is_empty() && !l.starts_with('#')
        })
        .next()
        .context("No segment URI found")?;

    let file_url = resolve_url(media_playlist_url, segment_uri.trim());

    let client = reqwest::Client::new();
    let data = client
        .get(&file_url)
        .header("User-Agent", USER_AGENT)
        .send()
        .await?
        .bytes()
        .await?
        .to_vec();

    Ok((data, file_url))
}

/// Download and decrypt a song, returning the decrypted m4a bytes.
///
/// The full flow:
/// 1. Fetch master m3u8 from enhancedHls URL
/// 2. Select best quality variant
/// 3. Fetch media m3u8 for that variant
/// 4. Download the fMP4 file
/// 5. Parse init segment + fragments
/// 6. For each fragment, decrypt samples via TCP to the wrapper server
/// 7. Reassemble into a complete m4a file
pub(crate) async fn download_and_decrypt(
    enhanced_hls_url: &str,
    adam_id: &str,
) -> Result<Vec<u8>> {
    let client = reqwest::Client::new();

    // Step 1: Fetch master m3u8
    let master_body = client
        .get(enhanced_hls_url)
        .header("User-Agent", USER_AGENT)
        .send()
        .await?
        .text()
        .await?;

    // Step 2: Select best variant
    let variants = parse_master_m3u8(&master_body);
    let variant = select_best_variant(&variants)
        .context("No suitable audio variant found in master playlist")?;

    let media_url = resolve_url(enhanced_hls_url, &variant.uri);

    // Step 3: Fetch media m3u8
    let media_body = client
        .get(&media_url)
        .header("User-Agent", USER_AGENT)
        .send()
        .await?
        .text()
        .await?;

    let segments = parse_media_m3u8(&media_body);
    if segments.is_empty() {
        bail!("No segments in media playlist");
    }

    // Step 4: Download the fMP4 data
    let (mp4_data, _file_url) = download_mp4_data(&media_url, &media_body).await?;

    // Step 5-7: Parse and decrypt using mp4-atom, communicating with the decrypt server
    let decrypted = decrypt_fmp4(&mp4_data, adam_id, &segments)?;

    Ok(decrypted)
}

// ── MP4 parsing and decryption (using mp4-atom) ─────────────────────

/// Decryption info extracted from the init segment's tenc box.
#[allow(dead_code)]
struct DecryptInfo {
    default_is_protected: u8,
    default_per_sample_iv_size: u8,
    default_crypt_byte_block: u8,
    default_skip_byte_block: u8,
}

/// Parse the init segment and fragments, decrypt via TCP, and return the
/// complete decrypted m4a bytes.
fn decrypt_fmp4(data: &[u8], adam_id: &str, segments: &[M3u8Segment]) -> Result<Vec<u8>> {
    use std::io::Cursor;

    let mut cursor = Cursor::new(data);
    let total_len = data.len() as u64;

    // We'll work with raw MP4 boxes.
    // Init segment = ftyp + moov boxes
    // Fragments = (moof + mdat) pairs

    // Read all top-level boxes
    let mut boxes = Vec::new();
    while cursor.position() < total_len {
        let box_start = cursor.position() as usize;
        let remaining = &data[box_start..];
        if remaining.len() < 8 {
            break;
        }
        let box_size = u32::from_be_bytes([remaining[0], remaining[1], remaining[2], remaining[3]]) as u64;
        let box_type = &remaining[4..8];
        let box_type_str = std::str::from_utf8(box_type).unwrap_or("????").to_string();

        let actual_size = if box_size == 1 {
            // 64-bit extended size
            if remaining.len() < 16 {
                break;
            }
            u64::from_be_bytes([
                remaining[8], remaining[9], remaining[10], remaining[11],
                remaining[12], remaining[13], remaining[14], remaining[15],
            ])
        } else if box_size == 0 {
            // box extends to end of file
            total_len - box_start as u64
        } else {
            box_size
        };

        if box_start as u64 + actual_size > total_len {
            break;
        }

        boxes.push((box_start, actual_size as usize, box_type_str));
        cursor.set_position(box_start as u64 + actual_size);
    }

    // Separate init (ftyp+moov) from fragments (moof+mdat pairs)
    let mut init_end = 0;
    for (start, size, btype) in &boxes {
        if btype == "ftyp" || btype == "moov" {
            init_end = start + size;
        } else {
            break;
        }
    }

    tracing::debug!(
        "fMP4 top-level boxes: {:?}, init_end={}",
        boxes.iter().map(|(s, sz, t)| format!("{}@{}-{}", t, s, s+sz)).collect::<Vec<_>>(),
        init_end
    );
    dump_box_tree(&data[..init_end], 0, 0);

    // Dump all tenc boxes
    let mut search_pos = 0;
    while search_pos < init_end {
        if let Some(pos) = find_box(&data[search_pos..init_end], b"tenc") {
            let abs_pos = search_pos + pos;
            let tenc_data = &data[abs_pos..];
            let tsize = u32::from_be_bytes([tenc_data[0], tenc_data[1], tenc_data[2], tenc_data[3]]) as usize;
            tracing::debug!(
                "tenc at {}: version={}, crypt/skip={:02x}, isProtected={}, ivSize={}, raw={:02x?}",
                abs_pos,
                tenc_data[8],
                tenc_data[13],
                tenc_data[14],
                tenc_data[15],
                &tenc_data[8..std::cmp::min(tsize, 32)]
            );
            search_pos = abs_pos + tsize;
        } else {
            break;
        }
    }

    // Extract tenc info from moov for decryption parameters
    let decrypt_info = extract_tenc_info(&data[..init_end])?;

    #[cfg(test)]
    {
        std::fs::write("/tmp/attention_raw.mp4", data).ok();
        std::fs::write("/tmp/attention_init.mp4", &data[..init_end]).ok();
        let sanitized_init = sanitize_init_segment(&data[..init_end])?;
        std::fs::write("/tmp/attention_init_clean.mp4", &sanitized_init).ok();
    }

    // Group remaining boxes into fragments: each fragment is moof + mdat
    // Also handle emsg and prft boxes that may appear between fragments
    let mut fragment_groups: Vec<(usize, usize)> = Vec::new(); // (start, end) of each moof+mdat group
    let mut frag_start: Option<usize> = None;
    for (start, size, btype) in &boxes {
        if *start < init_end {
            continue;
        }
        match btype.as_str() {
            "moof" => {
                frag_start = Some(*start);
            }
            "mdat" => {
                if let Some(fs) = frag_start {
                    fragment_groups.push((fs, start + size));
                    frag_start = None;
                }
            }
            _ => {}
        }
    }

    // Connect to decrypt server
    let mut conn = TcpStream::connect(DECRYPT_ADDR)
        .context("Failed to connect to decrypt server")?;

    // Build output: init segment (with encryption boxes removed from moov) + decrypted fragments
    let mut output = Vec::with_capacity(data.len());

    // Write sanitized init segment
    let sanitized_init = sanitize_init_segment(&data[..init_end])?;
    output.extend_from_slice(&sanitized_init);

    // Process each fragment
    tracing::debug!("segment count: {}, fragment count: {}", segments.len(), fragment_groups.len());

    for (seg_idx, (frag_start, frag_end)) in fragment_groups.iter().enumerate() {
        let frag_data = &data[*frag_start..*frag_end];

        let segment = segments.get(seg_idx);

        if let Some(seg) = segment {
            if let Some(key) = &seg.key {
                if seg_idx > 0 {
                    conn.write_all(&[0u8; 4])?;
                }
                let effective_adam_id = if key.uri == PREFETCH_KEY { "0" } else { adam_id };
                tracing::debug!("seg {}: key change, adamId={}", seg_idx, effective_adam_id);
                send_string(&mut conn, effective_adam_id)?;
                send_string(&mut conn, &key.uri)?;
            }
        }

        let decrypted_frag =
            decrypt_fragment(frag_data, &mut conn, &decrypt_info)?;
        tracing::debug!(
            "seg {}: frag {}..{} -> {} bytes decrypted",
            seg_idx, frag_start, frag_end, decrypted_frag.len()
        );
        output.extend_from_slice(&decrypted_frag);
    }

    // Close the decrypt connection: send 5 zero bytes
    conn.write_all(&[0u8; 5])?;

    Ok(output)
}

fn send_string(conn: &mut TcpStream, s: &str) -> Result<()> {
    let bytes = s.as_bytes();
    conn.write_all(&[bytes.len() as u8])?;
    conn.write_all(bytes)?;
    Ok(())
}

fn dump_box_tree(data: &[u8], offset: usize, depth: usize) {
    let mut pos = offset;
    while pos + 8 <= data.len() {
        let size = u32::from_be_bytes([data[pos], data[pos+1], data[pos+2], data[pos+3]]) as usize;
        let btype = std::str::from_utf8(&data[pos+4..pos+8]).unwrap_or("????");
        if size < 8 || pos + size > data.len() {
            break;
        }
        let indent = "  ".repeat(depth);
        tracing::debug!("{}[{}] size={} offset={}", indent, btype, size, pos);
        let child_off = box_child_offset(&data[pos+4..pos+8]);
        if child_off > 0 && pos + child_off < pos + size {
            dump_box_tree(data, pos + child_off, depth + 1);
        }
        pos += size;
    }
}

/// Extract tenc box info from init segment data.
fn extract_tenc_info(init_data: &[u8]) -> Result<DecryptInfo> {
    // Search for 'tenc' box in the init segment
    // tenc box structure (after box header):
    //   version (1 byte) + flags (3 bytes) + reserved (1 byte) +
    //   if version >= 1: default_crypt_byte_block (4 bits) + default_skip_byte_block (4 bits)
    //   else: reserved (1 byte)
    //   default_isProtected (1 byte) + default_Per_Sample_IV_Size (1 byte) + default_KID (16 bytes)

    let tenc_pos = find_box(init_data, b"tenc")
        .context("No tenc box found in init segment")?;

    let tenc_data = &init_data[tenc_pos..];
    let box_size = u32::from_be_bytes([tenc_data[0], tenc_data[1], tenc_data[2], tenc_data[3]]) as usize;
    if box_size < 8 + 6 {
        bail!("tenc box too small");
    }

    // Full box: version(1) + flags(3)
    let version = tenc_data[8];
    let (crypt_byte_block, skip_byte_block) = if version >= 1 {
        let byte = tenc_data[13];
        ((byte >> 4) & 0x0F, byte & 0x0F)
    } else {
        (0, 0)
    };

    let default_is_protected = tenc_data[14];
    let default_per_sample_iv_size = tenc_data[15];

    Ok(DecryptInfo {
        default_is_protected,
        default_per_sample_iv_size,
        default_crypt_byte_block: crypt_byte_block,
        default_skip_byte_block: skip_byte_block,
    })
}

/// Find a box by its 4CC type in raw MP4 data.
/// Returns the offset of the box start.
fn find_box(data: &[u8], box_type: &[u8; 4]) -> Option<usize> {
    let mut pos = 0;
    while pos + 8 <= data.len() {
        let size = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        if &data[pos + 4..pos + 8] == box_type {
            return Some(pos);
        }
        if size < 8 || pos + size > data.len() {
            break;
        }
        let child_offset = box_child_offset(&data[pos + 4..pos + 8]);
        if child_offset > 0 && pos + child_offset < pos + size {
            if let Some(inner) = find_box(&data[pos + child_offset..pos + size], box_type) {
                return Some(pos + child_offset + inner);
            }
        }
        pos += size;
    }
    None
}

/// Return the offset where child boxes begin for known container box types.
/// Returns 0 for non-container boxes (don't recurse).
fn box_child_offset(box_type: &[u8]) -> usize {
    match box_type {
        // Simple containers: children start right after the 8-byte box header
        b"moov" | b"trak" | b"mdia" | b"minf" | b"stbl" | b"sinf" | b"schi"
        | b"moof" | b"traf" | b"edts" | b"mvex" => 8,
        // Full box containers: 8 header + 4 version/flags = 12
        b"dinf" => 8,
        // stsd: full box with entry_count: 8 header + 4 version/flags + 4 entry_count = 16
        b"stsd" => 16,
        // Encrypted audio sample entry (enca):
        // 8 header + 6 reserved + 2 data_ref_index + 8 reserved + 2 channel_count +
        // 2 sample_size + 2 compression_id + 2 packet_size + 4 sample_rate = 36
        b"enca" | b"mp4a" | b"alac" => 36,
        // Encrypted video sample entry: similar but different size, not needed for audio
        _ => 0,
    }
}

fn is_container_box(box_type: &[u8]) -> bool {
    box_child_offset(box_type) > 0
}

/// Create a sanitized init segment with encryption boxes removed.
fn sanitize_init_segment(init_data: &[u8]) -> Result<Vec<u8>> {
    // For now, just pass through the init segment as-is.
    // A full implementation would remove sinf/senc/sbgp/sgpd boxes and
    // fix up stsd entries. But since the downstream symphonia decoder
    // handles this gracefully, we do a simpler approach:
    // remove encryption-related boxes from stsd entries.
    Ok(remove_encryption_boxes(init_data))
}

/// Remove encryption-related boxes from init segment data.
/// This removes sinf boxes from sample entries in stsd,
/// and removes sbgp/sgpd boxes with seig/seam grouping from stbl.
fn remove_encryption_boxes(data: &[u8]) -> Vec<u8> {
    // For a minimal working implementation, we strip:
    // 1. sinf boxes from within stsd sample entries
    // 2. sbgp and sgpd boxes with seam/seig from stbl
    // We also need to fix up the 'enca' or 'encv' box type back to the original codec type.
    //
    // This is a recursive box-level operation.
    rewrite_boxes(data, &[])
}

fn rewrite_boxes(data: &[u8], parent_chain: &[&[u8; 4]]) -> Vec<u8> {
    let mut output = Vec::new();
    let mut pos = 0;

    while pos + 8 <= data.len() {
        let box_size = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        if box_size < 8 || pos + box_size > data.len() {
            // Copy remaining data as-is
            output.extend_from_slice(&data[pos..]);
            break;
        }
        let box_type: [u8; 4] = [data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]];
        let box_data = &data[pos..pos + box_size];

        // Check if this is an stsd entry (parent is stsd) and the type is enca/encv
        let in_stsd = parent_chain.last().map_or(false, |p| **p == *b"stsd");

        // sinf is a child of enca/encv (encrypted sample entries), skip it
        let in_encrypted_entry = parent_chain.last().map_or(false, |p| {
            **p == *b"enca" || **p == *b"encv"
        });
        if &box_type == b"sinf" && in_encrypted_entry {
            pos += box_size;
            continue;
        }

        // Skip encryption-related sbgp/sgpd in stbl
        let in_stbl = parent_chain.last().map_or(false, |p| **p == *b"stbl");
        if in_stbl && (&box_type == b"sbgp" || &box_type == b"sgpd") {
            // Check grouping_type field (at offset 12 for full box: 8 header + 4 version/flags)
            if box_size >= 16 {
                let grouping_type = &data[pos + 12..pos + 16];
                if grouping_type == b"seig" || grouping_type == b"seam" {
                    pos += box_size;
                    continue;
                }
            }
        }

        if is_container_box(&box_type) {
            let mut new_chain: Vec<&[u8; 4]> = parent_chain.to_vec();
            new_chain.push(&box_type);

            let child_off = box_child_offset(&box_type);
            let pre_children = box_data[8..child_off].to_vec();
            let children = rewrite_boxes(&box_data[child_off..], &new_chain);

            let new_size = (8 + pre_children.len() + children.len()) as u32;
            output.extend_from_slice(&new_size.to_be_bytes());

            let mut out_type = box_type;
            if in_stsd {
                if &box_type == b"enca" {
                    if children.windows(4).any(|w| w == b"alac") {
                        out_type = *b"alac";
                    } else {
                        out_type = *b"mp4a";
                    }
                } else if &box_type == b"encv" {
                    out_type = *b"avc1";
                }
            }
            output.extend_from_slice(&out_type);
            output.extend_from_slice(&pre_children);
            output.extend_from_slice(&children);
        } else {
            // Copy box as-is
            output.extend_from_slice(box_data);
        }

        pos += box_size;
    }

    output
}

/// Decrypt a fragment's samples and return the cleaned fragment data.
fn decrypt_fragment(
    frag_data: &[u8],
    conn: &mut TcpStream,
    decrypt_info: &DecryptInfo,
) -> Result<Vec<u8>> {
    // A fragment consists of moof + mdat.
    // We need to:
    // 1. Parse the moof to find traf/trun/senc boxes for sample info
    // 2. Use senc to determine per-sample subsample patterns
    // 3. Decrypt the samples in mdat using the TCP connection
    // 4. Remove encryption boxes from moof (senc, saiz, saio, sbgp, sgpd with seig/seam)
    // 5. Fix up trun data offsets
    // 6. Output cleaned moof + decrypted mdat

    // Find moof and mdat boundaries
    let mut moof_start = None;
    let mut moof_size = 0;
    let mut mdat_start = None;
    let mut mdat_size = 0;
    let mut pos = 0;

    while pos + 8 <= frag_data.len() {
        let size = u32::from_be_bytes([
            frag_data[pos], frag_data[pos + 1], frag_data[pos + 2], frag_data[pos + 3],
        ]) as usize;
        let btype = &frag_data[pos + 4..pos + 8];

        if size < 8 || pos + size > frag_data.len() {
            break;
        }

        if btype == b"moof" {
            moof_start = Some(pos);
            moof_size = size;
        } else if btype == b"mdat" {
            mdat_start = Some(pos);
            mdat_size = size;
        }

        pos += size;
    }

    let moof_start = moof_start.context("No moof box in fragment")?;
    let mdat_start = mdat_start.context("No mdat box in fragment")?;

    let moof_data = &frag_data[moof_start..moof_start + moof_size];
    let mdat_data = &frag_data[mdat_start..mdat_start + mdat_size];

    // Extract sample info from moof (trun + senc)
    let sample_info = extract_sample_info(moof_data, decrypt_info)?;

    // Decrypt mdat payload
    let mdat_header_size = 8; // standard mdat box header
    let mut mdat_payload = mdat_data[mdat_header_size..].to_vec();

    decrypt_samples(
        &mut mdat_payload,
        &sample_info,
        conn,
        decrypt_info,
    )?;

    // Build cleaned moof (remove encryption boxes, fix data offset)
    let cleaned_moof = clean_moof(moof_data, mdat_size)?;

    // Build output fragment
    let mut output = Vec::new();
    output.extend_from_slice(&cleaned_moof);

    // Write mdat with decrypted payload
    let new_mdat_size = (mdat_header_size + mdat_payload.len()) as u32;
    output.extend_from_slice(&new_mdat_size.to_be_bytes());
    output.extend_from_slice(b"mdat");
    output.extend_from_slice(&mdat_payload);

    Ok(output)
}

#[derive(Debug)]
struct SampleInfo {
    samples: Vec<SampleEntry>,
}

#[derive(Debug)]
struct SampleEntry {
    size: u32,
    subsample_info: Vec<SubsampleEntry>,
}

#[derive(Debug, Clone)]
struct SubsampleEntry {
    bytes_of_clear_data: u16,
    bytes_of_protected_data: u32,
}

/// Extract sample sizes from trun and subsample info from senc within a moof box.
fn extract_sample_info(moof_data: &[u8], decrypt_info: &DecryptInfo) -> Result<SampleInfo> {
    // Find traf within moof
    let traf_pos = find_box(moof_data, b"traf")
        .context("No traf box in moof")?;
    let traf_size = u32::from_be_bytes([
        moof_data[traf_pos],
        moof_data[traf_pos + 1],
        moof_data[traf_pos + 2],
        moof_data[traf_pos + 3],
    ]) as usize;
    let traf_data = &moof_data[traf_pos..traf_pos + traf_size];

    // Parse trun for sample sizes
    let sample_sizes = parse_trun_sample_sizes(traf_data)?;

    // Parse senc for subsample info
    let subsample_patterns = parse_senc_subsamples(traf_data, sample_sizes.len(), decrypt_info.default_per_sample_iv_size);

    let mut samples = Vec::new();
    for (i, size) in sample_sizes.iter().enumerate() {
        let subsample_info = subsample_patterns
            .get(i)
            .cloned()
            .unwrap_or_default();
        samples.push(SampleEntry {
            size: *size,
            subsample_info,
        });
    }

    Ok(SampleInfo { samples })
}

/// Parse trun box to get sample sizes.
fn parse_trun_sample_sizes(traf_data: &[u8]) -> Result<Vec<u32>> {
    let trun_pos = find_child_box(traf_data, b"trun")
        .context("No trun box in traf")?;
    let trun_size = u32::from_be_bytes([
        traf_data[trun_pos],
        traf_data[trun_pos + 1],
        traf_data[trun_pos + 2],
        traf_data[trun_pos + 3],
    ]) as usize;
    let trun = &traf_data[trun_pos..trun_pos + trun_size];

    // trun full box: header(8) + version(1) + flags(3) + sample_count(4) + ...
    if trun.len() < 12 {
        bail!("trun box too small");
    }
    let flags = u32::from_be_bytes([0, trun[9], trun[10], trun[11]]);
    let sample_count = u32::from_be_bytes([trun[12], trun[13], trun[14], trun[15]]) as usize;

    let mut offset = 16;

    // data_offset present
    if flags & 0x000001 != 0 {
        offset += 4;
    }
    // first_sample_flags present
    if flags & 0x000004 != 0 {
        offset += 4;
    }

    let has_duration = flags & 0x000100 != 0;
    let has_size = flags & 0x000200 != 0;
    let has_flags = flags & 0x000400 != 0;
    let has_cts_offset = flags & 0x000800 != 0;

    let mut sizes = Vec::with_capacity(sample_count);
    for _ in 0..sample_count {
        if has_duration {
            offset += 4;
        }
        if has_size {
            if offset + 4 > trun.len() {
                break;
            }
            let size = u32::from_be_bytes([trun[offset], trun[offset + 1], trun[offset + 2], trun[offset + 3]]);
            sizes.push(size);
            offset += 4;
        } else {
            sizes.push(0);
        }
        if has_flags {
            offset += 4;
        }
        if has_cts_offset {
            offset += 4;
        }
    }

    Ok(sizes)
}

/// Parse senc box for subsample encryption info.
fn parse_senc_subsamples(traf_data: &[u8], sample_count: usize, iv_size: u8) -> Vec<Vec<SubsampleEntry>> {
    let Some(senc_pos) = find_child_box(traf_data, b"senc") else {
        return Vec::new();
    };
    let senc_size = u32::from_be_bytes([
        traf_data[senc_pos],
        traf_data[senc_pos + 1],
        traf_data[senc_pos + 2],
        traf_data[senc_pos + 3],
    ]) as usize;
    let senc = &traf_data[senc_pos..senc_pos + senc_size];

    // senc full box: header(8) + version(1) + flags(3) + sample_count(4)
    if senc.len() < 16 {
        return Vec::new();
    }
    let flags = u32::from_be_bytes([0, senc[9], senc[10], senc[11]]);
    let _senc_sample_count = u32::from_be_bytes([senc[12], senc[13], senc[14], senc[15]]) as usize;
    let has_subsamples = flags & 0x2 != 0;

    let per_sample_iv_size = iv_size;

    tracing::debug!(
        "senc: flags={:#x}, has_subsamples={}, sample_count={}, senc_len={}, iv_size={}",
        flags, has_subsamples, _senc_sample_count, senc.len(), per_sample_iv_size
    );

    let mut offset = 16;
    let mut result = Vec::new();

    for _ in 0..sample_count {
        if offset >= senc.len() {
            break;
        }

        // Skip IV if present
        offset += per_sample_iv_size as usize;

        let mut subsamples = Vec::new();
        if has_subsamples {
            if offset + 2 > senc.len() {
                break;
            }
            let subsample_count = u16::from_be_bytes([senc[offset], senc[offset + 1]]) as usize;
            offset += 2;
            for _ in 0..subsample_count {
                if offset + 6 > senc.len() {
                    break;
                }
                let clear = u16::from_be_bytes([senc[offset], senc[offset + 1]]);
                let protected = u32::from_be_bytes([senc[offset + 2], senc[offset + 3], senc[offset + 4], senc[offset + 5]]);
                subsamples.push(SubsampleEntry {
                    bytes_of_clear_data: clear,
                    bytes_of_protected_data: protected,
                });
                offset += 6;
            }
        }
        result.push(subsamples);
    }

    result
}

/// Find a child box within a container box (at offset 8 past the header).
fn find_child_box(container_data: &[u8], box_type: &[u8; 4]) -> Option<usize> {
    // Container has 8-byte header, children start at offset 8
    let start = 8;
    let mut pos = start;
    while pos + 8 <= container_data.len() {
        let size = u32::from_be_bytes([
            container_data[pos],
            container_data[pos + 1],
            container_data[pos + 2],
            container_data[pos + 3],
        ]) as usize;
        if size < 8 || pos + size > container_data.len() {
            break;
        }
        if &container_data[pos + 4..pos + 8] == box_type {
            return Some(pos);
        }
        pos += size;
    }
    None
}

/// Decrypt samples in the mdat payload using the TCP decrypt server.
fn decrypt_samples(
    mdat_payload: &mut [u8],
    sample_info: &SampleInfo,
    conn: &mut TcpStream,
    decrypt_info: &DecryptInfo,
) -> Result<()> {
    let decrypt_block_len = (decrypt_info.default_crypt_byte_block as usize) * 16;
    let skip_block_len = (decrypt_info.default_skip_byte_block as usize) * 16;

    tracing::debug!(
        "decrypt_samples: {} samples, crypt_block={}, skip_block={}, iv_size={}",
        sample_info.samples.len(),
        decrypt_block_len,
        skip_block_len,
        decrypt_info.default_per_sample_iv_size
    );

    let mut pos = 0;
    for (i, sample) in sample_info.samples.iter().enumerate() {
        let sample_end = pos + sample.size as usize;
        if sample_end > mdat_payload.len() {
            tracing::debug!(
                "  sample {}: size {} exceeds mdat payload (pos={}, payload_len={})",
                i, sample.size, pos, mdat_payload.len()
            );
            break;
        }

        if sample.subsample_info.is_empty() {
            // Full sample encryption
            cbcs_decrypt_raw(
                &mut mdat_payload[pos..sample_end],
                conn,
                decrypt_block_len,
                skip_block_len,
            )?;
        } else {
            // Subsample encryption
            let mut sub_pos = pos;
            for ss in &sample.subsample_info {
                sub_pos += ss.bytes_of_clear_data as usize;
                if ss.bytes_of_protected_data > 0 {
                    let end = sub_pos + ss.bytes_of_protected_data as usize;
                    if end <= mdat_payload.len() {
                        cbcs_decrypt_raw(
                            &mut mdat_payload[sub_pos..end],
                            conn,
                            decrypt_block_len,
                            skip_block_len,
                        )?;
                    }
                    sub_pos = end;
                }
            }
        }

        pos = sample_end;
    }

    Ok(())
}

/// CBCS decrypt raw data: send encrypted blocks to the server, receive decrypted.
fn cbcs_decrypt_raw(
    data: &mut [u8],
    conn: &mut TcpStream,
    decrypt_block_len: usize,
    skip_block_len: usize,
) -> Result<()> {
    if skip_block_len == 0 {
        // Full subsample decryption (e.g., Apple Music ALAC)
        cbcs_full_subsample_decrypt(data, conn)
    } else {
        // Stripe pattern decryption
        cbcs_stripe_decrypt(data, conn, decrypt_block_len, skip_block_len)
    }
}

fn cbcs_full_subsample_decrypt(data: &mut [u8], conn: &mut TcpStream) -> Result<()> {
    // Truncate to multiple of 16
    let truncated_len = data.len() & !0xf;
    if truncated_len == 0 {
        return Ok(());
    }

    // Send size (4 bytes LE) + data
    conn.write_all(&(truncated_len as u32).to_le_bytes())?;
    conn.write_all(&data[..truncated_len])?;
    conn.flush()?;

    // Read back decrypted data
    let mut buf = vec![0u8; truncated_len];
    conn.read_exact(&mut buf)?;
    data[..truncated_len].copy_from_slice(&buf);

    Ok(())
}

fn cbcs_stripe_decrypt(
    data: &mut [u8],
    conn: &mut TcpStream,
    decrypt_block_len: usize,
    skip_block_len: usize,
) -> Result<()> {
    let size = data.len();
    if size < decrypt_block_len {
        return Ok(());
    }

    // Count total encrypted bytes
    let count = ((size - decrypt_block_len) / (decrypt_block_len + skip_block_len)) + 1;
    let total_len = count * decrypt_block_len;

    // Send total size
    conn.write_all(&(total_len as u32).to_le_bytes())?;

    // Send encrypted blocks
    let mut pos = 0;
    loop {
        if size - pos < decrypt_block_len {
            break;
        }
        conn.write_all(&data[pos..pos + decrypt_block_len])?;
        pos += decrypt_block_len;
        if size - pos < skip_block_len {
            break;
        }
        pos += skip_block_len;
    }
    conn.flush()?;

    // Read back decrypted blocks
    pos = 0;
    let mut buf = vec![0u8; decrypt_block_len];
    loop {
        if size - pos < decrypt_block_len {
            break;
        }
        conn.read_exact(&mut buf)?;
        data[pos..pos + decrypt_block_len].copy_from_slice(&buf);
        pos += decrypt_block_len;
        if size - pos < skip_block_len {
            break;
        }
        pos += skip_block_len;
    }

    Ok(())
}

/// Clean moof box: remove encryption boxes and fix data offset in trun.
fn clean_moof(moof_data: &[u8], _mdat_size: usize) -> Result<Vec<u8>> {
    let cleaned = remove_encryption_boxes_from_moof(moof_data);

    // Fix data offset in trun: it should point to the first byte of mdat payload
    // data_offset is relative to the start of the enclosing moof box
    fix_trun_data_offset(&cleaned)
}

fn remove_encryption_boxes_from_moof(data: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();

    // moof header
    if data.len() < 8 {
        return data.to_vec();
    }
    // We'll rebuild the moof box
    let original_size = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
    let _ = original_size;
    // Skip box header, process children
    let mut children_output = Vec::new();
    let mut pos = 8;
    while pos + 8 <= data.len() {
        let box_size = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        if box_size < 8 || pos + box_size > data.len() {
            break;
        }
        let box_type = &data[pos + 4..pos + 8];

        if box_type == b"traf" {
            // Recursively clean traf
            let cleaned_traf = remove_encryption_boxes_from_traf(&data[pos..pos + box_size]);
            children_output.extend_from_slice(&cleaned_traf);
        } else {
            // Copy other boxes (mfhd, etc.) as-is
            children_output.extend_from_slice(&data[pos..pos + box_size]);
        }
        pos += box_size;
    }

    // Rebuild moof
    let new_size = (8 + children_output.len()) as u32;
    output.extend_from_slice(&new_size.to_be_bytes());
    output.extend_from_slice(b"moof");
    output.extend_from_slice(&children_output);

    output
}

fn remove_encryption_boxes_from_traf(data: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    let mut children_output = Vec::new();
    let mut pos = 8; // skip traf header

    while pos + 8 <= data.len() {
        let box_size = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        if box_size < 8 || pos + box_size > data.len() {
            break;
        }
        let box_type = &data[pos + 4..pos + 8];

        let should_skip = match box_type {
            b"senc" | b"saiz" | b"saio" => true,
            b"sbgp" | b"sgpd" => {
                // Check grouping type
                if box_size >= 16 {
                    let gt = &data[pos + 12..pos + 16];
                    gt == b"seig" || gt == b"seam"
                } else {
                    false
                }
            }
            // Also skip UUID boxes that might be senc alternatives
            _ => false,
        };

        if !should_skip {
            children_output.extend_from_slice(&data[pos..pos + box_size]);
        }
        pos += box_size;
    }

    let new_size = (8 + children_output.len()) as u32;
    output.extend_from_slice(&new_size.to_be_bytes());
    output.extend_from_slice(b"traf");
    output.extend_from_slice(&children_output);

    output
}

/// Fix the data_offset in trun to account for the new moof size.
fn fix_trun_data_offset(moof_data: &[u8]) -> Result<Vec<u8>> {
    let mut data = moof_data.to_vec();
    let moof_size = data.len() as i32;

    // Find trun within moof -> traf
    if let Some(traf_pos) = find_child_box(&data, b"traf") {
        let traf_size = u32::from_be_bytes([data[traf_pos], data[traf_pos + 1], data[traf_pos + 2], data[traf_pos + 3]]) as usize;
        let traf_end = traf_pos + traf_size;
        // Find trun within traf
        let traf_data = &data[traf_pos..traf_end];
        if let Some(trun_rel) = find_child_box(traf_data, b"trun") {
            let trun_abs = traf_pos + trun_rel;
            let trun_flags = u32::from_be_bytes([0, data[trun_abs + 9], data[trun_abs + 10], data[trun_abs + 11]]);

            // data_offset flag
            if trun_flags & 0x000001 != 0 {
                let data_offset_pos = trun_abs + 16;
                if data_offset_pos + 4 <= data.len() {
                    // data_offset should be moof_size + 8 (mdat header)
                    let new_offset = moof_size + 8;
                    data[data_offset_pos..data_offset_pos + 4]
                        .copy_from_slice(&new_offset.to_be_bytes());
                }
            }
        }
    }

    Ok(data)
}

// ── Provider implementation ─────────────────────────────────────────

pub fn factory(state: Arc<AppState>) -> Box<dyn MusicProvider> {
    Box::new(AppleMusicProvider::new(state))
}

pub struct AppleMusicProvider {
    state: Arc<AppState>,
}

impl AppleMusicProvider {
    fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

impl MusicProvider for AppleMusicProvider {
    fn name(&self) -> &'static str {
        "Apple Music"
    }

    fn render(&self) -> Element {
        apple_music_content(self.state.clone())
    }
}

// ── UI ──────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
enum DownloadStatus {
    Idle,
    Searching,
    FetchingToken,
    Downloading(String),
    Error(String),
}

fn apple_music_content(state: Arc<AppState>) -> Element {
    let mut search_query = use_signal(|| String::new());
    let results: Signal<Vec<SongData>> = use_signal(Vec::new);
    let status = use_signal(|| DownloadStatus::Idle);
    let token: Signal<Option<String>> = use_signal(|| None);

    let on_search = {
        move |_| {
            let query = search_query().trim().to_string();
            if query.is_empty() {
                return;
            }
            let mut status = status.clone();
            let mut results = results.clone();
            let mut token = token.clone();

            spawn(async move {
                status.set(DownloadStatus::FetchingToken);

                // Fetch token if we don't have one
                let bearer = if let Some(t) = token() {
                    t
                } else {
                    match fetch_bearer_token().await {
                        Ok(t) => {
                            token.set(Some(t.clone()));
                            t
                        }
                        Err(e) => {
                            error!("Failed to fetch Apple Music token: {}", e);
                            status.set(DownloadStatus::Error(format!("Token error: {}", e)));
                            return;
                        }
                    }
                };

                status.set(DownloadStatus::Searching);
                match search_songs(&bearer, &query, 10).await {
                    Ok(songs) => {
                        results.set(songs);
                        status.set(DownloadStatus::Idle);
                    }
                    Err(e) => {
                        error!("Apple Music search failed: {}", e);
                        status.set(DownloadStatus::Error(format!("Search error: {}", e)));
                    }
                }
            });
        }
    };

    rsx! {
        div {
            class: "space-y-4",

            // Search bar
            div {
                class: "flex gap-2",
                input {
                    class: "flex-1 p-3 rounded-xl bg-slate-800 border border-slate-700 text-white text-sm placeholder-slate-500 focus:outline-none focus:border-pink-500/50",
                    placeholder: "Search Apple Music...",
                    value: "{search_query()}",
                    oninput: move |evt| search_query.set(evt.value()),
                    onkeypress: {
                        let on_search = on_search.clone();
                        move |evt: KeyboardEvent| {
                            if evt.key() == Key::Enter {
                                on_search(());
                            }
                        }
                    },
                }
                button {
                    class: "px-4 py-3 rounded-xl bg-pink-500/20 border border-pink-500/50 text-pink-400 text-sm font-medium hover:bg-pink-500/30 transition-colors disabled:opacity-50",
                    disabled: matches!(status(), DownloadStatus::Searching | DownloadStatus::FetchingToken | DownloadStatus::Downloading(_)),
                    onclick: move |_| on_search(()),
                    match status() {
                        DownloadStatus::Searching | DownloadStatus::FetchingToken => "...",
                        _ => "Search",
                    }
                }
            }

            // Status message
            match status() {
                DownloadStatus::Error(ref msg) => rsx! {
                    p { class: "text-sm text-red-400", "{msg}" }
                },
                DownloadStatus::Downloading(ref name) => rsx! {
                    p { class: "text-sm text-sky-400", "Downloading: {name}..." }
                },
                DownloadStatus::FetchingToken => rsx! {
                    p { class: "text-sm text-slate-400", "Fetching token..." }
                },
                _ => rsx! {},
            }

            // Results list
            if !results().is_empty() {
                div {
                    class: "space-y-2 max-h-80 overflow-y-auto",
                    for song in results().iter() {
                        { song_result_item(song.clone(), state.clone(), token.clone(), status.clone()) }
                    }
                }
            }
        }
    }
}

fn song_result_item(
    song: SongData,
    state: Arc<AppState>,
    token: Signal<Option<String>>,
    status: Signal<DownloadStatus>,
) -> Element {
    let artwork_url = song
        .attributes
        .artwork
        .url
        .replace("{w}", "80")
        .replace("{h}", "80");

    let duration = song
        .attributes
        .duration_in_millis
        .map(|ms| {
            let secs = ms / 1000;
            format!("{}:{:02}", secs / 60, secs % 60)
        })
        .unwrap_or_default();

    let song_name = song.attributes.name.clone();
    let artist_name = song.attributes.artist_name.clone();
    let song_id = song.id.clone();
    let display_name = format!("{} - {}", song_name, artist_name);

    rsx! {
        button {
            class: "w-full p-3 rounded-xl bg-slate-800/50 border border-slate-700/50 hover:bg-slate-700/50 transition-colors flex items-center gap-3 text-left",
            disabled: matches!(status(), DownloadStatus::Downloading(_)),
            onclick: move |_| {
                let state = state.clone();
                let token = token.clone();
                let mut status = status.clone();
                let song_id = song_id.clone();
                let display_name = display_name.clone();
                let file_name = format!("{}.m4a", display_name);

                spawn(async move {
                    status.set(DownloadStatus::Downloading(display_name.clone()));

                    let bearer = match token() {
                        Some(t) => t,
                        None => {
                            status.set(DownloadStatus::Error("No token available".to_string()));
                            return;
                        }
                    };

                    // Get the enhanced HLS URL
                    let hls_url = match get_song_enhanced_hls(&bearer, &song_id).await {
                        Ok(url) => url,
                        Err(e) => {
                            error!("Failed to get HLS URL: {}", e);
                            status.set(DownloadStatus::Error(format!("HLS error: {}", e)));
                            return;
                        }
                    };

                    // Download and decrypt
                    match download_and_decrypt(&hls_url, &song_id).await {
                        Ok(data) => {
                            if let Err(e) = state.start_music_stream(data, file_name) {
                                error!("Failed to start music stream: {}", e);
                                status.set(DownloadStatus::Error(format!("Stream error: {}", e)));
                            } else {
                                status.set(DownloadStatus::Idle);
                            }
                        }
                        Err(e) => {
                            error!("Failed to download/decrypt: {}", e);
                            status.set(DownloadStatus::Error(format!("Download error: {}", e)));
                        }
                    }
                });
            },

            // Artwork thumbnail
            img {
                class: "w-10 h-10 rounded-lg flex-shrink-0",
                src: "{artwork_url}",
            }

            // Song info
            div {
                class: "flex-1 min-w-0",
                p {
                    class: "text-sm text-white font-medium truncate",
                    "{song_name}"
                }
                p {
                    class: "text-xs text-slate-400 truncate",
                    "{artist_name} · {song.attributes.album_name}"
                }
            }

            // Duration
            span {
                class: "text-xs text-slate-500 flex-shrink-0",
                "{duration}"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_search_and_download_attention() {
        tracing_subscriber::fmt::init();

        println!("=== Step 1: Fetching bearer token ===");
        let token = fetch_bearer_token().await.expect("Failed to fetch token");
        println!("Token: {}...{}", &token[..20], &token[token.len()-10..]);

        println!("\n=== Step 2: Searching for 'Charlie Puth Attention' ===");
        let results = search_songs(&token, "Charlie Puth Attention", 5)
            .await
            .expect("Search failed");
        println!("Found {} results:", results.len());
        for (i, song) in results.iter().enumerate() {
            println!(
                "  [{}] {} - {} (id: {})",
                i, song.attributes.name, song.attributes.artist_name, song.id
            );
        }
        assert!(!results.is_empty(), "No search results found");

        let song = &results[0];
        println!(
            "\n=== Step 3: Getting enhanced HLS URL for '{}' (id: {}) ===",
            song.attributes.name, song.id
        );
        let hls_url = get_song_enhanced_hls(&token, &song.id)
            .await
            .expect("Failed to get HLS URL");
        println!("HLS URL: {}", hls_url);

        println!("\n=== Step 4: Downloading and decrypting ===");
        let data = download_and_decrypt(&hls_url, &song.id)
            .await
            .expect("Download/decrypt failed");
        println!("Decrypted data size: {} bytes", data.len());

        let output_path = "/tmp/attention.m4a";
        std::fs::write(output_path, &data).expect("Failed to write file");
        println!("Written to {}", output_path);
    }
}

