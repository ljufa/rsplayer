use symphonia::core::audio::RawSample;
use symphonia::core::conv::FromSample;
use symphonia::core::sample::SampleFormat as SymphoniaSampleFormat;

// DSD Wrapper types
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Default)]
pub struct DsdU32(pub u32);

impl cpal::Sample for DsdU32 {
    type Float = f32;
    type Signed = i32;
    const EQUILIBRIUM: Self = DsdU32(0x69696969);
}

impl cpal::SizedSample for DsdU32 {
    const FORMAT: cpal::SampleFormat = cpal::SampleFormat::DsdU32;
}

// Symphonia Sample trait implementation
impl std::ops::Add for DsdU32 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        DsdU32(self.0.wrapping_add(rhs.0))
    }
}

impl std::ops::Sub for DsdU32 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        DsdU32(self.0.wrapping_sub(rhs.0))
    }
}

impl symphonia::core::sample::Sample for DsdU32 {
    const FORMAT: SymphoniaSampleFormat = SymphoniaSampleFormat::U32; // Proxy
    const EFF_BITS: u32 = 32;
    const MID: Self = DsdU32(0x69696969);

    fn clamped(self) -> Self {
        self
    }
}

impl RawSample for DsdU32 {
    type RawType = u32;
    fn into_raw_sample(self) -> Self::RawType {
        self.0
    }
}

// Implement FromSample for primitives required by ConvertibleSample
// For types that are not compatible or where conversion is complex (PCM->DSD), we return silence.
macro_rules! impl_from_sample_for_dsd_dummy {
        ($($t:ty),*) => {
            $(
                impl FromSample<$t> for DsdU32 {
                    fn from_sample(_s: $t) -> Self {
                        DsdU32(0x69696969)
                    }
                }
            )*
        };
    }

impl_from_sample_for_dsd_dummy!(i8, i16, i24, u8, u16, u24, f32, f64);

// For u32 and i32, we assume they hold packed DSD data and pass it through.
impl FromSample<u32> for DsdU32 {
    fn from_sample(s: u32) -> Self {
        DsdU32(s)
    }
}

impl FromSample<i32> for DsdU32 {
    fn from_sample(s: i32) -> Self {
        DsdU32(s as u32)
    }
}

use symphonia::core::conv::IntoSample;
use symphonia::core::sample::{i24, u24};

// ConvertibleSample is automatically implemented because DsdU32 implements Sample and all FromSample variants.

// Explicit IntoSample<f32> for DsdU32 required by AudioOutputSample trait bound
impl IntoSample<f32> for DsdU32 {
    fn into_sample(self) -> f32 {
        let ones = self.0.count_ones();
        let centered = (ones as i32) - 16;
        centered as f32 / 16.0
    }
}

// Implement cpal::FromSample for DsdU32 relationships required by cpal::Sample
// DsdU32::Float is f32. DsdU32::Signed is i32.
// Required: f32 <-> DsdU32, i32 <-> DsdU32

impl cpal::FromSample<f32> for DsdU32 {
    fn from_sample_(_s: f32) -> Self {
        DsdU32(0x69696969)
    }
}

impl cpal::FromSample<DsdU32> for f32 {
    fn from_sample_(_s: DsdU32) -> Self {
        0.0
    }
}

impl cpal::FromSample<i32> for DsdU32 {
    fn from_sample_(_s: i32) -> Self {
        DsdU32(0x69696969)
    }
}

impl cpal::FromSample<DsdU32> for i32 {
    fn from_sample_(_s: DsdU32) -> Self {
        0
    }
}
