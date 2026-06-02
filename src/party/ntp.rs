//! Decentralized party clock synchronization using NTP-like protocol.
//!
//! Provides a shared "party clock" that all participants can sync to, enabling
//! synchronized playback of audio across the network.
//!
//! # Protocol
//!
//! ```text
//! Host A (wants to sync)         Host B, C, D (have party clock)
//!       │
//!       │──── NtpRequest { id, t1 } ──── multicast ────────────►│
//!       │
//!       │                        Each host: schedule response
//!       │                               delay = random(10-50ms)
//!       │
//!       │◄─── NtpResponse { id, t1, t2, t3 } ── (first responder)
//!       │
//!       │                        Others: see response, cancel
//! ```
//!
//! # Offset Calculation (standard NTP)
//!
//! ```text
//! offset = ((t2 - t1) + (t3 - t4)) / 2
//! party_now() = local_now() + offset
//! ```
//!
//! # Decentralization
//!
//! - First host defines party clock (offset = 0)
//! - Any synced host can respond to sync requests
//! - Party clock persists even if original host leaves

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Local, TimeZone};
use rand::Rng;
use rkyv::{Archive, Deserialize, Serialize};
use tokio::time::interval;
use tracing::{debug, info, warn};

use std::net::SocketAddr;

use crate::io::NetworkSender;
use crate::party::network_stream::{NetworkStream, NetworkStreamContext};
use crate::party::tagged_packet::{NTP_TAG, PacketTag, TaggedPacket};
use crate::pipeline::Pushable;

#[derive(Debug, Clone, PartialEq)]
pub struct NtpDebugInfo {
    pub synced: bool,
    pub offset_micros: i64,
    pub raw_offset_micros: Option<i64>,
    pub last_rtt_micros: Option<i64>,
    pub best_rtt_micros: Option<i64>,
    pub offset_sample_count: usize,
    pub local_time_micros: u64,
    pub party_time_micros: u64,
    pub party_time_formatted: String,
    pub pending_requests: usize,
    pub pending_responses: usize,
}

const RESPONSE_DELAY_MIN_MS: u64 = 10;
const RESPONSE_DELAY_MAX_MS: u64 = 50;
const SEEN_RESPONSE_TTL_MS: u64 = 200;
const SYNC_INTERVAL_MS: u64 = 1000;
const REQUEST_TIMEOUT_MS: u64 = 500;
const FIRST_HOST_TIMEOUT_MS: u64 = 1500;
const OFFSET_SAMPLE_WINDOW: usize = 16;
const MAX_SAMPLE_RTT_MICROS: i64 = 250_000;
const MIN_OUTLIER_THRESHOLD_MICROS: i64 = 5_000;
const RTT_WEIGHT_FLOOR_MICROS: f64 = 1_000.0;
const SAMPLE_RECENCY_DECAY: f64 = 0.85;
const LARGE_OFFSET_MICROS: i64 = 500_000;
const MEDIUM_OFFSET_MICROS: i64 = 100_000;
const SMALL_OFFSET_MICROS: i64 = 20_000;

#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
#[rkyv(compare(PartialEq))]
pub enum NtpPacket {
    Request {
        request_id: u64,
        t1: u64,
    },
    Response {
        request_id: u64,
        t1: u64,
        t2: u64,
        t3: u64,
    },
}

struct PendingRequest {
    t1: u64,
    sent_at: Instant,
}

struct SeenResponse {
    request_id: u64,
    seen_at: Instant,
}

struct PendingNtpResponse {
    request_id: u64,
    t1: u64,
    t2: u64,
    respond_at: Instant,
}

struct OffsetSample {
    offset_micros: i64,
    rtt_micros: i64,
}

struct NtpServiceInner {
    offset: i64,
    synced: bool,
    next_request_id: u64,
    pending_requests: HashMap<u64, PendingRequest>,
    seen_responses: Vec<SeenResponse>,
    pending_responses: Vec<PendingNtpResponse>,
    offset_samples: VecDeque<OffsetSample>,
    last_raw_offset_micros: Option<i64>,
    last_rtt_micros: Option<i64>,
    best_rtt_micros: Option<i64>,
    last_sync_request: Option<Instant>,
    first_request_sent_at: Option<Instant>,
}

impl Default for NtpServiceInner {
    fn default() -> Self {
        Self {
            offset: 0,
            synced: false,
            next_request_id: rand::thread_rng().r#gen(),
            pending_requests: HashMap::new(),
            seen_responses: Vec::new(),
            pending_responses: Vec::new(),
            offset_samples: VecDeque::with_capacity(OFFSET_SAMPLE_WINDOW),
            last_raw_offset_micros: None,
            last_rtt_micros: None,
            best_rtt_micros: None,
            last_sync_request: None,
            first_request_sent_at: None,
        }
    }
}

pub struct NtpService {
    inner: Mutex<NtpServiceInner>,
    sender: NetworkSender,
}

impl NtpService {
    pub fn new(sender: NetworkSender) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(NtpServiceInner::default()),
            sender,
        })
    }

    /// Start the NTP service background task.
    /// Must be called from within a Tokio runtime context.
    pub fn start_task(self: &Arc<Self>) {
        let service_clone = self.clone();
        tokio::spawn(async move {
            service_clone.run().await;
        });
    }

    pub fn start_view_task(self: &Arc<Self>, ctx: NetworkStreamContext) {
        let service = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(100));
            loop {
                interval.tick().await;
                ctx.view_state.update_ntp(service.debug_info());
            }
        });
    }

    pub fn local_now_micros() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64
    }

    pub fn party_now(&self) -> u64 {
        let local = Self::local_now_micros();
        let inner = self.inner.lock().unwrap();
        local.saturating_add_signed(inner.offset)
    }

    pub fn is_synced(&self) -> bool {
        self.inner.lock().unwrap().synced
    }

    pub fn debug_info(&self) -> NtpDebugInfo {
        let inner = self.inner.lock().unwrap();
        let local_time = Self::local_now_micros();
        let party_time = if inner.offset >= 0 {
            local_time + inner.offset as u64
        } else {
            local_time.saturating_sub((-inner.offset) as u64)
        };

        let secs = (party_time / 1_000_000) as i64;
        let micros = (party_time % 1_000_000) as u32;
        let party_time_formatted = Local
            .timestamp_opt(secs, micros * 1000)
            .single()
            .map(|dt: DateTime<Local>| dt.format("%Y-%m-%d %H:%M:%S%.3f").to_string())
            .unwrap_or_else(|| "Invalid time".to_string());

        NtpDebugInfo {
            synced: inner.synced,
            offset_micros: inner.offset,
            raw_offset_micros: inner.last_raw_offset_micros,
            last_rtt_micros: inner.last_rtt_micros,
            best_rtt_micros: inner.best_rtt_micros,
            offset_sample_count: inner.offset_samples.len(),
            local_time_micros: local_time,
            party_time_micros: party_time,
            party_time_formatted,
            pending_requests: inner.pending_requests.len(),
            pending_responses: inner.pending_responses.len(),
        }
    }

    pub fn become_first_host(&self) {
        let mut inner = self.inner.lock().unwrap();
        if !inner.synced {
            info!("Becoming first host, defining party clock");
            inner.offset = 0;
            inner.synced = true;
        }
    }

    /// Ask other hosts for the party clock
    pub fn create_sync_request(&self) -> Option<NtpPacket> {
        let mut inner = self.inner.lock().unwrap();

        if let Some(last) = inner.last_sync_request
            && last.elapsed() < Duration::from_millis(REQUEST_TIMEOUT_MS)
        {
            return None;
        }

        let request_id = inner.next_request_id;
        inner.next_request_id += 1;
        let t1 = Self::local_now_micros();
        let now = Instant::now();

        inner
            .pending_requests
            .insert(request_id, PendingRequest { t1, sent_at: now });
        inner.last_sync_request = Some(now);
        if inner.first_request_sent_at.is_none() {
            inner.first_request_sent_at = Some(now);
        }

        // debug!("Creating NTP sync request {}", request_id);
        Some(NtpPacket::Request { request_id, t1 })
    }

    pub fn on_request_received(&self, request_id: u64, t1: u64) {
        let mut inner = self.inner.lock().unwrap();
        if !inner.synced {
            return;
        }

        let local = Self::local_now_micros();
        let t2 = local.saturating_add_signed(inner.offset);
        let delay_ms = rand::thread_rng().gen_range(RESPONSE_DELAY_MIN_MS..=RESPONSE_DELAY_MAX_MS);
        inner.pending_responses.push(PendingNtpResponse {
            request_id,
            t1,
            t2,
            respond_at: Instant::now() + Duration::from_millis(delay_ms),
        });
    }

    pub fn on_response_received(&self, request_id: u64, t1: u64, t2: u64, t3: u64) {
        let mut inner = self.inner.lock().unwrap();

        inner.seen_responses.push(SeenResponse {
            request_id,
            seen_at: Instant::now(),
        });

        let Some(req) = inner.pending_requests.get(&request_id) else {
            return;
        };

        if req.t1 != t1 {
            warn!("NTP response t1 mismatch: expected {}, got {}", req.t1, t1);
            return;
        }

        inner.pending_requests.remove(&request_id);

        let t4 = Self::local_now_micros();

        let t1_i = t1 as i128;
        let t2_i = t2 as i128;
        let t3_i = t3 as i128;
        let t4_i = t4 as i128;

        let offset = ((t2_i - t1_i) + (t3_i - t4_i)) / 2;
        let rtt = (t4_i - t1_i) - (t3_i - t2_i);

        if rtt < 0 || rtt > MAX_SAMPLE_RTT_MICROS as i128 {
            debug!("Ignoring NTP sample: offset={}µs, RTT={}µs", offset, rtt);
            return;
        }

        let Some(filtered_offset) =
            Self::update_offset_filter(&mut inner, offset as i64, rtt as i64)
        else {
            return;
        };

        let previous_offset = inner.offset;
        if !inner.synced {
            inner.offset = filtered_offset;
            inner.synced = true;
        } else {
            let delta = filtered_offset.saturating_sub(inner.offset);
            let alpha = Self::correction_alpha(delta);
            inner.offset = Self::blend_offsets(inner.offset, filtered_offset, alpha);
        }

        info!(
            "NTP sync update: raw_offset={}µs, filtered_offset={}µs, applied_offset={}µs, previous_offset={}µs, RTT={}µs",
            offset, filtered_offset, inner.offset, previous_offset, rtt
        );
    }

    pub fn handle_packet(&self, packet: NtpPacket) {
        match packet {
            NtpPacket::Request { request_id, t1 } => {
                debug!("Received NTP request {} from peer", request_id);
                self.on_request_received(request_id, t1);
            }
            NtpPacket::Response {
                request_id,
                t1,
                t2,
                t3,
            } => {
                debug!("Received NTP response for request {}", request_id);
                self.on_response_received(request_id, t1, t2, t3);
            }
        }
    }

    fn ntp_push(&self, packet: &NtpPacket) {
        let payload = rkyv::to_bytes::<rkyv::rancor::Error>(packet)
            .expect("NtpPacket serialization")
            .into_vec();
        self.sender.push(TaggedPacket {
            tag: NTP_TAG,
            payload,
        });
    }

    fn update_offset_filter(
        inner: &mut NtpServiceInner,
        offset_micros: i64,
        rtt_micros: i64,
    ) -> Option<i64> {
        inner.last_raw_offset_micros = Some(offset_micros);
        inner.last_rtt_micros = Some(rtt_micros);

        inner.offset_samples.push_back(OffsetSample {
            offset_micros,
            rtt_micros,
        });
        while inner.offset_samples.len() > OFFSET_SAMPLE_WINDOW {
            inner.offset_samples.pop_front();
        }

        let mut offsets: Vec<_> = inner
            .offset_samples
            .iter()
            .map(|sample| sample.offset_micros)
            .collect();
        let median = Self::median(&mut offsets);

        let filtered_samples: Vec<(usize, &OffsetSample)> = if inner.offset_samples.len() >= 3 {
            let mut deviations: Vec<_> = inner
                .offset_samples
                .iter()
                .map(|sample| sample.offset_micros.saturating_sub(median).abs())
                .collect();
            let mad = Self::median(&mut deviations);
            let threshold = MIN_OUTLIER_THRESHOLD_MICROS.max(mad.saturating_mul(3));

            inner
                .offset_samples
                .iter()
                .enumerate()
                .filter(|(_, sample)| {
                    sample.offset_micros.saturating_sub(median).abs() <= threshold
                })
                .collect()
        } else {
            inner.offset_samples.iter().enumerate().collect()
        };

        let best = filtered_samples
            .iter()
            .min_by_key(|(_, sample)| sample.rtt_micros)?;
        inner.best_rtt_micros = Some(best.1.rtt_micros);

        let newest_index = inner.offset_samples.len().saturating_sub(1);
        let mut weighted_sum = 0.0;
        let mut weight_sum = 0.0;
        for (index, sample) in filtered_samples {
            let sample_age = newest_index.saturating_sub(index) as i32;
            let recency_weight = SAMPLE_RECENCY_DECAY.powi(sample_age);
            let rtt = (sample.rtt_micros as f64).max(RTT_WEIGHT_FLOOR_MICROS);
            let rtt_weight = 1.0 / (rtt * rtt);
            let weight = recency_weight * rtt_weight;
            weighted_sum += sample.offset_micros as f64 * weight;
            weight_sum += weight;
        }

        if weight_sum == 0.0 {
            return None;
        }

        Some((weighted_sum / weight_sum).round() as i64)
    }

    fn median(values: &mut [i64]) -> i64 {
        values.sort_unstable();
        values[values.len() / 2]
    }

    fn correction_alpha(delta_micros: i64) -> f64 {
        let abs_delta = delta_micros.abs();
        if abs_delta >= LARGE_OFFSET_MICROS {
            0.75
        } else if abs_delta >= MEDIUM_OFFSET_MICROS {
            0.5
        } else if abs_delta >= SMALL_OFFSET_MICROS {
            0.25
        } else {
            0.1
        }
    }

    fn blend_offsets(current: i64, target: i64, alpha: f64) -> i64 {
        let delta = target.saturating_sub(current) as f64;
        current.saturating_add((delta * alpha).round() as i64)
    }

    async fn run(&self) {
        info!("NTP service task started");

        let mut sync_interval = interval(Duration::from_millis(SYNC_INTERVAL_MS));
        let mut cleanup_interval = interval(Duration::from_secs(1));
        let mut first_host_check = interval(Duration::from_millis(100));
        let mut response_poll = interval(Duration::from_millis(5));

        loop {
            tokio::select! {
                _ = sync_interval.tick() => {
                    if let Some(req) = self.create_sync_request() {
                        self.ntp_push(&req);
                    }
                }
                _ = cleanup_interval.tick() => {
                    let now = Instant::now();
                    let timeout = Duration::from_millis(REQUEST_TIMEOUT_MS);
                    let mut inner = self.inner.lock().unwrap();
                    inner.pending_requests.retain(|_, req| now.duration_since(req.sent_at) < timeout);

                    let ttl = Duration::from_millis(SEEN_RESPONSE_TTL_MS);
                    inner.seen_responses.retain(|s| now.duration_since(s.seen_at) < ttl);
                }
                _ = first_host_check.tick() => {
                    let mut inner = self.inner.lock().unwrap();
                    if !inner.synced
                        && let Some(first_sent) = inner.first_request_sent_at
                            && first_sent.elapsed() >= Duration::from_millis(FIRST_HOST_TIMEOUT_MS) {
                                info!("No NTP response received after {}ms, becoming first host", FIRST_HOST_TIMEOUT_MS);
                                inner.offset = 0;
                                inner.synced = true;
                            }
                }
                _ = response_poll.tick() => {
                    let now = Instant::now();
                    let to_send: Vec<_> = {
                        let mut inner = self.inner.lock().unwrap();
                        let ttl = Duration::from_millis(SEEN_RESPONSE_TTL_MS);
                        inner.seen_responses.retain(|s| now.duration_since(s.seen_at) < ttl);
                        let ready: Vec<_> = inner
                            .pending_responses
                            .iter()
                            .filter(|r| now >= r.respond_at)
                            .filter(|r| !inner.seen_responses.iter().any(|s| s.request_id == r.request_id))
                            .map(|r| (r.request_id, r.t1, r.t2))
                            .collect();
                        inner.pending_responses.retain(|r| now < r.respond_at);
                        ready
                    };
                    for (request_id, t1, t2) in to_send {
                        let local = Self::local_now_micros();
                        let offset = self.inner.lock().unwrap().offset;
                        let t3 = local.saturating_add_signed(offset);
                        debug!("Sending NTP response for request {}", request_id);
                        self.ntp_push(&NtpPacket::Response { request_id, t1, t2, t3 });
                    }
                }
            }
        }
    }
}

impl<S: crate::audio::AudioSample, const C: usize, const SR: u32> NetworkStream<S, C, SR>
    for NtpService
{
    fn tags(&self) -> &'static [PacketTag] {
        &[NTP_TAG]
    }

    fn handle(&self, _source: SocketAddr, _tag: PacketTag, bytes: &[u8]) -> anyhow::Result<()> {
        let packet = rkyv::from_bytes::<NtpPacket, rkyv::rancor::Error>(bytes)
            .map_err(|e| anyhow::anyhow!("NtpPacket deserialize: {:?}", e))?;
        self.handle_packet(packet);
        Ok(())
    }

    fn start(self: Arc<Self>, ctx: NetworkStreamContext) {
        NtpService::start_task(&self);
        self.start_view_task(ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::UdpSocket;
    use tokio::time::sleep;

    fn test_service() -> Arc<NtpService> {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let addr = "127.0.0.1:9999".parse().unwrap();
        let sender = NetworkSender::new(
            socket,
            addr,
            Arc::new(std::sync::Mutex::new(crate::io::SendTarget::Multicast)),
        );
        let service = NtpService::new(sender);
        service.start_task();
        service
    }

    #[tokio::test]
    async fn test_first_host_sync() {
        let service = test_service();
        assert!(!service.is_synced());

        service.become_first_host();
        assert!(service.is_synced());

        let party_time = service.party_now();
        let local_time = NtpService::local_now_micros();
        assert!((party_time as i64 - local_time as i64).abs() < 1000);
    }

    #[tokio::test]
    async fn test_sync_request_creation() {
        let service = test_service();

        sleep(Duration::from_millis(150)).await;

        let debug = service.debug_info();
        assert!(
            debug.pending_requests >= 1,
            "NTP service should have sent at least one sync request"
        );
    }

    #[tokio::test]
    async fn test_offset_calculation() {
        let service = test_service();

        sleep(Duration::from_millis(150)).await;

        let (request_id, t1) = {
            let inner = service.inner.lock().unwrap();
            let (&id, req) = inner
                .pending_requests
                .iter()
                .next()
                .expect("Should have at least one pending request");
            (id, req.t1)
        };

        let simulated_offset: i64 = 10000;
        let t2 = (t1 as i64 + simulated_offset) as u64;
        let t3 = t2 + 100;

        service.on_response_received(request_id, t1, t2, t3);

        assert!(service.is_synced());
    }
}
