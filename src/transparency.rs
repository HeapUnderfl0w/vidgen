//! This is a thought experiment of eventually adding
//! transparency support to vidgen. This would require
//! a different output format which in turn has different
//! options on ffmpeg, might be a bit of a pain to support.
//!
//! Additionally following libs would be needed
//!
//! - image
//!
//! Due to the above difficulties this step is skipped for now.


// -- This main function uses some test data on a 4x4 grid to visualize the transformation
// fn main() {
//     #[rustfmt::skip]
//     let mut testdata: [Rgba; 4 * 4] = [
//         Rgba { r: 185, g: 175, b: 143, a: 220 },
//         Rgba { r: 085, g: 131, b: 203, a: 161 },
//         Rgba { r: 180, g: 005, b: 065, a: 217 },
//         Rgba { r: 188, g: 131, b: 000, a: 097 },
//         Rgba { r: 249, g: 254, b: 098, a: 163 },
//         Rgba { r: 121, g: 055, b: 227, a: 181 },
//         Rgba { r: 074, g: 072, b: 246, a: 200 },
//         Rgba { r: 000, g: 088, b: 224, a: 216 },
//         Rgba { r: 102, g: 079, b: 072, a: 157 },
//         Rgba { r: 090, g: 069, b: 235, a: 213 },
//         Rgba { r: 234, g: 197, b: 182, a: 112 },
//         Rgba { r: 025, g: 167, b: 197, a: 088 },
//         Rgba { r: 195, g: 054, b: 119, a: 202 },
//         Rgba { r: 249, g: 162, b: 176, a: 086 },
//         Rgba { r: 179, g: 147, b: 208, a: 215 },
//         Rgba { r: 152, g: 089, b: 232, a: 209 }
//     ];

//     let old = testdata.clone();

//     process(&mut testdata);

//     for (idx, v) in testdata.iter().enumerate() {
//         println!("{: >3} | {: >3?} -> {: >3?}", idx, old[idx], v);
//     }
// }

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

pub type FLOAT = f64;
pub type RgbaF = (FLOAT, FLOAT, FLOAT, FLOAT);
pub type RgbF = (FLOAT, FLOAT, FLOAT);

impl Rgba {
    fn as_float(&self) -> RgbaF {
        (
            (self.r as FLOAT) / 255.0,
            (self.g as FLOAT) / 255.0,
            (self.b as FLOAT) / 255.0,
            (self.a as FLOAT) / 255.0,
        )
    }

    fn from_float((r, g, b, a): RgbaF) -> Self {
        Rgba {
            r: (r * 255.0).trunc() as u8,
            g: (g * 255.0).trunc() as u8,
            b: (b * 255.0).trunc() as u8,
            a: (a * 255.0).trunc() as u8,
        }
    }
}

#[inline]
fn bv<T>(b: bool, t: T, f: T) -> T {
    if b {
        t
    } else {
        f
    }
}

#[inline]
fn lerp(v: FLOAT, l: FLOAT, h: FLOAT) -> FLOAT {
    v * (1.0 - h) + l * h
}

pub fn process(image: &mut [Rgba]) {
    for idx in 0..image.len() {
        image[idx] = process_px(image[idx]);
    }
}

pub fn process_px(rpx: Rgba) -> Rgba {
    const BLUE_THRESH: FLOAT   = 0.4975;
    const BLUE_FACTOR: FLOAT   = 0.495;
    const BLOOM_ALPHA: FLOAT   = 1.0;
    const NOTE_ALPHA: FLOAT    = 1.0;
    const OPACITY_RANGE: FLOAT = 1.0;

    let (r, g, mut b, _) = rpx.as_float();

    let masked;
    if b > BLUE_THRESH {
        masked = true;
        b -= 0.5;
    } else {
        masked = false;
    }
    b *= BLUE_FACTOR;

    let pix_mul = {
        let tmpv = r.max(g).max(b);
        if tmpv != 0.0 {
            1.0 / tmpv
        } else {
            0.0
        }
    };

    let note_mask = bv(masked, 1.0, 0.0);
    let bloom_mask = ((((r.max(g).max(b) * BLOOM_ALPHA).clamp(0.0, 1.0)
        - note_mask * (1.0 - NOTE_ALPHA))
        - (1.0 - OPACITY_RANGE))
        * (1.0 / OPACITY_RANGE))
        .clamp(0.0, 1.0);

    let mixed_r = lerp(r * pix_mul, r, note_mask);
    let mixed_g = lerp(g * pix_mul, g, note_mask);
    let mixed_b = lerp(b * pix_mul, b, note_mask);

    Rgba::from_float((
        mixed_r,
        mixed_g,
        mixed_b,
        (note_mask * NOTE_ALPHA) + bloom_mask,
    ))
}
