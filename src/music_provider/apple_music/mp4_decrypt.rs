//! Fragmented MP4 decryptor.
//!
//! Parses an encrypted Apple Music fMP4 stream (CBCS, ALAC), decrypts its
//! samples via the wrapper TCP service, and rebuilds a standard decrypted
//! fMP4 that Symphonia can read.
//!
//! The wrapper protocol (see `apple-music-downloader/utils/runv2/runv2.go`):
//!
//! For each fragment, in fragment order:
//!   - If the fragment has a key and it is not the first fragment in the
//!     connection, send the 4-byte "switch keys" message `00 00 00 00`.
//!   - Send `[len u8][adam_id (or "0" for prefetch key)]`.
//!   - Send `[len u8][key_uri]`.
//!   - For each sample in each track run, send `[len u32 LE][data]` and read
//!     back exactly `len` decrypted bytes. `len` is the sample size rounded
//!     down to a multiple of 16 (full-subsample CBCS with a 0 skip pattern,
//!     which is what Apple Music ALAC uses). Any trailing bytes smaller than
//!     16 are left unmodified on the wire, so on our side we only touch the
//!     decrypted prefix.
//! At the very end, send `00 00 00 00 00` (close).
//!
//! Boxes removed after decryption:
//!   - `moov/trak/mdia/minf/stbl/sbgp` and `.../sgpd` of grouping type
//!     `seig`/`seam`.
//!   - `moov/.../stsd/enca/sinf` (and the `enca` wrapper type itself is
//!     rewritten to `frma.data_format`).
//!   - `moov/pssh` anywhere inside moov.
//!   - `moof/pssh`.
//!   - `moof/traf/senc`, `saiz`, `saio`, and PIFF-UUID senc.
//!   - `moof/traf/sbgp`, `sgpd` of grouping type `seig`/`seam`.
//!
//! `moof/traf/trun.data_offset` is adjusted by `-bytes_removed_from_moof`
//! so that it continues to point at the correct byte inside the (unchanged)
//! `mdat`.

use anyhow::{Context, Result, anyhow, bail};
use std::io::{Read, Write};
use std::net::TcpStream;

// ---------------------------------------------------------------------------
// Box tree
// ---------------------------------------------------------------------------

/// A mutable, fully-parsed MP4 box.
#[derive(Debug)]
pub struct Mp4Box {
    /// 4-byte box type (e.g. `moov`, `moof`, `mdat`, `enca`, `alac`).
    pub btype: [u8; 4],
    /// Extra bytes that appear after the header but before any children.
    /// For plain leaf boxes this holds the entire payload. For container
    /// boxes it is empty. For boxes like `stsd` that have a fixed-size
    /// preamble before children, it holds that preamble.
    pub header_data: Vec<u8>,
    /// Child boxes (empty for leaves).
    pub children: Vec<Mp4Box>,
    /// If this is a UUID box, the 16-byte extended type immediately follows
    /// the 4-byte "uuid" type. We store it here and prepend at write time.
    pub uuid: Option<[u8; 16]>,
    /// True if this box is a container (its payload is parsed as children).
    pub is_container: bool,
}

const CONTAINER_TYPES: &[&[u8; 4]] = &[
    b"moov", b"trak", b"edts", b"mdia", b"minf", b"dinf", b"stbl", b"mvex", b"moof", b"traf",
    b"mfra", b"schi", b"sinf", b"udta", b"meta", b"ilst", b"mp4a", b"alac", b"enca", b"encv",
    b"avc1", b"hvc1", b"hev1",
];

fn is_container(btype: &[u8; 4]) -> bool {
    CONTAINER_TYPES.iter().any(|t| *t == btype)
}

/// Sample-entry boxes inside stsd: their payload begins with a 6-byte
/// `reserved` + 2-byte `data_reference_index`, and for audio an additional
/// 20 bytes of audio-sample-entry fields, then child boxes.
fn is_audio_sample_entry(btype: &[u8; 4]) -> bool {
    matches!(
        btype,
        b"mp4a" | b"alac" | b"enca" | b"ac-3" | b"ec-3" | b"fLaC" | b"Opus"
    )
}

fn read_u32(buf: &[u8], off: usize) -> u32 {
    u32::from_be_bytes(buf[off..off + 4].try_into().unwrap())
}

fn write_u32(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_be_bytes());
}

fn read_u64(buf: &[u8], off: usize) -> u64 {
    u64::from_be_bytes(buf[off..off + 8].try_into().unwrap())
}

/// Parse `data` into a flat sequence of top-level boxes.
pub fn parse_all(data: &[u8]) -> Result<Vec<Mp4Box>> {
    let mut out = Vec::new();
    let mut off = 0;
    while off < data.len() {
        let (b, new_off) = parse_box(data, off)?;
        out.push(b);
        off = new_off;
    }
    Ok(out)
}

fn parse_box(data: &[u8], off: usize) -> Result<(Mp4Box, usize)> {
    if data.len() - off < 8 {
        bail!("truncated box header at {off}");
    }
    let size32 = read_u32(data, off);
    let btype: [u8; 4] = data[off + 4..off + 8].try_into().unwrap();
    let mut payload_start = off + 8;
    let total_end;
    if size32 == 1 {
        if data.len() - off < 16 {
            bail!("truncated largesize box header at {off}");
        }
        let size64 = read_u64(data, off + 8);
        payload_start = off + 16;
        total_end = off + (size64 as usize);
    } else if size32 == 0 {
        total_end = data.len();
    } else {
        total_end = off + (size32 as usize);
    }
    if total_end > data.len() || total_end < payload_start {
        bail!(
            "box {} at {off} has bogus size (size={size32}, end={total_end}, file={})",
            String::from_utf8_lossy(&btype),
            data.len()
        );
    }

    // Optional UUID extended type.
    let mut uuid: Option<[u8; 16]> = None;
    if &btype == b"uuid" {
        if total_end - payload_start < 16 {
            bail!("uuid box missing extended type");
        }
        let mut u = [0u8; 16];
        u.copy_from_slice(&data[payload_start..payload_start + 16]);
        uuid = Some(u);
        payload_start += 16;
    }

    let payload = &data[payload_start..total_end];
    let mut bx = Mp4Box {
        btype,
        header_data: Vec::new(),
        children: Vec::new(),
        uuid,
        is_container: false,
    };

    if is_container(&btype) {
        // Audio sample entries have a fixed preamble: 8 bytes reserved+dri,
        // then 20 bytes of audio-sample-entry fields, before child boxes.
        let preamble = if is_audio_sample_entry(&btype) { 28 } else { 0 };
        if payload.len() < preamble {
            bail!(
                "container {} too short for preamble ({} < {preamble})",
                String::from_utf8_lossy(&btype),
                payload.len()
            );
        }
        bx.header_data = payload[..preamble].to_vec();
        let mut p_off = payload_start + preamble;
        while p_off < total_end {
            let (c, new_off) = parse_box(data, p_off)?;
            bx.children.push(c);
            p_off = new_off;
        }
        bx.is_container = true;
    } else if &btype == b"meta" {
        // `meta` is a FullBox container: 4 bytes version+flags, then children.
        // We don't modify it, but the default `is_container` path would eat
        // the preamble wrong. Handle explicitly.
        if payload.len() < 4 {
            bx.header_data = payload.to_vec();
        } else {
            bx.header_data = payload[..4].to_vec();
            let mut p_off = payload_start + 4;
            while p_off < total_end {
                let (c, new_off) = parse_box(data, p_off)?;
                bx.children.push(c);
                p_off = new_off;
            }
            bx.is_container = true;
        }
    } else if &btype == b"stsd" {
        // stsd: FullBox, version(1)+flags(3) + entry_count(4), then N sample entries.
        if payload.len() < 8 {
            bail!("stsd too short");
        }
        bx.header_data = payload[..8].to_vec();
        let mut p_off = payload_start + 8;
        while p_off < total_end {
            let (c, new_off) = parse_box(data, p_off)?;
            bx.children.push(c);
            p_off = new_off;
        }
        bx.is_container = true;
    } else {
        bx.header_data = payload.to_vec();
    }

    Ok((bx, total_end))
}

impl Mp4Box {
    /// Serialize this box into `out`, computing its size header.
    pub fn encode(&self, out: &mut Vec<u8>) {
        let size_pos = out.len();
        write_u32(out, 0); // placeholder
        out.extend_from_slice(&self.btype);
        if let Some(u) = self.uuid {
            out.extend_from_slice(&u);
        }
        out.extend_from_slice(&self.header_data);
        if self.is_container {
            for c in &self.children {
                c.encode(out);
            }
        }
        let total = out.len() - size_pos;
        let total_u32: u32 = total
            .try_into()
            .expect("mp4 box > 4 GiB (largesize not used here)");
        out[size_pos..size_pos + 4].copy_from_slice(&total_u32.to_be_bytes());
    }

    pub fn type_str(&self) -> String {
        String::from_utf8_lossy(&self.btype).to_string()
    }

    pub fn find_child(&self, btype: &[u8; 4]) -> Option<&Mp4Box> {
        self.children.iter().find(|c| &c.btype == btype)
    }

    pub fn find_child_mut(&mut self, btype: &[u8; 4]) -> Option<&mut Mp4Box> {
        self.children.iter_mut().find(|c| &c.btype == btype)
    }
}

// ---------------------------------------------------------------------------
// moov: remove encryption boxes, rewrite enca -> original format
// ---------------------------------------------------------------------------

/// Rewrite `moov` in-place to drop encryption metadata. Returns the
/// original sample-entry format we saw (e.g. `b"alac"`), if any.
fn decrypt_moov(moov: &mut Mp4Box) -> Result<[u8; 4]> {
    let mut original_fmt: Option<[u8; 4]> = None;

    // Drop any `pssh` siblings inside moov (including nested).
    remove_pssh_recursive(moov);

    let traks = moov.children.iter_mut().filter(|c| &c.btype == b"trak");
    for trak in traks {
        let Some(mdia) = trak.find_child_mut(b"mdia") else {
            continue;
        };
        let Some(minf) = mdia.find_child_mut(b"minf") else {
            continue;
        };
        let Some(stbl) = minf.find_child_mut(b"stbl") else {
            continue;
        };

        // Remove encryption-related sbgp/sgpd from stbl.
        filter_seig_seam(&mut stbl.children);

        let Some(stsd) = stbl.find_child_mut(b"stsd") else {
            continue;
        };
        for entry in stsd.children.iter_mut() {
            if &entry.btype == b"enca" || &entry.btype == b"encv" {
                // Read frma to find original format.
                let sinf_idx = entry.children.iter().position(|c| &c.btype == b"sinf");
                if let Some(idx) = sinf_idx {
                    let sinf = entry.children.remove(idx);
                    if let Some(frma) = sinf.find_child(b"frma") {
                        if frma.header_data.len() >= 4 {
                            let mut fmt = [0u8; 4];
                            fmt.copy_from_slice(&frma.header_data[..4]);
                            entry.btype = fmt;
                            original_fmt = Some(fmt);
                        }
                    }
                }
            }
        }
        // Apple Music often emits 2 identical-format sample entries in stsd
        // (cuetools and Symphonia don't accept this). Collapse to a single
        // entry when all children share the same box type. Mirrors the Go
        // downloader's `sanitizeInit`.
        if stsd.children.len() > 1 {
            let first = stsd.children[0].btype;
            if stsd.children.iter().all(|c| c.btype == first) {
                stsd.children.truncate(1);
                if stsd.header_data.len() >= 8 {
                    stsd.header_data[4..8].copy_from_slice(&1u32.to_be_bytes());
                }
            }
        }
    }
    // Also force every trex (in mvex) to reference sample-description-index 1.
    if let Some(mvex) = moov.find_child_mut(b"mvex") {
        for trex in mvex.children.iter_mut().filter(|c| &c.btype == b"trex") {
            // FullBox: v+flags(4) + track_id(4) + default_sample_description_index(4)
            if trex.header_data.len() >= 12 {
                trex.header_data[8..12].copy_from_slice(&1u32.to_be_bytes());
            }
        }
    }
    original_fmt.ok_or_else(|| anyhow!("no enca/encv sample entry found in moov"))
}

fn remove_pssh_recursive(bx: &mut Mp4Box) {
    if bx.is_container {
        bx.children.retain(|c| &c.btype != b"pssh");
        for c in &mut bx.children {
            remove_pssh_recursive(c);
        }
    }
}

fn filter_seig_seam(children: &mut Vec<Mp4Box>) {
    children.retain(|c| {
        let t = &c.btype;
        if t != b"sbgp" && t != b"sgpd" {
            return true;
        }
        // FullBox: version(1)+flags(3)+grouping_type(4)
        if c.header_data.len() < 8 {
            return true;
        }
        let gtype = &c.header_data[4..8];
        !(gtype == b"seig" || gtype == b"seam")
    });
}

// ---------------------------------------------------------------------------
// moof: decrypt samples via wrapper, strip encryption boxes, fix trun.data_offset
// ---------------------------------------------------------------------------

/// One contiguous run of samples within a traf, extracted from a `trun` box.
struct TrunRun {
    /// Byte offset in mdat where samples start (absolute in the mdat payload).
    mdat_offset: usize,
    /// Per-sample sizes.
    sample_sizes: Vec<u32>,
}

/// All info the wrapper protocol needs about a single moof/mdat pair.
struct FragmentInfo {
    runs: Vec<TrunRun>,
}

/// Parse relevant trafs from a moof into a flat list of runs. We need to
/// inspect the tfhd and trun boxes to find sample sizes and mdat offsets.
fn collect_fragment_runs(moof: &Mp4Box, moof_size_before: usize) -> Result<FragmentInfo> {
    let mut runs: Vec<TrunRun> = Vec::new();
    for traf in moof.children.iter().filter(|c| &c.btype == b"traf") {
        // tfhd: FullBox, flags, track_id[4], [base_data_offset u64 if 0x1],
        // [sample_description_index u32 if 0x2], [default_sample_duration u32 if 0x8],
        // [default_sample_size u32 if 0x10], [default_sample_flags u32 if 0x20]
        let tfhd = traf
            .find_child(b"tfhd")
            .ok_or_else(|| anyhow!("traf without tfhd"))?;
        if tfhd.header_data.len() < 8 {
            bail!("tfhd too short");
        }
        let flags = u32::from_be_bytes([
            0,
            tfhd.header_data[1],
            tfhd.header_data[2],
            tfhd.header_data[3],
        ]);
        let mut tfhd_off = 8;
        let mut default_sample_size: u32 = 0;
        if (flags & 0x000001) != 0 {
            tfhd_off += 8;
        }
        if (flags & 0x000002) != 0 {
            tfhd_off += 4;
        }
        if (flags & 0x000008) != 0 {
            tfhd_off += 4;
        }
        if (flags & 0x000010) != 0 {
            if tfhd.header_data.len() < tfhd_off + 4 {
                bail!("tfhd truncated at default_sample_size");
            }
            default_sample_size =
                read_u32(&tfhd.header_data, tfhd_off);
            tfhd_off += 4;
        }
        let _ = tfhd_off;

        // trun: FullBox, flags, sample_count u32, [data_offset i32 if 0x1],
        // [first_sample_flags u32 if 0x4], then per-sample records of up to
        // 4 u32 fields determined by flags 0x100/0x200/0x400/0x800.
        for trun in traf.children.iter().filter(|c| &c.btype == b"trun") {
            let d = &trun.header_data;
            if d.len() < 8 {
                bail!("trun too short");
            }
            let t_flags = u32::from_be_bytes([0, d[1], d[2], d[3]]);
            let sample_count = read_u32(d, 4);
            let mut cur = 8;
            let data_offset: i32 = if (t_flags & 0x000001) != 0 {
                if d.len() < cur + 4 {
                    bail!("trun truncated at data_offset");
                }
                let v = i32::from_be_bytes(d[cur..cur + 4].try_into().unwrap());
                cur += 4;
                v
            } else {
                0
            };
            if (t_flags & 0x000004) != 0 {
                cur += 4;
            }
            let sample_duration_present = (t_flags & 0x000100) != 0;
            let sample_size_present = (t_flags & 0x000200) != 0;
            let sample_flags_present = (t_flags & 0x000400) != 0;
            let sample_cto_present = (t_flags & 0x000800) != 0;
            let per_sample_bytes = (sample_duration_present as usize
                + sample_size_present as usize
                + sample_flags_present as usize
                + sample_cto_present as usize)
                * 4;

            let mut sizes = Vec::with_capacity(sample_count as usize);
            for _ in 0..sample_count {
                if d.len() < cur + per_sample_bytes {
                    bail!("trun truncated at per-sample records");
                }
                let mut field_off = cur;
                if sample_duration_present {
                    field_off += 4;
                }
                let ssize = if sample_size_present {
                    let v = read_u32(d, field_off);
                    field_off += 4;
                    v
                } else {
                    default_sample_size
                };
                if sample_flags_present {
                    field_off += 4;
                }
                if sample_cto_present {
                    field_off += 4;
                }
                sizes.push(ssize);
                cur += per_sample_bytes;
            }

            // data_offset is "from start of the moof box" when default-base-is-moof
            // is set or when that trun carries the flag itself. In Apple Music
            // this is always the case. So the absolute position within the
            // file of the first sample is `moof_file_offset + data_offset`.
            // Since we're handing off mdat as its own payload, the mdat-relative
            // offset is `data_offset - (moof_size_before + 8)` (8 for the
            // mdat header). If there is an intervening free/uuid/etc. box,
            // the caller must pass its byte-length in `moof_size_before` as
            // the full distance from start of moof to start of mdat's payload.
            let mdat_payload_start_from_moof: i64 = (moof_size_before as i64) + 8;
            let rel = (data_offset as i64) - mdat_payload_start_from_moof;
            if rel < 0 {
                bail!(
                    "negative mdat-relative offset ({rel}); data_offset={data_offset}, moof_size_before={moof_size_before}"
                );
            }
            runs.push(TrunRun {
                mdat_offset: rel as usize,
                sample_sizes: sizes,
            });
        }
    }
    Ok(FragmentInfo { runs })
}

/// If a traf's tfhd sets `sample_description_index_present` (flag 0x02),
/// force the value to 1 so it references the (possibly collapsed) single
/// stsd entry we keep in moov.
fn force_sample_desc_index_1(moof: &mut Mp4Box) {
    for traf in moof.children.iter_mut().filter(|c| &c.btype == b"traf") {
        let Some(tfhd) = traf.find_child_mut(b"tfhd") else {
            continue;
        };
        if tfhd.header_data.len() < 8 {
            continue;
        }
        let flags = u32::from_be_bytes([
            0,
            tfhd.header_data[1],
            tfhd.header_data[2],
            tfhd.header_data[3],
        ]);
        if (flags & 0x000002) == 0 {
            continue;
        }
        let mut off = 8;
        if (flags & 0x000001) != 0 {
            off += 8;
        }
        if tfhd.header_data.len() < off + 4 {
            continue;
        }
        tfhd.header_data[off..off + 4].copy_from_slice(&1u32.to_be_bytes());
    }
}

/// After decryption, strip encryption-related boxes from every traf and
/// return the number of bytes removed from the moof.
fn strip_moof_encryption(moof: &mut Mp4Box) -> usize {
    // PIFF-senc UUID: 8974dbce-7be7-4c51-84f9-7148f9882554
    const PIFF_SENC: [u8; 16] = [
        0xa2, 0x39, 0x4f, 0x52, 0x5a, 0x9b, 0x4f, 0x14, 0xa2, 0x44, 0x6c, 0x42, 0x7c, 0x64, 0x8d,
        0xf4,
    ];
    // (There are two UUIDs used for senc: the official piff one above, and
    // another widely used. Accept both.)
    const PIFF_SENC_ALT: [u8; 16] = [
        0x89, 0x74, 0xdb, 0xce, 0x7b, 0xe7, 0x4c, 0x51, 0x84, 0xf9, 0x71, 0x48, 0xf9, 0x88, 0x25,
        0x54,
    ];

    let before = encoded_len(moof);

    // Drop pssh at moof level.
    moof.children.retain(|c| &c.btype != b"pssh");

    for traf in moof.children.iter_mut().filter(|c| &c.btype == b"traf") {
        traf.children.retain(|c| {
            let t = &c.btype;
            if t == b"senc" || t == b"saiz" || t == b"saio" {
                return false;
            }
            if t == b"uuid" {
                if let Some(u) = c.uuid {
                    if u == PIFF_SENC || u == PIFF_SENC_ALT {
                        return false;
                    }
                }
            }
            if (t == b"sbgp" || t == b"sgpd") && c.header_data.len() >= 8 {
                let gtype = &c.header_data[4..8];
                if gtype == b"seig" || gtype == b"seam" {
                    return false;
                }
            }
            true
        });
    }

    let after = encoded_len(moof);
    before - after
}

/// Rewrite each traf's trun.data_offset by subtracting the given delta.
fn fixup_trun_data_offsets(moof: &mut Mp4Box, bytes_removed: usize) {
    if bytes_removed == 0 {
        return;
    }
    for traf in moof.children.iter_mut().filter(|c| &c.btype == b"traf") {
        for trun in traf.children.iter_mut().filter(|c| &c.btype == b"trun") {
            let d = &mut trun.header_data;
            if d.len() < 8 {
                continue;
            }
            let flags = u32::from_be_bytes([0, d[1], d[2], d[3]]);
            if (flags & 0x000001) == 0 {
                continue;
            }
            if d.len() < 12 {
                continue;
            }
            let cur = i32::from_be_bytes(d[8..12].try_into().unwrap());
            let new = cur - (bytes_removed as i32);
            d[8..12].copy_from_slice(&new.to_be_bytes());
        }
    }
}

fn encoded_len(bx: &Mp4Box) -> usize {
    // 8 bytes header + uuid? + header_data + children
    let mut n = 8;
    if bx.uuid.is_some() {
        n += 16;
    }
    n += bx.header_data.len();
    if bx.is_container {
        for c in &bx.children {
            n += encoded_len(c);
        }
    }
    n
}

// ---------------------------------------------------------------------------
// Wrapper protocol
// ---------------------------------------------------------------------------

/// A TCP connection to the wrapper decryption service.
pub struct WrapperConn {
    stream: TcpStream,
}

impl WrapperConn {
    pub fn connect(addr: &str) -> Result<Self> {
        let stream = TcpStream::connect(addr).with_context(|| format!("connect wrapper {addr}"))?;
        stream.set_nodelay(true).ok();
        Ok(Self { stream })
    }

    fn write_all(&mut self, bytes: &[u8]) -> Result<()> {
        self.stream.write_all(bytes).context("wrapper write")
    }

    fn read_exact(&mut self, bytes: &mut [u8]) -> Result<()> {
        self.stream.read_exact(bytes).context("wrapper read")
    }

    fn send_string(&mut self, s: &str) -> Result<()> {
        let b = s.as_bytes();
        if b.len() > 255 {
            bail!("wrapper string too long: {}", b.len());
        }
        let len = [b.len() as u8];
        self.write_all(&len)?;
        self.write_all(b)
    }

    fn switch_keys(&mut self) -> Result<()> {
        self.write_all(&[0, 0, 0, 0])
    }

    fn close_session(&mut self) -> Result<()> {
        self.write_all(&[0, 0, 0, 0, 0])
    }

    /// Full-subsample CBCS decrypt: send uint32 LE length followed by data,
    /// read back `length` decrypted bytes into the same slice.
    ///
    /// `length` must be a multiple of 16; if `sample` is larger, the trailing
    /// bytes are left untouched (the wrapper returns them as-is in most
    /// implementations, and the Go reference explicitly truncates).
    fn decrypt_full_subsample(&mut self, sample: &mut [u8]) -> Result<()> {
        let trunc_len = sample.len() & !0xf;
        self.write_all(&(trunc_len as u32).to_le_bytes())?;
        if trunc_len == 0 {
            return Ok(());
        }
        self.write_all(&sample[..trunc_len])?;
        self.read_exact(&mut sample[..trunc_len])
    }
}

const PREFETCH_KEY: &str = "skd://itunes.apple.com/P000000000/s1/e1";

/// Decrypt an encrypted Apple Music fMP4.
///
/// `key_uris` gives the `skd://...` key URI for each fragment in file order.
/// Its length must equal the number of moof boxes in `data`.
///
/// `adam_id` is the song's Apple Music adam id (numeric string).
///
/// `wrapper_addr` is the TCP address of the decryption service.
pub fn decrypt_fmp4(
    data: &[u8],
    key_uris: &[String],
    adam_id: &str,
    wrapper_addr: &str,
) -> Result<Vec<u8>> {
    let mut boxes = parse_all(data).context("parse fMP4")?;

    // Fix up moov first (init segment).
    for bx in boxes.iter_mut() {
        if &bx.btype == b"moov" {
            decrypt_moov(bx)?;
        }
    }

    // Collect and decrypt every moof/mdat pair.
    let mut wrapper = WrapperConn::connect(wrapper_addr)?;
    let mut fragment_index = 0usize;
    let mut active_key_uri: Option<String> = None;

    let len = boxes.len();
    let mut i = 0;
    while i < len {
        if &boxes[i].btype != b"moof" {
            i += 1;
            continue;
        }
        // Find the immediately-following mdat (skipping free/etc.).
        let mut mdat_pos = None;
        let mut bytes_between = 0usize;
        for j in (i + 1)..len {
            if &boxes[j].btype == b"mdat" {
                mdat_pos = Some(j);
                break;
            }
            bytes_between += encoded_len(&boxes[j]);
        }
        let mdat_pos = mdat_pos.ok_or_else(|| anyhow!("moof without following mdat"))?;

        let moof_size_before = encoded_len(&boxes[i]) + bytes_between;
        let info = collect_fragment_runs(&boxes[i], moof_size_before)
            .with_context(|| format!("collect runs for fragment {fragment_index}"))?;

        let key_uri = key_uris
            .get(fragment_index)
            .ok_or_else(|| anyhow!("no key URI for fragment {fragment_index}"))?;

        // Only switch key context when the URI actually changes. The wrapper's
        // inner sample-decryption loop stays active otherwise, so we just keep
        // feeding samples against the previously-selected context. This mirrors
        // the Go downloader's behaviour, where the m3u8 parser only attaches a
        // key to segments that declared a fresh `#EXT-X-KEY` line.
        let key_changed = match active_key_uri.as_deref() {
            Some(prev) => prev != key_uri.as_str(),
            None => true,
        };
        if key_changed {
            if active_key_uri.is_some() {
                wrapper.switch_keys()?;
            }
            let adam_arg = if key_uri == PREFETCH_KEY { "0" } else { adam_id };
            if std::env::var("AM_DEBUG").is_ok() {
                eprintln!(
                    "AM[frag {}] SWITCH adam={:?} key_uri={:?} runs={} samples_total={}",
                    fragment_index,
                    adam_arg,
                    key_uri,
                    info.runs.len(),
                    info.runs.iter().map(|r| r.sample_sizes.len()).sum::<usize>()
                );
            }
            wrapper.send_string(adam_arg)?;
            wrapper.send_string(key_uri)?;
            active_key_uri = Some(key_uri.clone());
        } else if std::env::var("AM_DEBUG").is_ok() {
            eprintln!(
                "AM[frag {}] reuse key_uri={:?} runs={} samples_total={}",
                fragment_index,
                key_uri,
                info.runs.len(),
                info.runs.iter().map(|r| r.sample_sizes.len()).sum::<usize>()
            );
        }

        // Decrypt samples in mdat.
        let mdat = &mut boxes[mdat_pos];
        let payload = &mut mdat.header_data;
        for run in info.runs.iter() {
            let mut off = run.mdat_offset;
            for &sz in run.sample_sizes.iter() {
                let end = off + (sz as usize);
                if end > payload.len() {
                    bail!(
                        "sample beyond mdat: off={off} sz={sz} end={end} mdat_len={}",
                        payload.len()
                    );
                }
                wrapper.decrypt_full_subsample(&mut payload[off..end])?;
                off = end;
            }
        }

        // Strip encryption boxes from moof and fix up trun offsets.
        let removed = strip_moof_encryption(&mut boxes[i]);
        fixup_trun_data_offsets(&mut boxes[i], removed);
        force_sample_desc_index_1(&mut boxes[i]);

        fragment_index += 1;
        i = mdat_pos + 1;
    }

    wrapper.close_session().ok();

    // Serialize.
    let mut out = Vec::with_capacity(data.len());
    for bx in &boxes {
        bx.encode(&mut out);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_box_parse_encode() {
        // A minimal ftyp box.
        let src: Vec<u8> = {
            let mut v = Vec::new();
            write_u32(&mut v, 24); // size
            v.extend_from_slice(b"ftyp");
            v.extend_from_slice(b"iso5");
            write_u32(&mut v, 1);
            v.extend_from_slice(b"iso5");
            v.extend_from_slice(b"mp41");
            v
        };
        let boxes = parse_all(&src).unwrap();
        let mut out = Vec::new();
        for b in &boxes {
            b.encode(&mut out);
        }
        assert_eq!(src, out);
    }

    #[test]
    fn container_roundtrip() {
        // moov { trak { header_data="TRAK" as placeholder } }
        // Build by hand:
        let mut trak_payload = Vec::new();
        write_u32(&mut trak_payload, 12); // nested unknown leaf (size 12)
        trak_payload.extend_from_slice(b"tkhd");
        trak_payload.extend_from_slice(&[1, 2, 3, 4]);

        let mut trak = Vec::new();
        write_u32(&mut trak, 8 + trak_payload.len() as u32);
        trak.extend_from_slice(b"trak");
        trak.extend_from_slice(&trak_payload);

        let mut moov = Vec::new();
        write_u32(&mut moov, 8 + trak.len() as u32);
        moov.extend_from_slice(b"moov");
        moov.extend_from_slice(&trak);

        let boxes = parse_all(&moov).unwrap();
        assert_eq!(boxes.len(), 1);
        assert!(boxes[0].is_container);
        assert_eq!(boxes[0].children.len(), 1);
        assert!(boxes[0].children[0].is_container);
        assert_eq!(boxes[0].children[0].children.len(), 1);
        assert_eq!(boxes[0].children[0].children[0].type_str(), "tkhd");

        let mut out = Vec::new();
        for b in &boxes {
            b.encode(&mut out);
        }
        assert_eq!(moov, out);
    }
}
