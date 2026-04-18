use std::io::Cursor;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use rubato::FftFixedIn;
use symphonia::core::codecs::{CODEC_TYPE_NULL, DecoderOptions};
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::audio::decoders::{SymphoniaDecoder, CompressedPacketQueue, FftResampler};
use crate::audio::symphonia_compat::WireCodecParams;
use crate::party::sync_stream::*;
use crate::pipeline::Pullable;

const SR: u32 = 48000;
const CH: usize = 2;

fn test_addr() -> SocketAddr {
    "127.0.0.1:1234".parse().unwrap()
}

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
    for (seq, (dur, data)) in packets.iter().enumerate() {
        mgr.receive(
            addr,
            SyncedFrame::whole(stream_id, seq as u64 + 1, *dur, data.clone()),
        );
    }
    mgr.receive_control(
        addr,
        SyncedControl::Start {
            stream_id,
            party_clock_time: 0,
            seq: 1,
        },
    );
}

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

/// Decode the same packets directly through symphonia (no synced stream manager)
/// to get a ground-truth reference signal for comparison.
fn decode_reference(codec_params: &WireCodecParams, packets: &[(u32, Vec<u8>)]) -> Vec<f32> {
    let params = codec_params.to_symphonia();
    let decoder = symphonia::default::get_codecs()
        .make(&params, &DecoderOptions::default())
        .unwrap();

    let packet_queue = Arc::new(CompressedPacketQueue::new());
    let decoder_node = Arc::new(SymphoniaDecoder::<CH>::new(decoder));
    decoder_node.set_source(packet_queue.clone());

    // Feed all packets into the queue
    for (seq, (dur, data)) in packets.iter().enumerate() {
        packet_queue.push_packet(seq as u64 + 1, *dur, data.clone());
    }

    let mut all = Vec::new();

    if codec_params.sample_rate != SR {
        let resampler = FftFixedIn::new(
            codec_params.sample_rate as usize,
            SR as usize,
            1024,
            1,
            CH,
        )
        .unwrap();
        let resampler_node = Arc::new(FftResampler::<f32, CH, SR>::new(resampler));
        resampler_node.set_source(decoder_node.clone());

        // Pull all resampled audio
        loop {
            match resampler_node.pull(960) {
                Some(buf) => all.extend_from_slice(buf.data()),
                None => break,
            }
        }
    } else {
        use crate::audio::decoders::Interleaver;
        let interleaver = Arc::new(Interleaver::<f32, CH, SR>::new());
        interleaver.set_source(decoder_node.clone());

        loop {
            match interleaver.pull(960) {
                Some(buf) => all.extend_from_slice(buf.data()),
                None => break,
            }
        }
    }

    all
}

/// Feeds the m4a through SyncedAudioStreamManager and compares the output
/// against a direct symphonia decode of the same packets.
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

    // The synced stream should produce approximately the same number of samples
    // as the reference decode.
    let tolerance = reference.len() / 10;
    assert!(
        output.len().abs_diff(reference.len()) <= tolerance,
        "Synced output has {} samples, reference has {} (±{} tolerance).",
        output.len(),
        reference.len(),
        tolerance,
    );

    // Compare actual audio content of the first few samples.
    let compare_len = output.len().min(reference.len()).min(4800);
    let max_diff: f64 = output[..compare_len]
        .iter()
        .zip(&reference[..compare_len])
        .map(|(a, b)| (a - b).abs() as f64)
        .fold(0.0, f64::max);
    assert!(
        max_diff < 0.5,
        "Audio content diverges (max sample diff = {:.4})",
        max_diff,
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

    // Send in order: seq 1, 3, 2, 5, 4, 6
    for &i in &[0usize, 2, 1, 4, 3, 5] {
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

    let packet_queue = Arc::new(CompressedPacketQueue::new());
    let decoder_node = Arc::new(SymphoniaDecoder::<CH>::new(decoder));
    decoder_node.set_source(packet_queue.clone());

    let resampler = FftFixedIn::<f32>::new(
        src_rate as usize,
        SR as usize,
        1024,
        1,
        CH,
    )
    .unwrap();
    let resampler_node = Arc::new(FftResampler::<f32, CH, SR>::new(resampler));
    resampler_node.set_source(decoder_node.clone());

    // Feed all packets
    for (seq, (dur, data)) in packets.iter().enumerate() {
        packet_queue.push_packet(seq as u64 + 1, *dur, data.clone());
    }

    // Pull all resampled audio
    let mut total_resampled = 0usize;
    loop {
        match resampler_node.pull(960) {
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
