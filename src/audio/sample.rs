use std::fmt::Debug;

use num_traits::{Bounded, FromPrimitive, Num, ToPrimitive};
use rkyv::Archive;

pub trait AudioSample:
    Num
    + Copy
    + Send
    + Sync
    + PartialOrd
    + ToPrimitive
    + FromPrimitive
    + Bounded
    + Archive
    + Debug
    + 'static
{
    fn silence() -> Self;

    fn to_f64_normalized(self) -> f64;

    fn from_f64_normalized(value: f64) -> Self;

    fn to_i64_for_mix(self) -> i64;

    fn from_i64_mixed(value: i64, source_count: usize) -> Self;
}

const I64_SCALE: i64 = 1 << 24;

impl AudioSample for f32 {
    fn silence() -> Self {
        0.0
    }

    fn to_f64_normalized(self) -> f64 {
        self as f64
    }

    fn from_f64_normalized(value: f64) -> Self {
        value.clamp(-1.0, 1.0) as f32
    }

    fn to_i64_for_mix(self) -> i64 {
        (self.clamp(-1.0, 1.0) * I64_SCALE as f32) as i64
    }

    fn from_i64_mixed(value: i64, source_count: usize) -> Self {
        (value as f32 / (I64_SCALE as f32 * source_count as f32)).clamp(-1.0, 1.0)
    }
}

impl AudioSample for f64 {
    fn silence() -> Self {
        0.0
    }

    fn to_f64_normalized(self) -> f64 {
        self
    }

    fn from_f64_normalized(value: f64) -> Self {
        value.clamp(-1.0, 1.0)
    }

    fn to_i64_for_mix(self) -> i64 {
        (self.clamp(-1.0, 1.0) * I64_SCALE as f64) as i64
    }

    fn from_i64_mixed(value: i64, source_count: usize) -> Self {
        (value as f64 / (I64_SCALE as f64 * source_count as f64)).clamp(-1.0, 1.0)
    }
}

impl AudioSample for i16 {
    fn silence() -> Self {
        0
    }

    fn to_f64_normalized(self) -> f64 {
        self as f64 / i16::MAX as f64
    }

    fn from_f64_normalized(value: f64) -> Self {
        (value.clamp(-1.0, 1.0) * i16::MAX as f64) as i16
    }

    fn to_i64_for_mix(self) -> i64 {
        self as i64
    }

    fn from_i64_mixed(value: i64, source_count: usize) -> Self {
        (value / source_count as i64).clamp(i16::MIN as i64, i16::MAX as i64) as i16
    }
}

impl AudioSample for i32 {
    fn silence() -> Self {
        0
    }

    fn to_f64_normalized(self) -> f64 {
        self as f64 / i32::MAX as f64
    }

    fn from_f64_normalized(value: f64) -> Self {
        (value.clamp(-1.0, 1.0) * i32::MAX as f64) as i32
    }

    fn to_i64_for_mix(self) -> i64 {
        self as i64
    }

    fn from_i64_mixed(value: i64, source_count: usize) -> Self {
        (value / source_count as i64).clamp(i32::MIN as i64, i32::MAX as i64) as i32
    }
}

impl AudioSample for u8 {
    fn silence() -> Self {
        128
    }

    fn to_f64_normalized(self) -> f64 {
        (self as f64 - 128.0) / 128.0
    }

    fn from_f64_normalized(value: f64) -> Self {
        ((value.clamp(-1.0, 1.0) * 128.0) + 128.0) as u8
    }

    fn to_i64_for_mix(self) -> i64 {
        self as i64 - 128
    }

    fn from_i64_mixed(value: i64, source_count: usize) -> Self {
        ((value / source_count as i64) + 128).clamp(0, 255) as u8
    }
}

impl AudioSample for i8 {
    fn silence() -> Self {
        0
    }

    fn to_f64_normalized(self) -> f64 {
        self as f64 / i8::MAX as f64
    }

    fn from_f64_normalized(value: f64) -> Self {
        (value.clamp(-1.0, 1.0) * i8::MAX as f64) as i8
    }

    fn to_i64_for_mix(self) -> i64 {
        self as i64
    }

    fn from_i64_mixed(value: i64, source_count: usize) -> Self {
        (value / source_count as i64).clamp(i8::MIN as i64, i8::MAX as i64) as i8
    }
}

impl AudioSample for u16 {
    fn silence() -> Self {
        32768
    }

    fn to_f64_normalized(self) -> f64 {
        (self as f64 - 32768.0) / 32768.0
    }

    fn from_f64_normalized(value: f64) -> Self {
        ((value.clamp(-1.0, 1.0) * 32768.0) + 32768.0) as u16
    }

    fn to_i64_for_mix(self) -> i64 {
        self as i64 - 32768
    }

    fn from_i64_mixed(value: i64, source_count: usize) -> Self {
        ((value / source_count as i64) + 32768).clamp(0, 65535) as u16
    }
}
