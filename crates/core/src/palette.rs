//! Treemap colours.
//!
//! WinDirStat equalises every palette colour to the same brightness (0.6) and
//! later scales pixels by `brightness / 0.6`, so cushions of different colours
//! look equally bright. The palette, the equalisation and the overflow rule
//! below follow WinDirStat 2.5.0 (Controls/TreeMap.h).

pub type Rgb = [u8; 3];

/// Brightness every palette colour is equalised to.
pub const PALETTE_BRIGHTNESS: f64 = 0.6;

/// WinDirStat's 18 cushion colours, before equalisation.
pub const CLASSIC: [Rgb; 18] = [
    [0, 0, 255],     // Blue
    [255, 0, 0],     // Red
    [0, 255, 0],     // Green
    [255, 255, 0],   // Yellow
    [0, 255, 255],   // Cyan
    [255, 0, 255],   // Magenta
    [255, 170, 0],   // Orange
    [0, 85, 255],    // Dodger Blue
    [255, 0, 85],    // Hot Pink
    [85, 255, 0],    // Lime Green
    [170, 0, 255],   // Violet
    [0, 255, 85],    // Spring Green
    [255, 0, 170],   // Deep Pink
    [0, 170, 255],   // Sky Blue
    [255, 85, 0],    // Orange Red
    [0, 255, 170],   // Aquamarine
    [85, 0, 255],    // Indigo
    [255, 255, 255], // White (used for every extension past the 17th)
];

/// A softer palette in the spirit of Windows 11's accent colours. Each colour
/// already sits near brightness 0.6 (r+g+b ≈ 459), so equalisation keeps its
/// hue instead of pushing it toward cyan or pink. Neighbouring ranks
/// alternate hue families so the largest file types stay distinguishable.
pub const FLUENT: [Rgb; 18] = [
    [76, 160, 224],  // Blue
    [240, 100, 110], // Coral
    [118, 200, 140], // Green
    [250, 190, 20],  // Gold
    [180, 100, 200], // Orchid
    [40, 200, 210],  // Teal
    [250, 140, 70],  // Orange
    [230, 80, 150],  // Magenta
    [160, 210, 90],  // Lime
    [110, 120, 230], // Indigo
    [235, 130, 95],  // Rust
    [70, 215, 175],  // Mint
    [230, 90, 140],  // Ruby
    [100, 160, 200], // Steel
    [60, 200, 200],  // Sea
    [140, 120, 230], // Iris
    [200, 150, 200], // Lavender
    [255, 255, 255], // Neutral (every extension past the 17th)
];

/// Give `color` the brightness `brightness` (0..1), where brightness is
/// (r+g+b)/3/255. Port of CColorSpace::MakeBrightColor.
pub fn make_bright(color: Rgb, brightness: f64) -> Rgb {
    let mut r = color[0] as f64 / 255.0;
    let mut g = color[1] as f64 / 255.0;
    let mut b = color[2] as f64 / 255.0;
    let f = 3.0 * brightness / (r + g + b);
    r *= f;
    g *= f;
    b *= f;
    normalize((r * 255.0) as i32, (g * 255.0) as i32, (b * 255.0) as i32)
}

/// Push channel values above 255 into the other two channels, preserving the
/// sum. Port of CColorSpace::NormalizeColor.
pub fn normalize(mut r: i32, mut g: i32, mut b: i32) -> Rgb {
    if r > 255 {
        distribute_first(&mut r, &mut g, &mut b);
    } else if g > 255 {
        distribute_first(&mut g, &mut r, &mut b);
    } else if b > 255 {
        distribute_first(&mut b, &mut r, &mut g);
    }
    [r.clamp(0, 255) as u8, g.clamp(0, 255) as u8, b.clamp(0, 255) as u8]
}

fn distribute_first(first: &mut i32, second: &mut i32, third: &mut i32) {
    let h = (*first - 255) / 2;
    *first = 255;
    *second += h;
    *third += h;
    if *second > 255 {
        let j = *second - 255;
        *second = 255;
        *third += j;
    } else if *third > 255 {
        let j = *third - 255;
        *third = 255;
        *second += j;
    }
}

/// A palette equalised for cushion rendering.
pub fn equalized(base: &[Rgb]) -> Vec<Rgb> {
    base.iter().map(|&c| make_bright(c, PALETTE_BRIGHTNESS)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equalized_colors_share_brightness() {
        for c in equalized(&CLASSIC) {
            let sum = c[0] as u32 + c[1] as u32 + c[2] as u32;
            // 0.6 * 3 * 255 = 459, minus truncation of up to one per channel.
            assert!((456..=459).contains(&sum), "{c:?} sums to {sum}");
        }
    }

    #[test]
    fn pure_blue_overflows_into_other_channels() {
        // 0.6 brightness on pure blue needs ~459 in one channel; 3.0 * 0.6 is
        // just below 1.8 in f64, so it truncates to 458 and the overflow of
        // 203 splits into 101 for each other channel (as in WinDirStat).
        assert_eq!(make_bright([0, 0, 255], 0.6), [101, 101, 255]);
    }

    #[test]
    fn white_becomes_neutral_grey() {
        assert_eq!(make_bright([255, 255, 255], 0.6), [153, 153, 153]);
    }
}
