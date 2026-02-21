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

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Local, TimeZone};
use rand::Rng;
use rkyv::{Archive, Deserialize, Serialize};
use tokio::time::{interval, sleep};
use tracing::{debug, info, warn};

use crate::io::NetworkSender;
use crate::party::realtime_stream::NetworkPacket;
use crate::pipeline::Pushable;

#[derive(Debug, Clone, PartialEq)]
pub struct NtpDebugInfo {
    pub synced: bool,
    pub offset_micros: i64,
    pub local_time_micros: u64,
    pub party_time_micros: u64,
    pub party_time_formatted: String,
    pub pending_requests: usize,
    pub pending_responses: usize,
}

const RESPONSE_DELAY_MIN_MS: u64 = 10;
const RESPONSE_DELAY_MAX_MS: u64 = 50;
const SEEN_RESPONSE_TTL_MS: u64 = 200;
const SYNC_INTERVAL_MS: u64 = 5000;
const REQUEST_TIMEOUT_MS: u64 = 500;
const FIRST_HOST_TIMEOUT_MS: u64 = 1500;

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

struct NtpServiceInner {
    offset: i64,
    synced: bool,
    next_request_id: u64,
    pending_requests: HashMap<u64, PendingRequest>,
    seen_responses: Vec<SeenResponse>,
    last_sync_request: Option<Instant>,
    first_request_sent_at: Option<Instant>,
}

impl Default for NtpServiceInner {
    fn default() -> Self {
        Self {
            offset: 0,
            synced: false,
            next_request_id: 1,
            pending_requests: HashMap::new(),
            seen_responses: Vec::new(),
            last_sync_request: None,
            first_request_sent_at: None,
        }
    }
}

pub struct NtpService {
    inner: Mutex<NtpServiceInner>,
    sender: NetworkSender,
    shutdown_flag: Arc<AtomicBool>,
}

impl NtpService {
    pub fn new(sender: NetworkSender, shutdown_flag: Arc<AtomicBool>) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(NtpServiceInner::default()),
            sender,
            shutdown_flag,
        })
    }

    /// Start the NTP service background task.
    /// Must be called from within a Tokio runtime context.
    pub fn start(self: &Arc<Self>) {
        let service_clone = self.clone();
        tokio::spawn(async move {
            service_clone.run().await;
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
            local_time_micros: local_time,
            party_time_micros: party_time,
            party_time_formatted,
            pending_requests: inner.pending_requests.len(),
            pending_responses: 0, // No longer tracked in inner
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

        if inner.synced {
            return None;
        }

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

        debug!("Creating NTP sync request {}", request_id);
        Some(NtpPacket::Request { request_id, t1 })
    }

    pub fn on_request_received(self: &Arc<Self>, request_id: u64, t1: u64) {
        let inner = self.inner.lock().unwrap();
        if !inner.synced {
            return;
        }

        let offset = inner.offset;
        let local = Self::local_now_micros();
        let t2 = if offset >= 0 {
            local + offset as u64
        } else {
            local.saturating_sub((-offset) as u64)
        };
        drop(inner);

        let delay_ms = rand::thread_rng().gen_range(RESPONSE_DELAY_MIN_MS..=RESPONSE_DELAY_MAX_MS);
        let self_clone = self.clone();

        tokio::spawn(async move {
            sleep(Duration::from_millis(delay_ms)).await;

            // Check if we still need to send (has someone else answered?)
            let should_send = {
                let mut inner = self_clone.inner.lock().unwrap();
                let now = Instant::now();
                let ttl = Duration::from_millis(SEEN_RESPONSE_TTL_MS);
                inner
                    .seen_responses
                    .retain(|s| now.duration_since(s.seen_at) < ttl);
                !inner
                    .seen_responses
                    .iter()
                    .any(|s| s.request_id == request_id)
            };

            if should_send {
                let local = Self::local_now_micros();
                let inner = self_clone.inner.lock().unwrap();
                let offset = inner.offset;
                let t3 = if offset >= 0 {
                    local + offset as u64
                } else {
                    local.saturating_sub((-offset) as u64)
                };
                let packet = NtpPacket::Response {
                    request_id,
                    t1,
                    t2,
                    t3,
                };
                debug!("Sending NTP response for request {}", request_id);
                self_clone.sender.push(NetworkPacket::Ntp(packet));
            } else {
                debug!(
                    "Cancelling NTP response for request {} (already answered)",
                    request_id
                );
            }
        });
    }

    pub fn on_response_received(&self, request_id: u64, t1: u64, t2: u64, t3: u64) {
        let mut inner = self.inner.lock().unwrap();

        inner.seen_responses.push(SeenResponse {
            request_id,
            seen_at: Instant::now(),
        });

        let Some(req) = inner.pending_requests.remove(&request_id) else {
            return;
        };

        if req.t1 != t1 {
            warn!("NTP response t1 mismatch: expected {}, got {}", req.t1, t1);
            return;
        }

        let t4 = Self::local_now_micros();

        let t1_i = t1 as i128;
        let t2_i = t2 as i128;
        let t3_i = t3 as i128;
        let t4_i = t4 as i128;

        let offset = ((t2_i - t1_i) + (t3_i - t4_i)) / 2;
        let rtt = (t4_i - t1_i) - (t3_i - t2_i);

        info!("NTP sync complete: offset={}µs, RTT={}µs", offset, rtt);

        inner.offset = offset as i64;
        inner.synced = true;
    }

    pub fn handle_packet(self: &Arc<Self>, packet: NtpPacket) {
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

    async fn run(&self) {
        info!("NTP service task started");

        let mut sync_interval = interval(Duration::from_millis(SYNC_INTERVAL_MS));
        let mut cleanup_interval = interval(Duration::from_secs(1));
        let mut first_host_check = interval(Duration::from_millis(100));

        while !self.shutdown_flag.load(Ordering::SeqCst) {
            tokio::select! {
                _ = sync_interval.tick() => {
                    if let Some(req) = self.create_sync_request() {
                        self.sender.push(NetworkPacket::Ntp(req));
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
            }
        }

        info!("NTP service task shutting down");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::UdpSocket;

    fn test_service() -> (Arc<NtpService>, Arc<AtomicBool>) {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let addr = "127.0.0.1:9999".parse().unwrap();
        let sender = NetworkSender::new(socket, addr);
        let shutdown = Arc::new(AtomicBool::new(false));
        let service = NtpService::new(sender, shutdown.clone());
        service.start();
        (service, shutdown)
    }

    #[tokio::test]
    async fn test_first_host_sync() {
        let (service, shutdown) = test_service();
        assert!(!service.is_synced());

        service.become_first_host();
        assert!(service.is_synced());

        let party_time = service.party_now();
        let local_time = NtpService::local_now_micros();
        assert!((party_time as i64 - local_time as i64).abs() < 1000);

        shutdown.store(true, Ordering::SeqCst);
    }

    #[tokio::test]
    async fn test_sync_request_creation() {
        let (service, shutdown) = test_service();

        sleep(Duration::from_millis(150)).await;

        let debug = service.debug_info();
        assert!(
            debug.pending_requests >= 1,
            "NTP service should have sent at least one sync request"
        );

        shutdown.store(true, Ordering::SeqCst);
    }

    #[tokio::test]
    async fn test_offset_calculation() {
        let (service, shutdown) = test_service();

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

        shutdown.store(true, Ordering::SeqCst);
    }
}
