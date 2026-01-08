use anyhow::{Context, Result};
use socket2::{Domain, Protocol, Socket, Type};
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

use super::{MULTICAST_ADDR, MULTICAST_PORT};
use crate::audio::AudioFrame;
use crate::pipeline::node::{jitter_buffer, JitterBufferConsumer, JitterBufferProducer};
use crate::pipeline::Source;
use crate::state::{AppState, ConnectionStatus, HostId, HostInfo};

const HOST_TIMEOUT: Duration = Duration::from_secs(5);
const JITTER_BUFFER_CAPACITY: usize = 16;

struct HostPipeline {
    producer: JitterBufferProducer,
    consumer: JitterBufferConsumer,
    last_seen: Instant,
}

impl HostPipeline {
    fn new() -> Self {
        let (producer, consumer) = jitter_buffer(JITTER_BUFFER_CAPACITY);
        Self {
            producer,
            consumer,
            last_seen: Instant::now(),
        }
    }
}

pub struct HostPipelineManager {
    pipelines: HashMap<HostId, HostPipeline>,
}

impl HostPipelineManager {
    pub fn new() -> Self {
        Self {
            pipelines: HashMap::new(),
        }
    }

    pub fn push_frame(&mut self, host_id: HostId, frame: AudioFrame) {
        use crate::pipeline::Sink;

        let pipeline = self.pipelines.entry(host_id).or_insert_with(|| {
            info!("Creating pipeline for new host: {}", host_id.to_string());
            HostPipeline::new()
        });
        pipeline.last_seen = Instant::now();
        pipeline.producer.push(frame);
    }

    pub fn pull_and_mix(&mut self) -> Option<AudioFrame> {
        let mut mixed_samples: Option<Vec<i16>> = None;
        let mut result_seq = 0u64;
        let mut result_timestamp = 0u64;

        for pipeline in self.pipelines.values_mut() {
            if let Some(frame) = pipeline.consumer.pull() {
                result_seq = result_seq.max(frame.sequence_number);
                result_timestamp = result_timestamp.max(frame.timestamp);

                match &mut mixed_samples {
                    None => {
                        mixed_samples = Some(frame.samples.data().to_vec());
                    }
                    Some(mixed) => {
                        for (i, sample) in frame.samples.data().iter().enumerate() {
                            if i < mixed.len() {
                                mixed[i] = mixed[i].saturating_add(*sample);
                            }
                        }
                    }
                }
            }
        }

        mixed_samples.and_then(|samples| AudioFrame::new(result_seq, samples).ok())
    }

    pub fn cleanup_stale_hosts(&mut self) {
        let now = Instant::now();
        self.pipelines.retain(|host_id, pipeline| {
            let alive = now.duration_since(pipeline.last_seen) < HOST_TIMEOUT;
            if !alive {
                info!("Removing stale host pipeline: {}", host_id.to_string());
            }
            alive
        });
    }

    pub fn host_count(&self) -> usize {
        self.pipelines.len()
    }
}

impl Default for HostPipelineManager {
    fn default() -> Self {
        Self::new()
    }
}

pub struct NetworkReceiver {
    socket: std::net::UdpSocket,
    state: Arc<AppState>,
    pipeline_manager: Arc<Mutex<HostPipelineManager>>,
}

impl NetworkReceiver {
    pub fn new(
        state: Arc<AppState>,
        pipeline_manager: Arc<Mutex<HostPipelineManager>>,
    ) -> Result<Self> {
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))
            .context("Failed to create socket")?;

        socket
            .set_reuse_address(true)
            .context("Failed to set reuse address")?;

        let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), MULTICAST_PORT);
        socket
            .bind(&bind_addr.into())
            .context("Failed to bind socket")?;

        let multicast_ip: Ipv4Addr = MULTICAST_ADDR
            .parse()
            .context("Invalid multicast address")?;

        socket
            .join_multicast_v4(&multicast_ip, &Ipv4Addr::UNSPECIFIED)
            .context("Failed to join multicast group")?;

        info!(
            "Network receiver joined multicast group {}:{}",
            MULTICAST_ADDR, MULTICAST_PORT
        );

        Ok(Self {
            socket: socket.into(),
            state,
            pipeline_manager,
        })
    }

    pub fn run(mut self) {
        info!("Network receive thread started");

        *self.state.connection_status.lock().unwrap() = ConnectionStatus::Connected;

        let mut buf = [0u8; 65536];
        let mut last_cleanup = Instant::now();

        loop {
            if let Err(e) = self.handle_packet(&mut buf) {
                warn!("Error processing packet: {:?}", e);
            }

            if last_cleanup.elapsed() > Duration::from_secs(1) {
                self.pipeline_manager.lock().unwrap().cleanup_stale_hosts();
                last_cleanup = Instant::now();
            }
        }
    }

    fn handle_packet(&mut self, buf: &mut [u8]) -> Result<()> {
        let (size, source_addr) = self
            .socket
            .recv_from(buf)
            .context("Failed to receive UDP packet")?;

        let received_data = &buf[..size];
        let host_id = HostId::from(source_addr);

        let frame = AudioFrame::deserialize(received_data)
            .context(format!("Failed to deserialize frame from {}", source_addr))?;

        if !frame.validate() {
            anyhow::bail!("Invalid frame from {}", source_addr);
        }

        {
            let local_host_id = self.state.local_host_id.lock().unwrap();
            if let Some(local_id) = *local_host_id {
                if host_id == local_id {
                    return Ok(());
                }
            }
        }

        debug!(
            "Receive packet from {:?}, seq num {}, is_v4: {}",
            source_addr,
            frame.sequence_number,
            source_addr.is_ipv4()
        );

        {
            let mut hosts = self.state.active_hosts.lock().unwrap();
            hosts
                .entry(host_id)
                .and_modify(|info| {
                    info.last_seen = Instant::now();
                })
                .or_insert_with(|| {
                    info!("New host detected: {}", host_id.to_string());
                    HostInfo::new(host_id)
                });
        }

        self.pipeline_manager
            .lock()
            .unwrap()
            .push_frame(host_id, frame);

        Ok(())
    }
}

pub struct NetworkSource {
    pipeline_manager: Arc<Mutex<HostPipelineManager>>,
}

impl NetworkSource {
    pub fn new(pipeline_manager: Arc<Mutex<HostPipelineManager>>) -> Self {
        Self { pipeline_manager }
    }
}

impl Source for NetworkSource {
    type Output = AudioFrame;

    fn pull(&mut self) -> Option<Self::Output> {
        self.pipeline_manager.lock().unwrap().pull_and_mix()
    }
}
