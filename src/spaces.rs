use image::RgbImage as Image;

type Rgb = [u8; 3];
type UnRgb = [std::mem::MaybeUninit<u8>; 3];


struct Channels(*mut UnRgb, usize);

impl Channels {
    fn set_rgb(&mut self, channel: usize, rgb: Rgb) {
        // SAFETY: Caller guarantees that self.0 is a pointer to within a slice
        // such that all the indexes are valid.
        let pixel = unsafe { &mut *self.0.add(self.1 * channel) };
        std::mem::MaybeUninit::write_slice(pixel, &rgb);
    }
    fn set_grey(&mut self, channel: usize, value: u8) {
        self.set_rgb(channel, [value, value, value]);
    }
}


fn mul_add(multiplier: f32, multiplicand: f32, addend: f32) -> f32 {
    if cfg!(target_feature = "fma") {
        multiplier.mul_add(multiplicand, addend)
    } else {
        multiplier * multiplicand + addend
    }
}

fn round_u8(value: f32) -> u8 { mul_add(value, 255.0, 0.5) as u8 }


pub struct Space {
    pub name: &'static str,
    channels: usize,
    fill_channels: fn(channels: Channels, rgb: Rgb),
}


pub fn build_image(space: &Space, src_image: &Image) -> (u32, u32, Box<[u8]>) {
    let (width, height) = src_image.dimensions();
    let size = width as usize * (space.channels + 1) * height as usize * 3;
    let mut buffer = Box::<[u8]>::new_uninit_slice(size);
    let dst_rows = buffer
        .as_chunks_mut::<3>()
        .0
        .chunks_exact_mut(width as usize * (space.channels + 1));
    let src_rows = src_image
        .as_raw()
        .as_slice()
        .as_chunks::<3>()
        .0
        .chunks_exact(width as usize);

    for (src_row, dst_row) in src_rows.zip(dst_rows) {
        let (cpy_row, dst_row) = dst_row.split_at_mut(width as usize);
        // SAFETY: It’s safe to convert &[T; N] into &[MaybeUninit<T>; N].
        cpy_row.copy_from_slice(unsafe { std::mem::transmute(src_row) });

        for (dst, src) in dst_row.iter_mut().zip(src_row) {
            let channels = Channels(dst as *mut _, width as usize);
            (space.fill_channels)(channels, *src);
        }
    }

    // SAFETY: All data has been initialised.
    let buffer = unsafe { buffer.assume_init() };
    (width * (space.channels as u32 + 1), height, buffer)
}


fn rgb_fill_channels(mut channels: Channels, rgb: Rgb) {
    channels.set_rgb(0, [rgb[0], 0, 0]);
    channels.set_rgb(1, [0, rgb[1], 0]);
    channels.set_rgb(2, [0, 0, rgb[2]]);
}

fn lin_rgb_fill_channels(mut channels: Channels, rgb: Rgb) {
    let [r, g, b] = srgb::gamma::linear_from_u8(rgb);
    channels.set_rgb(0, [round_u8(r) as u8, 0, 0]);
    channels.set_rgb(1, [0, round_u8(g) as u8, 0]);
    channels.set_rgb(2, [0, 0, round_u8(b) as u8]);
}


fn xyz_fill_channels(mut channels: Channels, rgb: Rgb) {
    let [x, y, z] = srgb::xyz_from_u8(rgb);
    channels.set_grey(0, srgb::gamma::compress_u8(x / srgb::xyz::D65_XYZ[0]));
    channels.set_grey(1, srgb::gamma::compress_u8(y));
    channels.set_grey(2, srgb::gamma::compress_u8(z / srgb::xyz::D65_XYZ[1]));
}

fn xyy_fill_channels(mut channels: Channels, rgb: Rgb) {
    let [x, y, z] = srgb::xyz_from_u8(rgb);
    let sum = x + y + z;

    fn rgb_from_xyy(lc_x: f32, lc_y: f32) -> Rgb {
        let x = lc_x * 0.5 / lc_y;
        let y = 0.5;
        let z = (1.0 - lc_x - lc_y) * 0.5 / lc_y;
        srgb::u8_from_xyz([x, y, z])
    }

    channels.set_rgb(0, rgb_from_xyy(x / sum, srgb::xyz::D65_xyY[1]));
    channels.set_rgb(1, rgb_from_xyy(srgb::xyz::D65_xyY[0], y / sum));
    channels.set_grey(2, srgb::gamma::compress_u8(y));
}


fn hs_common_from_rgb(
    channels: &mut Channels,
    rgb: [u8; 3],
) -> (u8, u8, i32, i32) {
    let r = rgb[0];
    let g = rgb[1];
    let b = rgb[2];

    let min = std::cmp::min(std::cmp::min(r, g), b);
    let max = std::cmp::max(std::cmp::max(r, g), b);
    let sum = min as i32 + max as i32;
    let range = max as i32 - min as i32;

    let hue = if range == 0 {
        f32::NAN
    } else if max == r {
        ((g as i32 - b as i32) as f32 / range as f32).rem_euclid(6.0)
    } else if max == g {
        (b as i32 - r as i32) as f32 / range as f32 + 2.0
    } else {
        (r as i32 - g as i32) as f32 / range as f32 + 4.0
    };

    channels.set_rgb(
        0,
        if hue != hue {
            [0, 0, 0]
        } else {
            let x = 0.5 - 0.5 * (hue % 2.0 - 1.0).abs();
            let (r, g, b) = match hue as u8 {
                0 => (0.5, x, 0.0),
                1 => (x, 0.5, 0.0),
                2 => (0.0, 0.5, x),
                3 => (0.0, x, 0.5),
                4 => (x, 0.0, 0.5),
                5 => (0.5, 0.0, x),
                _ => unreachable!(),
            };
            fn map(v: f32) -> u8 { mul_add(v, 255.0, 64.25) as u8 }
            [map(r), map(g), map(b)]
        },
    );

    (min, max, sum, range)
}

fn hsl_fill_channels(mut channels: Channels, rgb: Rgb) {
    let (_min, _max, sum, range) = hs_common_from_rgb(&mut channels, rgb);

    let saturation = if range == 0 || range == 255 {
        0.0
    } else {
        range as f32 / (255 - (sum - 255).abs()) as f32
    };

    channels.set_grey(1, round_u8(saturation) as u8);
    channels.set_grey(2, (sum / 2) as u8);
}

fn hsv_fill_channels(mut channels: Channels, rgb: Rgb) {
    let (_min, max, _sum, range) = hs_common_from_rgb(&mut channels, rgb);

    let saturation = if max == 0 {
        0.0
    } else {
        range as f32 / max as f32
    };

    channels.set_grey(1, round_u8(saturation) as u8);
    channels.set_grey(2, max);
}

fn hwb_fill_channels(mut channels: Channels, rgb: Rgb) {
    let (min, max, _sum, _range) = hs_common_from_rgb(&mut channels, rgb);
    channels.set_grey(1, min);
    channels.set_grey(2, 255 - max);
}


fn lab_fill_channels(mut channels: Channels, rgb: Rgb) {
    fn set(channels: &mut Channels, channel: usize, l: f32, a: f32, b: f32) {
        channels.set_rgb(channel, lab::Lab { l, a, b }.to_rgb());
    }
    let lab = lab::Lab::from_rgb(&rgb);
    set(&mut channels, 0, lab.l, 0.0, 0.0);
    set(&mut channels, 1, 30.0, lab.a, 0.0);
    set(&mut channels, 2, 30.0, 0.0, lab.b);
}

fn lchab_fill_channels(mut channels: Channels, rgb: Rgb) {
    fn set(channels: &mut Channels, channel: usize, l: f32, c: f32, h: f32) {
        channels.set_rgb(channel, lab::LCh { l, c, h }.to_rgb());
    }
    let lch = lab::LCh::from_rgb(&rgb);
    set(&mut channels, 0, lch.l, 0.0, 0.0);
    set(&mut channels, 1, lch.c / 1.338088, 0.0, 0.0);
    set(&mut channels, 2, 50.0, 133.8088 * 0.5, lch.h);
}

fn luv_fill_channels(mut channels: Channels, rgb: Rgb) {
    fn set(channels: &mut Channels, channel: usize, l: f32, u: f32, v: f32) {
        channels.set_rgb(channel, luv::Luv { l, u, v }.to_rgb());
    }
    let luv = luv::Luv::from_rgb(&rgb);
    set(&mut channels, 0, luv.l, 0.0, 0.0);
    set(&mut channels, 1, 30.0, luv.u, 0.0);
    set(&mut channels, 2, 30.0, 0.0, luv.v);
}

fn lchuv_fill_channels(mut channels: Channels, rgb: Rgb) {
    fn set(channels: &mut Channels, channel: usize, l: f32, c: f32, h: f32) {
        channels.set_rgb(channel, luv::LCh { l, c, h }.to_rgb());
    }
    let lch = luv::LCh::from_rgb(&rgb);
    set(&mut channels, 0, lch.l, 0.0, 0.0);
    set(&mut channels, 1, lch.c / 1.790383, 0.0, 0.0);
    set(&mut channels, 2, 50.0, 179.0383 * 0.5, lch.h);
}


fn cmy_fill_channels(mut channels: Channels, rgb: Rgb) {
    let [r, g, b] = rgb;
    channels.set_rgb(0, [0, 255 - r, 255 - r]);
    channels.set_rgb(1, [255 - g, 0, 255 - g]);
    channels.set_rgb(2, [255 - b, 255 - b, 0]);
}

fn cmyk_fill_channels(mut channels: Channels, rgb: Rgb) {
    let [r, g, b] = rgb;
    let max = std::cmp::max(std::cmp::max(r, g), b);
    let c = round_u8(1.0 - r as f32 / max as f32);
    let m = round_u8(1.0 - g as f32 / max as f32);
    let y = round_u8(1.0 - b as f32 / max as f32);
    channels.set_rgb(0, [0, c, c]);
    channels.set_rgb(1, [m, 0, m]);
    channels.set_rgb(2, [y, y, 0]);
    channels.set_grey(3, 255 - max);
}


#[rustfmt::skip]
pub static SPACES: [Space; 13] = [
    Space { name: "rgb",     channels: 3, fill_channels: rgb_fill_channels},
    Space { name: "lin-rgb", channels: 3, fill_channels: lin_rgb_fill_channels},
    Space { name: "XYZ",     channels: 3, fill_channels: xyz_fill_channels},
    Space { name: "xyY",     channels: 3, fill_channels: xyy_fill_channels},
    Space { name: "hsl",     channels: 3, fill_channels: hsl_fill_channels},
    Space { name: "hsv",     channels: 3, fill_channels: hsv_fill_channels},
    Space { name: "hwb",     channels: 3, fill_channels: hwb_fill_channels},
    Space { name: "lab",     channels: 3, fill_channels: lab_fill_channels},
    Space { name: "lchab",   channels: 3, fill_channels: lchab_fill_channels},
    Space { name: "luv",     channels: 3, fill_channels: luv_fill_channels},
    Space { name: "lchuv",   channels: 3, fill_channels: lchuv_fill_channels},
    Space { name: "cmy",     channels: 3, fill_channels: cmy_fill_channels},
    Space { name: "cmyk",    channels: 4, fill_channels: cmyk_fill_channels},
];
