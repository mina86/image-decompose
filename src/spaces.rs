use image::RgbImage as Image;

type Rgb = [u8; 3];
type UnRgb = [std::mem::MaybeUninit<u8>; 3];


trait Pixel {
    fn set_rgb(&mut self, rgb: Rgb);
    fn set_grey(&mut self, value: u8) { self.set_rgb([value, value, value]); }
}

impl Pixel for UnRgb {
    fn set_rgb(&mut self, rgb: Rgb) {
        std::mem::MaybeUninit::write_slice(self, &rgb);
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
    pub fill_channels: fn(channels: &mut [&mut UnRgb], rgb: Rgb),
}


pub fn build_image(space: &Space, src_image: &Image) -> (u32, u32, Box<[u8]>) {
    const CHANNELS: usize = 3;

    let (width, height) = src_image.dimensions();
    let size = width as usize * (CHANNELS + 1) * height as usize * 3;
    let mut buffer = Box::<[u8]>::new_uninit_slice(size);
    let dst_rows = buffer
        .as_chunks_mut::<3>()
        .0
        .chunks_exact_mut(width as usize * (CHANNELS + 1));
    let src_rows = src_image
        .as_raw()
        .as_slice()
        .as_chunks::<3>()
        .0
        .chunks_exact(width as usize);

    for (src_row, dst_row) in src_rows.zip(dst_rows) {
        let (cpy_row, rest) = dst_row.split_at_mut(width as usize);
        // SAFETY: It’s safe to convert &[T; N] into &[MaybeUninit<T>; N].
        cpy_row.copy_from_slice(unsafe { std::mem::transmute(src_row) });

        let (fst_row, rest) = rest.split_at_mut(width as usize);
        let (snd_row, trd_row) = rest.split_at_mut(width as usize);
        for (src, fst, snd, trd) in
            itertools::izip!(src_row, fst_row, snd_row, trd_row)
        {
            (space.fill_channels)(&mut [fst, snd, trd], *src);
        }
    }

    let buffer = unsafe { buffer.assume_init() };
    (width * (CHANNELS as u32 + 1), height, buffer)
}


fn rgb_fill_channels(channels: &mut [&mut UnRgb], rgb: Rgb) {
    channels[0].set_rgb([rgb[0], 0, 0]);
    channels[1].set_rgb([0, rgb[1], 0]);
    channels[2].set_rgb([0, 0, rgb[2]]);
}

fn lin_rgb_fill_channels(channels: &mut [&mut UnRgb], rgb: Rgb) {
    let [r, g, b] = srgb::gamma::linear_from_u8(rgb);
    channels[0].set_rgb([round_u8(r) as u8, 0, 0]);
    channels[1].set_rgb([0, round_u8(g) as u8, 0]);
    channels[2].set_rgb([0, 0, round_u8(b) as u8]);
}

fn xyz_fill_channels(channels: &mut [&mut UnRgb], rgb: Rgb) {
    let [x, y, z] = srgb::xyz_from_u8(rgb);
    channels[0].set_grey(srgb::gamma::compress_u8(x / srgb::xyz::D65_XYZ[0]));
    channels[1].set_grey(srgb::gamma::compress_u8(y));
    channels[2].set_grey(srgb::gamma::compress_u8(z / srgb::xyz::D65_XYZ[1]));
}

fn xyy_fill_channels(channels: &mut [&mut UnRgb], rgb: Rgb) {
    let [x, y, z] = srgb::xyz_from_u8(rgb);
    let sum = x + y + z;

    fn rgb_from_xyy(lc_x: f32, lc_y: f32) -> Rgb {
        let x = lc_x * 0.5 / lc_y;
        let y = 0.5;
        let z = (1.0 - lc_x - lc_y) * 0.5 / lc_y;
        srgb::u8_from_xyz([x, y, z])
    }

    channels[0].set_rgb(rgb_from_xyy(x / sum, srgb::xyz::D65_xyY[1]));
    channels[1].set_rgb(rgb_from_xyy(srgb::xyz::D65_xyY[0], y / sum));
    channels[2].set_grey(srgb::gamma::compress_u8(y));
}


fn hs_common_from_rgb(rgb: Rgb) -> (f32, u8, u8, i32, i32) {
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

    (hue, min, max, sum, range)
}

fn hs_common_hue_to_rgb(hue: f32) -> Rgb {
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
    }
}

fn hsl_fill_channels(channels: &mut [&mut UnRgb], rgb: Rgb) {
    let (hue, _min, _max, sum, range) = hs_common_from_rgb(rgb);

    let saturation = if range == 0 || range == 255 {
        0.0
    } else {
        range as f32 / (255 - (sum - 255).abs()) as f32
    };

    channels[0].set_rgb(hs_common_hue_to_rgb(hue));
    channels[1].set_grey(round_u8(saturation) as u8);
    channels[2].set_grey((sum / 2) as u8);
}

fn hsv_fill_channels(channels: &mut [&mut UnRgb], rgb: Rgb) {
    let (hue, _min, max, _sum, range) = hs_common_from_rgb(rgb);

    let saturation = if max == 0 {
        0.0
    } else {
        range as f32 / max as f32
    };

    channels[0].set_rgb(hs_common_hue_to_rgb(hue));
    channels[1].set_grey(round_u8(saturation) as u8);
    channels[2].set_grey(max);
}

fn hwb_fill_channels(channels: &mut [&mut UnRgb], rgb: Rgb) {
    let (hue, min, max, _sum, _range) = hs_common_from_rgb(rgb);
    channels[0].set_rgb(hs_common_hue_to_rgb(hue));
    channels[1].set_grey(min);
    channels[2].set_grey(255 - max);
}



fn lab_fill_channels(channels: &mut [&mut UnRgb], rgb: Rgb) {
    let lab = lab::Lab::from_rgb(&rgb);
    channels[0].set_rgb(
        lab::Lab {
            l: lab.l,
            a: 0.0,
            b: 0.0,
        }
        .to_rgb(),
    );
    channels[1].set_rgb(
        lab::Lab {
            l: 30.0,
            a: lab.a,
            b: 0.0,
        }
        .to_rgb(),
    );
    channels[2].set_rgb(
        lab::Lab {
            l: 30.0,
            a: 0.0,
            b: lab.b,
        }
        .to_rgb(),
    );
}

fn lchab_fill_channels(channels: &mut [&mut UnRgb], rgb: Rgb) {
    let lch = lab::LCh::from_rgb(&rgb);
    channels[0].set_rgb(
        lab::LCh {
            l: lch.l,
            c: 0.0,
            h: 0.0,
        }
        .to_rgb(),
    );
    channels[1].set_rgb(
        lab::LCh {
            l: lch.c / 1.338088,
            c: 0.0,
            h: 0.0,
        }
        .to_rgb(),
    );
    channels[2].set_rgb(
        lab::LCh {
            l: 50.0,
            c: 133.8088 * 0.5,
            h: lch.h,
        }
        .to_rgb(),
    );
}



fn luv_fill_channels(channels: &mut [&mut UnRgb], rgb: Rgb) {
    let luv = luv::Luv::from_rgb(&rgb);
    channels[0].set_rgb(
        luv::Luv {
            l: luv.l,
            u: 0.0,
            v: 0.0,
        }
        .to_rgb(),
    );
    channels[1].set_rgb(
        luv::Luv {
            l: 30.0,
            u: luv.u,
            v: 0.0,
        }
        .to_rgb(),
    );
    channels[2].set_rgb(
        luv::Luv {
            l: 30.0,
            u: 0.0,
            v: luv.v,
        }
        .to_rgb(),
    );
}

fn lchuv_fill_channels(channels: &mut [&mut UnRgb], rgb: Rgb) {
    let lch = luv::LCh::from_rgb(&rgb);
    channels[0].set_rgb(
        luv::LCh {
            l: lch.l,
            c: 0.0,
            h: 0.0,
        }
        .to_rgb(),
    );
    channels[1].set_rgb(
        luv::LCh {
            l: lch.c / 1.790383,
            c: 0.0,
            h: 0.0,
        }
        .to_rgb(),
    );
    channels[2].set_rgb(
        luv::LCh {
            l: 50.0,
            c: 179.0383 * 0.5,
            h: lch.h,
        }
        .to_rgb(),
    );
}



#[rustfmt::skip]
pub static SPACES: [Space; 11] = [
    Space { name: "rgb",     fill_channels: rgb_fill_channels},
    Space { name: "lin-rgb", fill_channels: lin_rgb_fill_channels},
    Space { name: "XYZ",     fill_channels: xyz_fill_channels},
    Space { name: "xyY",     fill_channels: xyy_fill_channels},
    Space { name: "hsl",     fill_channels: hsl_fill_channels},
    Space { name: "hsv",     fill_channels: hsv_fill_channels},
    Space { name: "hwb",     fill_channels: hwb_fill_channels},
    Space { name: "lab",     fill_channels: lab_fill_channels},
    Space { name: "lchab",   fill_channels: lchab_fill_channels},
    Space { name: "luv",     fill_channels: luv_fill_channels},
    Space { name: "lchuv",   fill_channels: lchuv_fill_channels},
];
