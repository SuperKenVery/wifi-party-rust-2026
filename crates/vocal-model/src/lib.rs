use burn::tensor::{Tensor, TensorData};
use burn_store::ModuleSnapshot;
use burn_wgpu::{Wgpu, WgpuDevice};
use include_bytes_aligned::include_bytes_aligned;
use tracing::debug;

#[cfg(has_vocal_model)]
mod all_rt {
    include!(concat!(env!("OUT_DIR"), "/model/all_rt.rs"));
}

pub const HAS_MODEL: bool = cfg!(has_vocal_model);

pub struct RtDttModel {
    #[cfg(has_vocal_model)]
    model: all_rt::Model<Wgpu>,
    #[cfg(has_vocal_model)]
    device: WgpuDevice,
}

impl RtDttModel {
    pub fn new() -> Option<Self> {
        #[cfg(has_vocal_model)]
        {
            debug!("RtDttModel::new entry");
            let device = WgpuDevice::default();
            let aligned_bpk: &'static [u8] =
                include_bytes_aligned!(32, concat!(env!("OUT_DIR"), "/model/all_rt.bpk"));
            debug!("RtDttModel::new creating model struct");
            let mut model = all_rt::Model::<Wgpu>::new(&device);
            debug!("RtDttModel::new loading model weights");
            let mut store = burn_store::BurnpackStore::from_static(aligned_bpk);
            debug!("RtDttModel::new loading model");
            model
                .load_from(&mut store)
                .expect("Failed to load burnpack weights");
            debug!("RtDttModel::new done loading");
            Some(Self { model, device })
        }

        #[cfg(not(has_vocal_model))]
        {
            None
        }
    }

    pub fn forward(&self, input: Vec<f32>, shape: [usize; 4]) -> Vec<f32> {
        #[cfg(has_vocal_model)]
        {
            let input = Tensor::<Wgpu, 4>::from_data(TensorData::new(input, shape), &self.device);
            let output = self.model.forward(input);
            output.into_data().to_vec().expect("burn tensor to vec")
        }

        #[cfg(not(has_vocal_model))]
        {
            let _ = (input, shape);
            unreachable!("RtDttModel::forward called without a generated vocal model")
        }
    }
}
