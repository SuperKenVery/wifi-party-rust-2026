//! Orchestration: URL parsing + full download-and-decrypt pipeline.

use anyhow::{Context, Result, anyhow, bail};
use regex::Regex;
use reqwest::Client;

use super::api;
use super::m3u8;
use super::mp4_decrypt;

/// Default TCP address of the wrapper decryption service.
pub const DEFAULT_WRAPPER_ADDR: &str = "127.0.0.1:10020";

/// Max ALAC sample rate we accept. 192 kHz is the Apple Music ceiling.
pub const ALAC_MAX_SAMPLE_RATE: u32 = 192000;

/// Parse the storefront and song id from an Apple Music URL.
///
/// Supports these URL shapes we care about:
///   - `https://music.apple.com/{sf}/album/{slug}/{album_id}?i={song_id}`
///   - `https://music.apple.com/{sf}/song/{slug}/{song_id}`
pub fn parse_song_url(url: &str) -> Result<(String, String)> {
    let album_re =
        Regex::new(r"^https://(?:beta\.music|music|classical\.music)\.apple\.com/(\w{2})/album/[^/?]+/(\d+)").unwrap();
    let song_re =
        Regex::new(r"^https://(?:beta\.music|music|classical\.music)\.apple\.com/(\w{2})/song/[^/?]+/(\d+)").unwrap();
    let i_re = Regex::new(r"[?&]i=(\d+)").unwrap();

    if let Some(m) = song_re.captures(url) {
        let sf = m.get(1).unwrap().as_str().to_string();
        let id = m.get(2).unwrap().as_str().to_string();
        return Ok((sf, id));
    }
    if let Some(m) = album_re.captures(url) {
        let sf = m.get(1).unwrap().as_str().to_string();
        if let Some(i) = i_re.captures(url) {
            return Ok((sf, i.get(1).unwrap().as_str().to_string()));
        } else {
            bail!("album URL without `?i=<song_id>` query: {url}");
        }
    }
    bail!("unrecognized Apple Music URL: {url}")
}

/// Fully-downloaded + decrypted track ready to feed Symphonia.
pub struct DownloadedSong {
    pub bytes: Vec<u8>,
    /// Suggested file name; uses the song's name+artist and ends with `.m4a`.
    pub file_name: String,
    /// The song data returned by the Apple Music API.
    pub info: api::SongData,
}

/// Download and decrypt one song from a user-supplied Apple Music URL.
pub async fn download_song(
    client: &Client,
    token: &str,
    url: &str,
    wrapper_addr: &str,
) -> Result<DownloadedSong> {
    let (storefront, song_id) = parse_song_url(url)?;
    download_song_by_id(client, token, &storefront, &song_id, wrapper_addr).await
}

/// Same as [`download_song`] but taking an explicit storefront + song id.
pub async fn download_song_by_id(
    client: &Client,
    token: &str,
    storefront: &str,
    song_id: &str,
    wrapper_addr: &str,
) -> Result<DownloadedSong> {
    let info = api::get_song(client, token, storefront, song_id)
        .await
        .context("get song info")?;
    let enhanced_hls = info
        .attributes
        .extended_asset_urls
        .as_ref()
        .and_then(|x| x.enhanced_hls.as_ref())
        .ok_or_else(|| anyhow!("song has no enhancedHls URL (is it lossless-available?)"))?
        .to_string();

    // Master -> pick ALAC variant.
    let variant = m3u8::select_alac_variant(client, &enhanced_hls, ALAC_MAX_SAMPLE_RATE)
        .await
        .context("select ALAC variant")?;
    tracing::info!(
        "Apple Music: {} / {} -> {} ({} Hz, {} bit)",
        info.attributes.name,
        info.attributes.artist_name,
        variant.audio_group,
        variant.sample_rate,
        variant.bit_depth
    );

    // Media playlist -> segments.
    if let Ok(path) = std::env::var("DUMP_M3U8") {
        let text = client
            .get(&variant.media_uri)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        std::fs::write(&path, &text).ok();
        println!("Apple Music: wrote media m3u8 to {path}");
    }
    let segments = m3u8::fetch_media_playlist(client, &variant.media_uri)
        .await
        .context("fetch media playlist")?;

    // All Apple Music media playlists point all segments at the same file
    // with different byterange. We just download the whole file in one shot.
    let file_uri = &segments[0].uri;
    let raw = client
        .get(file_uri)
        .send()
        .await
        .context("download fMP4 payload")?
        .error_for_status()?
        .bytes()
        .await?;
    let raw_vec = raw.to_vec();
    tracing::info!(
        "Apple Music: downloaded {} bytes of encrypted fMP4",
        raw_vec.len()
    );
    if let Ok(path) = std::env::var("DUMP_ENCRYPTED") {
        std::fs::write(&path, &raw_vec).context("write encrypted dump")?;
        println!("Apple Music: wrote encrypted file to {path}");
    }

    let key_uris: Vec<String> = segments
        .iter()
        .map(|s| {
            s.key_uri
                .clone()
                .unwrap_or_else(|| "skd://itunes.apple.com/P000000000/s1/e1".to_string())
        })
        .collect();

    // Blocking decrypt on a worker thread so we don't block the runtime.
    let bytes = tokio::task::spawn_blocking({
        let raw = raw_vec;
        let key_uris = key_uris;
        let song_id = song_id.to_string();
        let wrapper_addr = wrapper_addr.to_string();
        move || mp4_decrypt::decrypt_fmp4(&raw, &key_uris, &song_id, &wrapper_addr)
    })
    .await
    .context("decrypt worker panicked")??;
    tracing::info!("Apple Music: decrypted -> {} bytes", bytes.len());

    let safe = sanitize_name(&format!(
        "{} - {}",
        info.attributes.artist_name, info.attributes.name
    ));
    let file_name = format!("{safe}.m4a");

    Ok(DownloadedSong {
        bytes,
        file_name,
        info,
    })
}

fn sanitize_name(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '/' | '\\' | '<' | '>' | ':' | '"' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_url_album_with_i() {
        let u = "https://music.apple.com/cn/album/dont-you-worry-child-radio-edit-feat-john-martin/716018055?i=716018386";
        let (sf, id) = parse_song_url(u).unwrap();
        assert_eq!(sf, "cn");
        assert_eq!(id, "716018386");
    }

    #[test]
    fn parse_url_song() {
        let u = "https://music.apple.com/us/song/you-move-me-2022-remaster/1624945520";
        let (sf, id) = parse_song_url(u).unwrap();
        assert_eq!(sf, "us");
        assert_eq!(id, "1624945520");
    }

    /// End-to-end download + decrypt + Symphonia decode past 15 s.
    ///
    /// Requires:
    ///   - network access to Apple's servers
    ///   - the wrapper decryption service listening on 127.0.0.1:10020
    #[tokio::test]
    #[ignore = "requires wrapper running on 127.0.0.1:10020 and network"]
    async fn download_dont_you_worry_child_decodes_past_15s() {
        let url = "https://music.apple.com/cn/album/dont-you-worry-child-radio-edit-feat-john-martin/716018055?i=716018386";
        let client = Client::builder().build().unwrap();
        let token = api::get_token(&client).await.expect("token");

        let song = download_song(&client, &token, url, DEFAULT_WRAPPER_ADDR)
            .await
            .expect("download");

        // Require the decrypted file to decode past 15 seconds. Use Symphonia
        // with the same feature set Wi-Fi Party compiles with (aac/flac/mp3/
        // ogg/wav) — ALAC is an iso/m4a trait that symphonia 0.5 reads via
        // the `isomp4` demux format even without explicit feature.
        use std::io::Cursor;
        use symphonia::core::audio::{AudioBufferRef, Signal};
        use symphonia::core::codecs::DecoderOptions;
        use symphonia::core::errors::Error as SymError;
        use symphonia::core::formats::FormatOptions;
        use symphonia::core::io::MediaSourceStream;
        use symphonia::core::meta::MetadataOptions;
        use symphonia::core::probe::Hint;

        let dump = std::env::var("DUMP_DECRYPTED").ok();
        if let Some(path) = dump.as_deref() {
            std::fs::write(path, &song.bytes).expect("write dump");
            println!("wrote decrypted file to {path} ({} bytes)", song.bytes.len());
        }
        let cursor = Cursor::new(song.bytes.clone());
        let mss = MediaSourceStream::new(Box::new(cursor), Default::default());
        let mut hint = Hint::new();
        hint.with_extension("m4a");
        let probed = symphonia::default::get_probe()
            .format(
                &hint,
                mss,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .expect("probe");
        let mut format = probed.format;
        let track = format
            .default_track()
            .expect("default track")
            .clone();
        let sample_rate = track.codec_params.sample_rate.unwrap_or(44100);
        let channels = track
            .codec_params
            .channels
            .map(|c| c.count())
            .unwrap_or(2) as u64;
        let mut decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())
            .expect("make decoder");

        let target_samples: u64 = 16 * sample_rate as u64;
        let mut total_samples: u64 = 0;
        loop {
            match format.next_packet() {
                Ok(packet) => match decoder.decode(&packet) {
                    Ok(decoded) => {
                        let frames = match decoded {
                            AudioBufferRef::F32(b) => b.frames(),
                            AudioBufferRef::S16(b) => b.frames(),
                            AudioBufferRef::S24(b) => b.frames(),
                            AudioBufferRef::S32(b) => b.frames(),
                            AudioBufferRef::U8(b) => b.frames(),
                            AudioBufferRef::U16(b) => b.frames(),
                            AudioBufferRef::U24(b) => b.frames(),
                            AudioBufferRef::U32(b) => b.frames(),
                            AudioBufferRef::F64(b) => b.frames(),
                            AudioBufferRef::S8(b) => b.frames(),
                        };
                        total_samples += frames as u64;
                        let _ = channels;
                        if total_samples >= target_samples {
                            break;
                        }
                    }
                    Err(SymError::DecodeError(e)) => {
                        panic!("decode error at sample {total_samples}: {e}");
                    }
                    Err(SymError::IoError(e))
                        if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                    {
                        break;
                    }
                    Err(e) => panic!("decoder error at sample {total_samples}: {e:?}"),
                },
                Err(SymError::IoError(e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    break;
                }
                Err(e) => panic!("packet error at sample {total_samples}: {e:?}"),
            }
        }
        assert!(
            total_samples >= target_samples,
            "decoded only {} samples ({:.2}s at {}Hz); wanted >= 15 s",
            total_samples,
            total_samples as f64 / sample_rate as f64,
            sample_rate
        );
        println!(
            "OK: decoded {:.2}s of audio ({} samples @ {} Hz)",
            total_samples as f64 / sample_rate as f64,
            total_samples,
            sample_rate
        );
    }
}
