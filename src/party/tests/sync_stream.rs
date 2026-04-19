use std::io::Cursor;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use rubato::{FftFixedIn, Resampler};
use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::{CODEC_TYPE_NULL, DecoderOptions};
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::audio::buffers::simple_buffer::SimpleBuffer;
use crate::audio::decoders::{CompressedPacket, FftResampler, Interleaver, SymphoniaDecoder};
use crate::audio::symphonia_compat::WireCodecParams;
use crate::party::sync_stream::*;
use crate::pipeline::{GraphNode, Pullable, Pushable};

const SR: u32 = 48000;
const CH: usize = 2;

fn test_addr() -> SocketAddr {
    "127.0.0.1:1234".parse().unwrap()
}

/// Load raw compressed packets from the test asset.
/// Returns (codec_params, Vec<(dur, data)>).
fn load_packets(limit: usize) -> (WireCodecParams, Vec<(u32, Vec<u8>)>) {
    let data = std::fs::read("assets/read_you.m4a").expect("assets/read_you.m4a not found");
    let mss = MediaSourceStream::new(Box::new(Cursor::new(data)), Default::default());
    let mut hint = Hint::new();
    hint.with_extension("m4a");

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .unwrap();
    let mut format = probed.format;

    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .unwrap();
    let codec_params = WireCodecParams::from_symphonia(&track.codec_params).unwrap();
    let track_id = track.id;

    let mut packets = Vec::new();
    loop {
        match format.next_packet() {
            Ok(pkt) if pkt.track_id() == track_id => {
                packets.push((pkt.dur as u32, pkt.data.to_vec()));
                if packets.len() >= limit {
                    break;
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    (codec_params, packets)
}

fn make_manager(clock: Arc<AtomicU64>) -> SyncedAudioStreamManager<f32, CH, SR> {
    let c = clock.clone();
    SyncedAudioStreamManager::<f32, CH, SR>::new(move || c.load(Ordering::Relaxed))
}

/// Sets up and starts a synced stream: meta → start → feed packets.
///
/// The Start command is sent BEFORE packets to match real-world ordering
/// (sender broadcasts Start, then streams packets). This avoids the reset
/// path in `receive_control` which clears already-queued packets.
fn feed_and_start(
    mgr: &SyncedAudioStreamManager<f32, CH, SR>,
    addr: SocketAddr,
    codec_params: WireCodecParams,
    packets: &[(u32, Vec<u8>)],
    stream_id: SyncedStreamId,
) {
    let meta = SyncedStreamMeta {
        stream_id,
        file_name: "read_you.m4a".to_string(),
        total_frames: packets.len() as u64,
        total_samples: packets.iter().map(|(d, _)| *d as u64).sum(),
        codec_params,
    };
    mgr.receive_meta(addr, meta);
    // Start BEFORE feeding packets (seq=1 matches initial next_feed_seq=1).
    mgr.receive_control(
        addr,
        SyncedControl::Start {
            stream_id,
            party_clock_time: 0,
            seq: 1,
        },
    );
    for (seq, (dur, data)) in packets.iter().enumerate() {
        mgr.receive(
            addr,
            SyncedFrame::whole(stream_id, seq as u64 + 1, *dur, data.clone()),
        );
    }
}

/// Pull all available audio from the manager.
fn pull_all(mgr: &SyncedAudioStreamManager<f32, CH, SR>, clock: &Arc<AtomicU64>) -> Vec<f32> {
    const CHUNK: usize = 480;
    let mut samples = Vec::new();
    clock.store(0, Ordering::Relaxed);
    for _ in 0..1_000_000 {
        match mgr.pull_and_mix(CHUNK) {
            Some(buf) => {
                samples.extend_from_slice(buf.data());
            }
            None => break,
        }
    }
    samples
}

/// Decode packets through our pipeline nodes (SymphoniaDecoder + Resampler/Interleaver)
/// using the push-based pipeline, to get a reference signal.
fn decode_reference(codec_params: &WireCodecParams, packets: &[(u32, Vec<u8>)]) -> Vec<f32> {
    let params = codec_params.to_symphonia();
    let decoder = symphonia::default::get_codecs()
        .make(&params, &DecoderOptions::default())
        .unwrap();

    let output_buffer = Arc::new(SimpleBuffer::<f32, CH, SR>::new());
    let decoder_node = Arc::new(SymphoniaDecoder::<CH>::new(decoder));

    let pipeline_head: Arc<dyn Pushable<CompressedPacket>> = if codec_params.sample_rate != SR {
        let resampler = FftFixedIn::new(
            codec_params.sample_rate as usize,
            SR as usize,
            1024,
            1,
            CH,
        )
        .unwrap();
        let resampler_node = Arc::new(FftResampler::<f32, CH, SR>::new(resampler));
        let resampler_graph = Arc::new(GraphNode::new(resampler_node));
        resampler_graph.add_output(output_buffer.clone());
        let decoder_graph = Arc::new(GraphNode::new(decoder_node));
        decoder_graph.add_output(resampler_graph);
        decoder_graph
    } else {
        let interleaver_node = Arc::new(Interleaver::<f32, CH, SR>::new());
        let interleaver_graph = Arc::new(GraphNode::new(interleaver_node));
        interleaver_graph.add_output(output_buffer.clone());
        let decoder_graph = Arc::new(GraphNode::new(decoder_node));
        decoder_graph.add_output(interleaver_graph);
        decoder_graph
    };

    // Push all packets through the pipeline
    for (seq, (dur, data)) in packets.iter().enumerate() {
        let _ = seq;
        pipeline_head.push(CompressedPacket {
            dur: *dur,
            data: data.clone(),
        });
    }

    // Pull all decoded audio from the output buffer
    let mut all = Vec::new();
    loop {
        match output_buffer.pull(960) {
            Some(buf) => all.extend_from_slice(buf.data()),
            None => break,
        }
    }
    all
}

/// Decode via symphonia's full container reader (with proper timestamps)
/// to get a "ground truth" PCM reference. This is what the audio SHOULD
/// sound like — symphonia handles encoder delay trimming, gapless info, etc.
fn decode_ground_truth() -> (WireCodecParams, Vec<f32>, u32) {
    let data = std::fs::read("assets/read_you.m4a").expect("assets/read_you.m4a not found");
    let mss = MediaSourceStream::new(Box::new(Cursor::new(data)), Default::default());
    let mut hint = Hint::new();
    hint.with_extension("m4a");

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .unwrap();
    let mut format = probed.format;

    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .unwrap();
    let codec_params = WireCodecParams::from_symphonia(&track.codec_params).unwrap();
    let track_id = track.id;
    let sample_rate = codec_params.sample_rate;

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .unwrap();

    let mut samples = Vec::new();
    loop {
        match format.next_packet() {
            Ok(pkt) if pkt.track_id() == track_id => {
                if let Ok(decoded) = decoder.decode(&pkt) {
                    // Extract interleaved f32 samples
                    let (num_frames, num_src_channels) = match &decoded {
                        AudioBufferRef::F32(buf) => (buf.frames(), buf.spec().channels.count()),
                        AudioBufferRef::S16(buf) => (buf.frames(), buf.spec().channels.count()),
                        AudioBufferRef::S32(buf) => (buf.frames(), buf.spec().channels.count()),
                        _ => continue,
                    };
                    for f in 0..num_frames {
                        for ch in 0..CH {
                            let src_ch = ch % num_src_channels;
                            let sample: f32 = match &decoded {
                                AudioBufferRef::F32(buf) => buf.chan(src_ch)[f],
                                AudioBufferRef::S16(buf) => {
                                    buf.chan(src_ch)[f] as f32 / 32768.0
                                }
                                AudioBufferRef::S32(buf) => {
                                    buf.chan(src_ch)[f] as f32 / 2147483648.0
                                }
                                _ => 0.0,
                            };
                            samples.push(sample);
                        }
                    }
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    (codec_params, samples, sample_rate)
}

// =====================================================================
// Tests
// =====================================================================

/// Feeds the m4a through SyncedAudioStreamManager and compares the output
/// against a direct decode of the same packets through our pipeline nodes.
///
/// Both paths use the same decode pipeline (SymphoniaDecoder + Resampler),
/// so this validates the SyncedAudioStreamManager orchestration (sequencing,
/// fragment reassembly, pull_and_mix) doesn't corrupt audio.
#[test]
fn test_output_matches_reference_decode() {
    let sid = new_stream_id();
    let (codec_params, packets) = load_packets(100);
    let reference = decode_reference(&codec_params, &packets);
    assert!(!reference.is_empty(), "Reference decode produced no samples");

    let clock = Arc::new(AtomicU64::new(0));
    let mgr = make_manager(clock.clone());
    feed_and_start(&mgr, test_addr(), codec_params, &packets, sid);
    let output = pull_all(&mgr, &clock);
    assert!(!output.is_empty(), "Synced stream produced no samples");

    // Sample counts may differ slightly due to pull_and_mix returning partial
    // buffers at the end. Allow a small tolerance (~1 pull chunk = 480 interleaved samples).
    let len_diff = output.len().abs_diff(reference.len());
    assert!(
        len_diff <= 960,
        "Synced output has {} samples, reference has {} (diff {} > 960)",
        output.len(),
        reference.len(),
        len_diff,
    );

    // Audio content should be identical up to the shorter length.
    let compare_len = output.len().min(reference.len());
    let max_diff: f64 = output[..compare_len]
        .iter()
        .zip(&reference[..compare_len])
        .map(|(a, b)| (a - b).abs() as f64)
        .fold(0.0, f64::max);
    assert!(
        max_diff < 1e-6,
        "Audio differs from reference (max sample diff = {:.2e}). \
         This means SyncedAudioStreamManager is corrupting the signal.",
        max_diff,
    );
    if len_diff > 0 {
        eprintln!(
            "Note: SyncedAudioStreamManager produced {} extra samples vs direct pipeline ({} vs {})",
            len_diff,
            output.len(),
            reference.len(),
        );
    }
}

/// Compares our packet-level decode (ts=0 for every packet) against symphonia's
/// container-level decode (with proper timestamps). This reveals whether our
/// approach of stripping timestamps causes any audio differences.
///
/// Note: Comparison is done at the raw decoded PCM level (before resampling)
/// since resampler chunk boundaries differ between the two paths.
#[test]
fn test_pipeline_vs_container_decode() {
    let (codec_params, ground_truth, _src_rate) = decode_ground_truth();
    let (_, packets) = load_packets(usize::MAX);

    // Decode through our packet-level pipeline (ts=0, no resampling)
    let params = codec_params.to_symphonia();
    let decoder = symphonia::default::get_codecs()
        .make(&params, &DecoderOptions::default())
        .unwrap();
    let output_buffer = Arc::new(SimpleBuffer::<f32, CH, SR>::new());
    let decoder_node = Arc::new(SymphoniaDecoder::<CH>::new(decoder));
    let interleaver_node = Arc::new(Interleaver::<f32, CH, SR>::new());
    let interleaver_graph = Arc::new(GraphNode::new(interleaver_node));
    interleaver_graph.add_output(output_buffer.clone());
    let decoder_graph = Arc::new(GraphNode::new(decoder_node));
    decoder_graph.add_output(interleaver_graph);

    for (_seq, (dur, data)) in packets.iter().enumerate() {
        decoder_graph.push(CompressedPacket {
            dur: *dur,
            data: data.clone(),
        });
    }

    let mut pipeline_raw = Vec::new();
    loop {
        match output_buffer.pull(4096) {
            Some(buf) => pipeline_raw.extend_from_slice(buf.data()),
            None => break,
        }
    }

    assert!(!pipeline_raw.is_empty(), "Pipeline produced no samples");
    assert!(!ground_truth.is_empty(), "Ground truth produced no samples");

    let len_diff = pipeline_raw.len() as i64 - ground_truth.len() as i64;
    let len_diff_frames = len_diff / CH as i64;
    eprintln!(
        "Pipeline raw: {} samples ({} frames), Ground truth: {} samples ({} frames), Diff: {} frames",
        pipeline_raw.len(),
        pipeline_raw.len() / CH,
        ground_truth.len(),
        ground_truth.len() / CH,
        len_diff_frames,
    );

    // Check if the outputs are identical (best case: ts=0 has no effect for this codec)
    let compare_len = pipeline_raw.len().min(ground_truth.len());
    let max_diff: f64 = pipeline_raw[..compare_len]
        .iter()
        .zip(&ground_truth[..compare_len])
        .map(|(a, b)| (a - b).abs() as f64)
        .fold(0.0, f64::max);

    eprintln!("Max sample diff (no alignment): {:.2e}", max_diff);
    eprintln!("Sample count diff: {} frames", len_diff_frames);

    if max_diff < 1e-6 && len_diff == 0 {
        eprintln!("PASS: Pipeline decode is bit-exact with container decode for this codec.");
        eprintln!(
            "  The ts=0 approach does NOT affect decoding for {:?}.",
            codec_params.codec
        );
        return;
    }

    // If not identical, find best alignment and report differences
    if len_diff_frames != 0 {
        eprintln!(
            "WARNING: Sample count differs by {} frames. This indicates encoder delay/padding \
             not being trimmed by our pipeline (ts=0 disables symphonia's gapless trimming).",
            len_diff_frames.abs()
        );
    }

    // Cross-correlate to find offset, skipping leading silence
    let pipe_first_nonzero = pipeline_raw.iter().position(|&s| s.abs() > 1e-4).unwrap_or(0);
    let gt_first_nonzero = ground_truth.iter().position(|&s| s.abs() > 1e-4).unwrap_or(0);
    let skip = pipe_first_nonzero.min(gt_first_nonzero);
    let offset = find_best_offset(
        &pipeline_raw[skip..],
        &ground_truth[skip..],
        CH,
    );
    let offset_frames = offset / CH as i64;
    eprintln!("Best alignment offset (after skipping {} silent samples): {} frames", skip, offset_frames);

    // Compare aligned audio
    let (a, b) = if offset >= 0 {
        let o = offset as usize + skip;
        let len = (pipeline_raw.len() - o).min(ground_truth.len() - skip);
        (&pipeline_raw[o..o + len], &ground_truth[skip..skip + len])
    } else {
        let o = (-offset) as usize + skip;
        let len = (ground_truth.len() - o).min(pipeline_raw.len() - skip);
        (&pipeline_raw[skip..skip + len], &ground_truth[o..o + len])
    };

    let num_frames = a.len() / CH;
    let total_rms = {
        let sum_sq: f64 = a.iter().zip(b.iter()).map(|(x, y)| {
            let d = (*x - *y) as f64;
            d * d
        }).sum();
        (sum_sq / a.len() as f64).sqrt()
    };
    eprintln!("Overall RMS error (aligned): {:.2e}", total_rms);

    // Count glitch frames
    let mut glitch_count = 0;
    let glitch_threshold = 0.01;
    for f in 0..num_frames {
        let start = f * CH;
        let mut err_sq = 0.0f64;
        for c in 0..CH {
            let diff = (a[start + c] - b[start + c]) as f64;
            err_sq += diff * diff;
        }
        let rms = (err_sq / CH as f64).sqrt();
        if rms > glitch_threshold {
            if glitch_count < 10 {
                eprintln!(
                    "  High error at frame {}: RMS {:.4} (time {:.3}s)",
                    f, rms, f as f64 / _src_rate as f64
                );
            }
            glitch_count += 1;
        }
    }
    eprintln!(
        "Frames with error > {}: {} / {} ({:.2}%)",
        glitch_threshold, glitch_count, num_frames,
        glitch_count as f64 / num_frames as f64 * 100.0,
    );

    assert!(
        total_rms < 0.01,
        "Aligned audio differs significantly (RMS {:.2e}). \
         The ts=0 approach may be causing decode artifacts for {:?}.",
        total_rms,
        codec_params.codec,
    );
}

/// Detects discontinuities (clicks/pops) in the pipeline output by looking
/// for sudden jumps in sample values between adjacent frames.
#[test]
fn test_no_discontinuities() {
    let (codec_params, packets) = load_packets(200);
    let output = decode_reference(&codec_params, &packets);
    assert!(!output.is_empty(), "No output produced");

    // Compute first derivative (sample-to-sample diff) per channel
    let mut max_jump: f64 = 0.0;
    let mut jump_count = 0;
    let jump_threshold = 0.5; // A sudden jump > 0.5 in normalized [-1,1] is suspicious

    // Skip the first few frames (encoder priming) and last few (padding)
    let skip_frames = 2048;
    let skip_samples = skip_frames * CH;
    if output.len() <= skip_samples * 2 {
        return; // Too short to test
    }

    for i in (skip_samples + CH)..output.len() - skip_samples {
        // Compare with previous sample of the same channel
        let diff = (output[i] - output[i - CH]).abs() as f64;
        if diff > max_jump {
            max_jump = diff;
        }
        if diff > jump_threshold {
            if jump_count < 10 {
                let frame = i / CH;
                let channel = i % CH;
                eprintln!(
                    "  Discontinuity at frame {} ch {}: {:.4} -> {:.4} (jump {:.4}), time {:.3}s",
                    frame,
                    channel,
                    output[i - CH],
                    output[i],
                    diff,
                    frame as f64 / SR as f64
                );
            }
            jump_count += 1;
        }
    }
    eprintln!("Max sample jump: {:.4}, discontinuities > {}: {}", max_jump, jump_threshold, jump_count);

    // Some jumps are normal in music (drums, transients), but many indicate glitches.
    // For a typical music file, we allow a small percentage.
    let total_samples = output.len() - skip_samples * 2;
    let jump_ratio = jump_count as f64 / total_samples as f64;
    // This test is primarily a diagnostic — it prints where discontinuities occur.
    // A high ratio strongly suggests pipeline bugs rather than natural audio.
    assert!(
        jump_ratio < 0.001,
        "Too many discontinuities: {} in {} samples ({:.4}%)",
        jump_count,
        total_samples,
        jump_ratio * 100.0
    );
}

/// Checks the audio output is not silent (non-trivial signal present).
#[test]
fn test_output_not_silent() {
    let sid = new_stream_id();
    let (codec_params, packets) = load_packets(20);

    let clock = Arc::new(AtomicU64::new(0));
    let mgr = make_manager(clock.clone());
    feed_and_start(&mgr, test_addr(), codec_params, &packets, sid);
    let output = pull_all(&mgr, &clock);

    assert!(!output.is_empty(), "No audio output produced");
    let rms = (output.iter().map(|&s| (s as f64).powi(2)).sum::<f64>() / output.len() as f64)
        .sqrt();
    assert!(rms > 1e-4, "Output is silent (RMS = {:.2e})", rms);
}

/// Sends frames out of order and verifies all are eventually decoded and played.
#[test]
fn test_out_of_order_frames_all_decoded() {
    let sid = new_stream_id();
    let (codec_params, packets) = load_packets(6);

    let clock = Arc::new(AtomicU64::new(0));
    let mgr = make_manager(clock.clone());
    let meta = SyncedStreamMeta {
        stream_id: sid,
        file_name: "read_you.m4a".to_string(),
        total_frames: packets.len() as u64,
        total_samples: packets.iter().map(|(d, _)| *d as u64).sum(),
        codec_params,
    };
    mgr.receive_meta(test_addr(), meta);
    // Start before feeding, seq=1 matches initial next_feed_seq.
    mgr.receive_control(
        test_addr(),
        SyncedControl::Start {
            stream_id: sid,
            party_clock_time: 0,
            seq: 1,
        },
    );

    // Send in order: seq 1, 3, 2, 5, 4, 6
    for &i in &[0usize, 2, 1, 4, 3, 5] {
        let (dur, data) = &packets[i];
        mgr.receive(
            test_addr(),
            SyncedFrame::whole(sid, i as u64 + 1, *dur, data.clone()),
        );
    }

    // All 6 packets should have been fed into the packet queue.
    let streams = mgr.active_streams();
    let state = streams.iter().find(|s| s.stream_id == sid).unwrap();
    assert_eq!(
        state.progress.buffered_frames, 6,
        "All 6 frames should be fed despite out-of-order delivery"
    );

    clock.store(0, Ordering::Relaxed);
    assert!(
        mgr.pull_and_mix(480).is_some(),
        "Should produce audio from seq 1"
    );
}

/// Splits one frame across two fragments and verifies it is reassembled before decoding.
#[test]
fn test_fragment_reassembly() {
    let sid = new_stream_id();
    let (codec_params, packets) = load_packets(3);

    let clock = Arc::new(AtomicU64::new(0));
    let mgr = make_manager(clock);
    let meta = SyncedStreamMeta {
        stream_id: sid,
        file_name: "read_you.m4a".to_string(),
        total_frames: packets.len() as u64,
        total_samples: packets.iter().map(|(d, _)| *d as u64).sum(),
        codec_params,
    };
    mgr.receive_meta(test_addr(), meta);

    // Send frame 1 as two fragments
    let (dur1, data1) = &packets[0];
    let mid = data1.len() / 2;
    mgr.receive(
        test_addr(),
        SyncedFrame {
            stream_id: sid,
            sequence_number: 1,
            dur: *dur1,
            fragment_idx: 0,
            fragment_total: 2,
            data: data1[..mid].to_vec(),
        },
    );
    // First fragment only — frame 1 must not be fed yet
    let before = mgr.active_streams();
    let before_state = before.iter().find(|s| s.stream_id == sid).unwrap();
    assert_eq!(
        before_state.progress.buffered_frames, 0,
        "Frame 1 should not be fed with only one fragment"
    );

    mgr.receive(
        test_addr(),
        SyncedFrame {
            stream_id: sid,
            sequence_number: 1,
            dur: *dur1,
            fragment_idx: 1,
            fragment_total: 2,
            data: data1[mid..].to_vec(),
        },
    );
    // Send frames 2 and 3 whole
    for i in 1..3 {
        let (dur, data) = &packets[i];
        mgr.receive(
            test_addr(),
            SyncedFrame::whole(sid, i as u64 + 1, *dur, data.clone()),
        );
    }

    mgr.receive_control(
        test_addr(),
        SyncedControl::Start {
            stream_id: sid,
            party_clock_time: 0,
            seq: 1,
        },
    );

    let streams = mgr.active_streams();
    let state = streams.iter().find(|s| s.stream_id == sid).unwrap();
    assert_eq!(
        state.progress.buffered_frames, 3,
        "All 3 frames (including fragmented) should be fed"
    );
}

/// Checks that the resampler produces the right number of samples for
/// codecs with large frames (e.g. ALAC ~4096 samples).
#[test]
fn test_resampled_sample_count() {
    let (codec_params, packets) = load_packets(10);
    let src_rate = codec_params.sample_rate;

    if src_rate == SR {
        return;
    }

    let params = codec_params.to_symphonia();
    let decoder = symphonia::default::get_codecs()
        .make(&params, &DecoderOptions::default())
        .unwrap();

    let output_buffer = Arc::new(SimpleBuffer::<f32, CH, SR>::new());
    let decoder_node = Arc::new(SymphoniaDecoder::<CH>::new(decoder));

    let resampler = FftFixedIn::<f32>::new(src_rate as usize, SR as usize, 1024, 1, CH).unwrap();
    let resampler_node = Arc::new(FftResampler::<f32, CH, SR>::new(resampler));
    let resampler_graph = Arc::new(GraphNode::new(resampler_node));
    resampler_graph.add_output(output_buffer.clone());
    let decoder_graph = Arc::new(GraphNode::new(decoder_node));
    decoder_graph.add_output(resampler_graph);

    for (_seq, (dur, data)) in packets.iter().enumerate() {
        decoder_graph.push(CompressedPacket {
            dur: *dur,
            data: data.clone(),
        });
    }

    let mut total_resampled = 0usize;
    loop {
        match output_buffer.pull(960) {
            Some(buf) => total_resampled += buf.data().len() / CH,
            None => break,
        }
    }

    let expected_total: usize = packets
        .iter()
        .map(|(dur, _)| *dur as usize * SR as usize / src_rate as usize)
        .sum();

    let ratio = total_resampled as f64 / expected_total as f64;
    assert!(
        ratio > 0.9 && ratio < 1.1,
        "Resampler produced {} frames, expected {} (ratio={:.2}).",
        total_resampled,
        expected_total,
        ratio,
    );
}

/// Verifies that pausing stops audio output.
#[test]
fn test_pause_stops_output() {
    let sid = new_stream_id();
    let (codec_params, packets) = load_packets(10);

    let clock = Arc::new(AtomicU64::new(0));
    let mgr = make_manager(clock.clone());
    feed_and_start(&mgr, test_addr(), codec_params, &packets, sid);

    clock.store(0, Ordering::Relaxed);
    assert!(
        mgr.pull_and_mix(480).is_some(),
        "Should produce audio before pause"
    );

    mgr.receive_control(test_addr(), SyncedControl::Pause { stream_id: sid });

    clock.store(10_000, Ordering::Relaxed);
    assert!(
        mgr.pull_and_mix(480).is_none(),
        "Should produce no audio after pause"
    );
}

/// Verifies that different pull sizes produce the same total audio content.
/// This catches issues with leftover buffer handling at chunk boundaries.
#[test]
fn test_different_pull_sizes_same_output() {
    let (codec_params, packets) = load_packets(50);

    // Decode with pull size 960 (standard)
    let output_960 = decode_with_pull_size(&codec_params, &packets, 960);
    // Decode with pull size 480 (half)
    let output_480 = decode_with_pull_size(&codec_params, &packets, 480);
    // Decode with pull size 1024 (non-standard)
    let output_1024 = decode_with_pull_size(&codec_params, &packets, 1024);

    assert!(!output_960.is_empty(), "960 pull size produced nothing");
    assert!(!output_480.is_empty(), "480 pull size produced nothing");
    assert!(!output_1024.is_empty(), "1024 pull size produced nothing");

    // All should produce the same number of samples
    assert_eq!(
        output_960.len(),
        output_480.len(),
        "Pull size 960 ({}) vs 480 ({}) produced different sample counts",
        output_960.len(),
        output_480.len()
    );
    assert_eq!(
        output_960.len(),
        output_1024.len(),
        "Pull size 960 ({}) vs 1024 ({}) produced different sample counts",
        output_960.len(),
        output_1024.len()
    );

    // Content should be identical
    let max_diff_480: f64 = output_960
        .iter()
        .zip(&output_480)
        .map(|(a, b)| (a - b).abs() as f64)
        .fold(0.0, f64::max);
    let max_diff_1024: f64 = output_960
        .iter()
        .zip(&output_1024)
        .map(|(a, b)| (a - b).abs() as f64)
        .fold(0.0, f64::max);

    assert!(
        max_diff_480 < 1e-6,
        "Pull size 480 diverges from 960: max diff {:.2e}",
        max_diff_480,
    );
    assert!(
        max_diff_1024 < 1e-6,
        "Pull size 1024 diverges from 960: max diff {:.2e}",
        max_diff_1024,
    );
}

// =====================================================================
// Helpers
// =====================================================================

fn decode_with_pull_size(
    codec_params: &WireCodecParams,
    packets: &[(u32, Vec<u8>)],
    pull_size: usize,
) -> Vec<f32> {
    let params = codec_params.to_symphonia();
    let decoder = symphonia::default::get_codecs()
        .make(&params, &DecoderOptions::default())
        .unwrap();

    let output_buffer = Arc::new(SimpleBuffer::<f32, CH, SR>::new());
    let decoder_node = Arc::new(SymphoniaDecoder::<CH>::new(decoder));

    let pipeline_head: Arc<dyn Pushable<CompressedPacket>> = if codec_params.sample_rate != SR {
        let resampler = FftFixedIn::new(
            codec_params.sample_rate as usize,
            SR as usize,
            1024,
            1,
            CH,
        )
        .unwrap();
        let resampler_node = Arc::new(FftResampler::<f32, CH, SR>::new(resampler));
        let resampler_graph = Arc::new(GraphNode::new(resampler_node));
        resampler_graph.add_output(output_buffer.clone());
        let decoder_graph = Arc::new(GraphNode::new(decoder_node));
        decoder_graph.add_output(resampler_graph);
        decoder_graph
    } else {
        let interleaver_node = Arc::new(Interleaver::<f32, CH, SR>::new());
        let interleaver_graph = Arc::new(GraphNode::new(interleaver_node));
        interleaver_graph.add_output(output_buffer.clone());
        let decoder_graph = Arc::new(GraphNode::new(decoder_node));
        decoder_graph.add_output(interleaver_graph);
        decoder_graph
    };

    // Push all packets eagerly
    for (_seq, (dur, data)) in packets.iter().enumerate() {
        pipeline_head.push(CompressedPacket {
            dur: *dur,
            data: data.clone(),
        });
    }

    // Pull with the specified pull_size from the output buffer
    let mut all = Vec::new();
    loop {
        match output_buffer.pull(pull_size) {
            Some(buf) => all.extend_from_slice(buf.data()),
            None => break,
        }
    }
    all
}

/// Uses the resampler with small pull sizes (like cpal's 256-frame callback)
/// and checks for discontinuities at chunk boundaries. This simulates the
/// exact scenario of the cpal output callback pulling resampled audio.
///
/// If the resampler produces boundary artifacts, they show up as sudden
/// jumps that exceed what's expected from the underlying audio.
#[test]
fn test_resampler_no_chunk_boundary_glitches() {
    let (codec_params, packets) = load_packets(50);
    let src_rate = codec_params.sample_rate;
    if src_rate == SR {
        eprintln!("Skipping: source is already at {} Hz, no resampling needed", SR);
        return;
    }

    // Decode + resample with small pulls (256 frames = cpal default)
    let small_output = decode_with_pull_size(&codec_params, &packets, 256);
    // Decode + resample with large pulls (4096 frames)
    let large_output = decode_with_pull_size(&codec_params, &packets, 4096);

    assert!(!small_output.is_empty(), "Small pull produced nothing");
    assert!(!large_output.is_empty(), "Large pull produced nothing");

    // Sample counts should match — pull size shouldn't affect total output
    assert_eq!(
        small_output.len(), large_output.len(),
        "Pull size 256 ({} samples) vs 4096 ({} samples) differ!",
        small_output.len(), large_output.len()
    );

    // Content should be bit-exact
    let max_diff: f64 = small_output.iter().zip(&large_output)
        .map(|(a, b)| (a - b).abs() as f64)
        .fold(0.0, f64::max);

    assert!(
        max_diff < 1e-6,
        "Resampler output differs between pull sizes (max diff = {:.2e}). \
         This indicates chunk-boundary artifacts in the resampler.",
        max_diff,
    );

    // Also check for discontinuities in the small-pull output specifically
    let skip = 2048 * CH; // skip encoder priming
    let mut max_jump: f64 = 0.0;
    let mut jump_count = 0;
    let jump_threshold = 0.3;

    for i in (skip + CH)..small_output.len().saturating_sub(skip) {
        let diff = (small_output[i] - small_output[i - CH]).abs() as f64;
        if diff > max_jump {
            max_jump = diff;
        }
        if diff > jump_threshold {
            if jump_count < 5 {
                let frame = i / CH;
                eprintln!(
                    "  Resampler discontinuity at frame {}: {:.4} -> {:.4} (jump {:.4})",
                    frame, small_output[i - CH], small_output[i], diff
                );
            }
            jump_count += 1;
        }
    }
    eprintln!(
        "Resampler max jump: {:.4}, discontinuities > {}: {}",
        max_jump, jump_threshold, jump_count
    );
}

/// Tests that feeding packets one at a time (simulating network arrival)
/// vs all at once produces the same output through the full synced stream
/// manager. This catches timing-dependent issues in the pipeline.
#[test]
fn test_incremental_feeding_matches_bulk() {
    let sid = new_stream_id();
    let (codec_params, packets) = load_packets(20);

    // Bulk feed: all packets at once
    let clock_bulk = Arc::new(AtomicU64::new(0));
    let mgr_bulk = make_manager(clock_bulk.clone());
    feed_and_start(&mgr_bulk, test_addr(), codec_params.clone(), &packets, sid);
    let bulk_output = pull_all(&mgr_bulk, &clock_bulk);

    // Incremental feed: feed packets one at a time, pulling between feeds
    let sid2 = new_stream_id();
    let clock_inc = Arc::new(AtomicU64::new(0));
    let mgr_inc = make_manager(clock_inc.clone());
    let meta = SyncedStreamMeta {
        stream_id: sid2,
        file_name: "read_you.m4a".to_string(),
        total_frames: packets.len() as u64,
        total_samples: packets.iter().map(|(d, _)| *d as u64).sum(),
        codec_params,
    };
    mgr_inc.receive_meta(test_addr(), meta);
    mgr_inc.receive_control(
        test_addr(),
        SyncedControl::Start {
            stream_id: sid2,
            party_clock_time: 0,
            seq: 1,
        },
    );

    let mut inc_output = Vec::new();
    for (i, (dur, data)) in packets.iter().enumerate() {
        mgr_inc.receive(
            test_addr(),
            SyncedFrame::whole(sid2, i as u64 + 1, *dur, data.clone()),
        );
        // Pull some audio after each packet (like the cpal callback would)
        clock_inc.store(0, Ordering::Relaxed);
        for _ in 0..20 {
            match mgr_inc.pull_and_mix(256) {
                Some(buf) => inc_output.extend_from_slice(buf.data()),
                None => break,
            }
        }
    }
    // Drain remaining
    for _ in 0..10000 {
        match mgr_inc.pull_and_mix(256) {
            Some(buf) => inc_output.extend_from_slice(buf.data()),
            None => break,
        }
    }

    assert!(!bulk_output.is_empty(), "Bulk produced nothing");
    assert!(!inc_output.is_empty(), "Incremental produced nothing");

    let compare_len = bulk_output.len().min(inc_output.len());
    let max_diff: f64 = bulk_output[..compare_len]
        .iter()
        .zip(&inc_output[..compare_len])
        .map(|(a, b)| (a - b).abs() as f64)
        .fold(0.0, f64::max);

    eprintln!(
        "Bulk: {} samples, Incremental: {} samples, Max diff: {:.2e}",
        bulk_output.len(), inc_output.len(), max_diff
    );

    assert!(
        max_diff < 1e-5,
        "Incremental feeding diverges from bulk (max diff = {:.2e}). \
         This indicates timing-dependent bugs in the pipeline.",
        max_diff,
    );
}

// =====================================================================
// Helpers
// =====================================================================

/// Simple resampling helper for ground truth comparison.
/// Used when the codec source rate differs from SR (e.g., AAC, MP3).
#[allow(dead_code)]
fn resample_vec(input: &[f32], from_rate: u32, to_rate: u32, channels: usize) -> Vec<f32> {
    let mut resampler =
        FftFixedIn::<f32>::new(from_rate as usize, to_rate as usize, 1024, 1, channels).unwrap();

    let num_frames = input.len() / channels;
    // De-interleave
    let mut channel_bufs: Vec<Vec<f32>> = (0..channels).map(|_| Vec::with_capacity(num_frames)).collect();
    for f in 0..num_frames {
        for ch in 0..channels {
            channel_bufs[ch].push(input[f * channels + ch]);
        }
    }

    let mut output: Vec<Vec<f32>> = vec![Vec::new(); channels];
    let mut offset = 0;
    while offset < num_frames {
        let needed = resampler.input_frames_next();
        let available = num_frames - offset;

        if available >= needed {
            let chunks: Vec<&[f32]> = channel_bufs.iter().map(|c| &c[offset..offset + needed]).collect();
            let resampled = resampler.process(&chunks, None).unwrap();
            for (ch, data) in output.iter_mut().enumerate() {
                data.extend_from_slice(&resampled[ch]);
            }
            offset += needed;
        } else {
            let padded: Vec<Vec<f32>> = channel_bufs
                .iter()
                .map(|c| {
                    let mut v = c[offset..].to_vec();
                    v.resize(needed, 0.0);
                    v
                })
                .collect();
            let chunks: Vec<&[f32]> = padded.iter().map(|v| v.as_slice()).collect();
            let resampled = resampler.process(&chunks, None).unwrap();
            let output_len = resampled[0].len();
            let keep = output_len * available / needed;
            for (ch, data) in output.iter_mut().enumerate() {
                data.extend_from_slice(&resampled[ch][..keep]);
            }
            offset = num_frames;
        }
    }

    // Re-interleave
    let out_frames = output[0].len();
    let mut result = Vec::with_capacity(out_frames * channels);
    for f in 0..out_frames {
        for ch in 0..channels {
            result.push(output[ch][f]);
        }
    }
    result
}

/// Find the best sample offset to align two signals via cross-correlation.
/// Searches within ±max_offset_frames frames. Returns the offset in samples
/// to apply to `a` (positive means `a` leads `b`).
fn find_best_offset(a: &[f32], b: &[f32], channels: usize) -> i64 {
    // Two-pass search: coarse (every 10 frames) then fine (every frame).
    let max_offset_frames: i64 = 20000;
    let max_offset = max_offset_frames * channels as i64;
    let corr_len = SR as usize * channels; // 1 second

    let compute_corr = |offset: i64| -> f64 {
        let (a_start, b_start, len) = if offset >= 0 {
            let o = offset as usize;
            let len = corr_len.min(a.len().saturating_sub(o)).min(b.len());
            (o, 0usize, len)
        } else {
            let o = (-offset) as usize;
            let len = corr_len.min(a.len()).min(b.len().saturating_sub(o));
            (0usize, o, len)
        };
        if len < channels * 100 {
            return f64::NEG_INFINITY;
        }
        a[a_start..a_start + len]
            .iter()
            .zip(&b[b_start..b_start + len])
            .map(|(&x, &y)| x as f64 * y as f64)
            .sum()
    };

    // Coarse pass: step by 10 frames
    let coarse_step = 10 * channels;
    let mut best_offset: i64 = 0;
    let mut best_corr = f64::NEG_INFINITY;
    let mut offset = -max_offset;
    while offset <= max_offset {
        let corr = compute_corr(offset);
        if corr > best_corr {
            best_corr = corr;
            best_offset = offset;
        }
        offset += coarse_step as i64;
    }

    // Fine pass: search ±10 frames around coarse best
    let fine_range = 10 * channels as i64;
    let fine_start = (best_offset - fine_range).max(-max_offset);
    let fine_end = (best_offset + fine_range).min(max_offset);
    for offset in (fine_start..=fine_end).step_by(channels) {
        let corr = compute_corr(offset);
        if corr > best_corr {
            best_corr = corr;
            best_offset = offset;
        }
    }

    best_offset
}
