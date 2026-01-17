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
}

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
}
