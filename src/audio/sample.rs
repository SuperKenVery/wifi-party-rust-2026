use num_traits::{One, PrimInt, Num, ToPrimitive};

/// Trait representing a single audio sample.
/// Defines properties like the silence value (center point).
pub trait AudioSample: Num + Copy + Send + Sync + PartialOrd + ToPrimitive {
    /// Returns the value representing silence for this sample type.
    fn silence() -> Self;
}

impl AudioSample for f32 {
    fn silence() -> Self {
        0.0
    }
}

impl AudioSample for f64 {
    fn silence() -> Self {
        0.0
    }
}

macro_rules! impl_audio_sample_int {
    ($($t:ty),*) => {
        $(
            impl AudioSample for $t {
                fn silence() -> Self {
                    (Self::min_value() + Self::max_value()) / (Self::one() + Self::one())
                }
            }
        )*
    }
}

impl_audio_sample_int!(u8, i8, u16, i16, u32, i32, u64, i64, usize, isize);
