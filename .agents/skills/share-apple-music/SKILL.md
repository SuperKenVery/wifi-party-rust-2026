---
name: "share-apple-music"
description: "Apple Music provider implementation for Wi-Fi Party: search, download, decrypt, and stream songs. Invoke when modifying Apple Music integration or debugging the fMP4 decrypt pipeline."
---

# Apple Music Provider

## File Location

`src/music_provider/apple_music.rs` - single-file implementation

## Registration

- Factory registered in `src/state/mod.rs` via `music_provider_factories` array
- Module declared in `src/music_provider/mod.rs`

## Data Flow

1. **Token**: Scrape `https://music.apple.com` HTML for JS bundle path, then extract JWT bearer token (starts with `eyJh`) from the JS file
2. **Search**: `GET https://amp-api.music.apple.com/v1/catalog/{storefront}/search?term=...&types=songs` with bearer token
3. **Song details**: `GET https://amp-api.music.apple.com/v1/catalog/{storefront}/songs/{id}?extend=extendedAssetUrls` to get `enhancedHls` m3u8 URL
4. **Master m3u8**: Parse to find ALAC or AAC variants (prefer ALAC, highest bandwidth)
5. **Media m3u8**: Parse for segments (byte ranges) and FairPlay key URIs (KEYFORMAT `streamingkeydelivery`)
6. **Download**: Fetch the single fMP4 file that all segments point to
7. **Decrypt**: TCP to `100.69.234.108:10020` (Frida wrapper on Android device)
8. **Feed**: Call `state.start_music_stream(decrypted_bytes, filename)` to share over network

## Decrypt Protocol (TCP to 100.69.234.108:10020)

Per key change:
- Send: adamId length (1 byte) + adamId bytes
- Send: keyUri length (1 byte) + keyUri bytes

Per encrypted sample/subsample:
- Send: size (4 bytes LE) + encrypted data
- Receive: decrypted data (same size)

End of key: send 4 zero bytes
End of connection: send 5 zero bytes

## fMP4 Processing

- Init segment (`ftyp` + `moov`): Remove `sinf` boxes from `stsd` entries, rename `enca`→`alac`/`mp4a`, strip `sbgp`/`sgpd` with `seig`/`seam`
- Fragments (`moof` + `mdat`): Parse `trun` for sample sizes, `senc` for subsample encryption patterns, decrypt via TCP, remove `senc`/`saiz`/`saio`/`sbgp`/`sgpd` from `traf`, fix `trun` data offset

## CBCS Decryption Modes

- **Full subsample** (skip_block=0): Truncate to 16-byte boundary, send/receive entire block
- **Stripe pattern** (crypt_block>0, skip_block>0): Alternate between crypt_block*16 encrypted bytes and skip_block*16 clear bytes

## Key Types and Structs

- `SongData`, `SongAttributes`, `Artwork`, `Preview` - Apple Music API response types
- `M3u8Variant`, `M3u8Segment`, `M3u8KeyInfo` - m3u8 parsing
- `DecryptInfo` - tenc box parameters
- `SampleInfo`, `SampleEntry`, `SubsampleEntry` - fMP4 sample encryption info
- `DownloadStatus` - UI state enum (Idle, Searching, FetchingToken, Downloading, Error)

## UI

Dioxus component `apple_music_content`:
- Search input + button
- Results list with artwork thumbnails, song name, artist, album, duration
- Click a result to download/decrypt/stream
- Status messages for loading/error states
