use std::{fmt, ops::RangeInclusive};

use anyhow::Context;

#[derive(Debug, Eq, PartialEq, Clone, Copy, Hash, Ord, PartialOrd, clap::ArgEnum)]
pub enum X264Preset {
    Ultrafast,
    Superfast,
    Veryfast,
    Faster,
    Fast,
    Medium,
    Slow,
    Slower,
    Veryslow,
}

impl fmt::Display for X264Preset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            X264Preset::Ultrafast => "ultrafast",
            X264Preset::Superfast => "superfast",
            X264Preset::Veryfast => "veryfast",
            X264Preset::Faster => "faster",
            X264Preset::Fast => "fast",
            X264Preset::Medium => "medium",
            X264Preset::Slow => "slow",
            X264Preset::Slower => "slower",
            X264Preset::Veryslow => "veryslow",
        };
        f.write_str(s)
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Copy, Hash, Ord, PartialOrd, clap::ArgEnum)]
pub enum X264Tune {
    Film,
    Animation,
    Grain,
    StillImage,
    FastDecode,
    ZeroLatency,
}

impl fmt::Display for X264Tune {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            X264Tune::Film => "film",
            X264Tune::Animation => "animation",
            X264Tune::Grain => "grain",
            X264Tune::StillImage => "stillimage",
            X264Tune::FastDecode => "fastdecode",
            X264Tune::ZeroLatency => "zerolatency",
        };
        f.write_str(s)
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Copy, Hash, Ord, PartialOrd)]
pub struct Crf(pub u8);

const VALID_RANGE: RangeInclusive<u8> = 0..=51;
impl Crf {
    pub fn parse(s: &str) -> anyhow::Result<Crf> {
        let p: u8 = s.parse().context("crf is not a number")?;
        if VALID_RANGE.contains(&p) {
            Ok(Crf(p))
        } else {
            anyhow::bail!(
                "crf out of range: valid range is {} to {}",
                VALID_RANGE.start(),
                VALID_RANGE.end()
            )
        }
    }
}
