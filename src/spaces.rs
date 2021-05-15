use image::GenericImage;
use image::RgbImage as Image;

type Rgb = image::Rgb<u8>;
type Tripple = (f32, f32, f32);


fn paste_from_fn<Encode: Fn(Tripple) -> Rgb>(
    out: &mut Image,
    left: u32,
    width: u32,
    buf: &Vec<Tripple>,
    encode: Encode,
) {
    let mut it = buf.iter();
    for y in 0..out.height() {
        for x in left..(left + width) {
            out.put_pixel(x, y, encode(*it.next().unwrap()));
        }
    }
}


pub trait Space: Sync {
    fn get_file_suffix(&self) -> &str;

    fn tripple_from_rgb(&self, rgb: Rgb) -> Tripple;
    fn rgb_from_fst(&self, value: f32) -> Rgb;
    fn rgb_from_snd(&self, value: f32) -> Rgb;
    fn rgb_from_trd(&self, value: f32) -> Rgb;

    fn decompose_image(&self, img: &Image) -> Vec<Tripple> {
        img.pixels()
            .map(|rgb: &Rgb| self.tripple_from_rgb(*rgb))
            .collect::<Vec<_>>()
    }

    fn build_image(&self, src: &Image) -> Image {
        let (width, height) = src.dimensions();
        let buf = self.decompose_image(src);
        let mut dst = Image::new(width * 4, height);
        dst.copy_from(&*src, 0, 0).unwrap();
        paste_from_fn(&mut dst, width, width, &buf, |colour: Tripple| {
            self.rgb_from_fst(colour.0)
        });
        paste_from_fn(&mut dst, width * 2, width, &buf, |colour: Tripple| {
            self.rgb_from_snd(colour.1)
        });
        paste_from_fn(&mut dst, width * 3, width, &buf, |colour: Tripple| {
            self.rgb_from_trd(colour.2)
        });
        dst
    }
}


struct RgbSpace;

impl Space for RgbSpace {
    fn get_file_suffix(&self) -> &str { "rgb" }

    fn tripple_from_rgb(&self, rgb: Rgb) -> Tripple {
        let [r, g, b] = rgb.0;
        (r as f32, g as f32, b as f32)
    }

    fn rgb_from_fst(&self, value: f32) -> Rgb { Rgb::from([value as u8, 0, 0]) }
    fn rgb_from_snd(&self, value: f32) -> Rgb { Rgb::from([0, value as u8, 0]) }
    fn rgb_from_trd(&self, value: f32) -> Rgb { Rgb::from([0, 0, value as u8]) }
}


struct LinearRgbSpace;

impl Space for LinearRgbSpace {
    fn get_file_suffix(&self) -> &str { "lin-rgb" }

    fn tripple_from_rgb(&self, rgb: Rgb) -> Tripple {
        let [r, g, b] = srgb::gamma::linear_from_u8(rgb.0);
        (r * 255.0 + 0.5, g * 255.0 + 0.5, b * 255.0 + 0.5)
    }

    fn rgb_from_fst(&self, value: f32) -> Rgb { Rgb::from([value as u8, 0, 0]) }
    fn rgb_from_snd(&self, value: f32) -> Rgb { Rgb::from([0, value as u8, 0]) }
    fn rgb_from_trd(&self, value: f32) -> Rgb { Rgb::from([0, 0, value as u8]) }
}


fn grey(value: f32) -> Rgb {
    let value = value as u8;
    Rgb::from([value, value, value])
}

struct XYZSpace;

impl Space for XYZSpace {
    fn get_file_suffix(&self) -> &str { "XYZ" }

    fn tripple_from_rgb(&self, rgb: Rgb) -> Tripple {
        let [x, y, z] = srgb::xyz_from_u8(rgb.0);
        (
            srgb::gamma::compress_u8(x / srgb::xyz::D65_XYZ[0]) as f32,
            srgb::gamma::compress_u8(y) as f32,
            srgb::gamma::compress_u8(z / srgb::xyz::D65_XYZ[1]) as f32,
        )
    }

    fn rgb_from_fst(&self, value: f32) -> Rgb { grey(value) }
    fn rgb_from_snd(&self, value: f32) -> Rgb { grey(value) }
    fn rgb_from_trd(&self, value: f32) -> Rgb { grey(value) }
}


struct XYYSpace;

fn rgb_from_xyy(lc_x: f32, lc_y: f32) -> Rgb {
    let x = lc_x * 0.5 / lc_y;
    let y = 0.5;
    let z = (1.0 - lc_x - lc_y) * 0.5 / lc_y;
    Rgb::from(srgb::u8_from_xyz([x, y, z]))
}

impl Space for XYYSpace {
    fn get_file_suffix(&self) -> &str { "xyY" }

    fn tripple_from_rgb(&self, rgb: Rgb) -> Tripple {
        let [x, y, z] = srgb::xyz_from_u8(rgb.0);
        let sum = x + y + z;
        (x / sum, y / sum, srgb::gamma::compress_u8(y) as f32)
    }

    fn rgb_from_fst(&self, value: f32) -> Rgb {
        rgb_from_xyy(value, srgb::xyz::D65_xyY[1])
    }
    fn rgb_from_snd(&self, value: f32) -> Rgb {
        rgb_from_xyy(srgb::xyz::D65_xyY[0], value)
    }
    fn rgb_from_trd(&self, value: f32) -> Rgb { grey(value) }
}


fn hs_common_from_rgb(rgb: Rgb) -> (f32, i32, i32, i32, i32) {
    let r = rgb.0[0] as i32;
    let g = rgb.0[1] as i32;
    let b = rgb.0[2] as i32;

    let min = std::cmp::min(std::cmp::min(r, g), b);
    let max = std::cmp::max(std::cmp::max(r, g), b);
    let sum = min + max;
    let range = max - min;

    let hue = if range == 0 {
        f32::NAN
    } else if max == r {
        ((g - b) as f32 / range as f32).rem_euclid(6.0)
    } else if max == g {
        (b - r) as f32 / range as f32 + 2.0
    } else {
        (r - g) as f32 / range as f32 + 4.0
    };

    (hue, min, max, sum, range)
}

fn hs_common_hue_to_rgb(hue: f32) -> Rgb {
    if hue != hue {
        Rgb::from([0, 0, 0])
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
        fn map(v: f32) -> u8 { (v * 255.0 + 64.25) as u8 }
        Rgb::from([map(r), map(g), map(b)])
    }
}


struct HslSpace;

impl Space for HslSpace {
    fn get_file_suffix(&self) -> &str { "hsl" }

    fn tripple_from_rgb(&self, rgb: Rgb) -> Tripple {
        let (hue, _min, _max, sum, range) = hs_common_from_rgb(rgb);

        let saturation = if range == 0 || range == 255 {
            0.0
        } else {
            range as f32 / (255 - (sum - 255).abs()) as f32
        };

        (hue, saturation, sum as f32 * 0.5)
    }

    fn rgb_from_fst(&self, value: f32) -> Rgb { hs_common_hue_to_rgb(value) }
    fn rgb_from_snd(&self, value: f32) -> Rgb {
        let v = (value * 255.0) as u8;
        Rgb::from([v, v, v])
    }
    fn rgb_from_trd(&self, value: f32) -> Rgb {
        let v = value as u8;
        Rgb::from([v, v, v])
    }
}


struct HsvSpace;

impl Space for HsvSpace {
    fn get_file_suffix(&self) -> &str { "hsv" }

    fn tripple_from_rgb(&self, rgb: Rgb) -> Tripple {
        let (hue, _min, max, _sum, range) = hs_common_from_rgb(rgb);

        let saturation = if max == 0 {
            0.0
        } else {
            range as f32 / max as f32
        };

        (hue, saturation, max as f32)
    }

    fn rgb_from_fst(&self, value: f32) -> Rgb { hs_common_hue_to_rgb(value) }
    fn rgb_from_snd(&self, value: f32) -> Rgb {
        let v = (value * 255.0) as u8;
        Rgb::from([v, v, v])
    }
    fn rgb_from_trd(&self, value: f32) -> Rgb {
        let v = value as u8;
        Rgb::from([v, v, v])
    }
}


struct HwbSpace;

impl Space for HwbSpace {
    fn get_file_suffix(&self) -> &str { "hwb" }

    fn tripple_from_rgb(&self, rgb: Rgb) -> Tripple {
        let (hue, min, max, _sum, _range) = hs_common_from_rgb(rgb);
        (hue, min as f32, (255 - max) as f32)
    }

    fn rgb_from_fst(&self, value: f32) -> Rgb { hs_common_hue_to_rgb(value) }
    fn rgb_from_snd(&self, value: f32) -> Rgb {
        let v = value as u8;
        Rgb::from([v, v, v])
    }
    fn rgb_from_trd(&self, value: f32) -> Rgb {
        let v = value as u8;
        Rgb::from([v, v, v])
    }
}


struct LabSpace;

impl Space for LabSpace {
    fn get_file_suffix(&self) -> &str { "lab" }

    fn tripple_from_rgb(&self, rgb: Rgb) -> Tripple {
        let lab = lab::Lab::from_rgb(&rgb.0);
        (lab.l, lab.a, lab.b)
    }

    fn rgb_from_fst(&self, value: f32) -> Rgb {
        Rgb::from(
            lab::Lab {
                l: value,
                a: 0.0,
                b: 0.0,
            }
            .to_rgb(),
        )
    }
    fn rgb_from_snd(&self, value: f32) -> Rgb {
        Rgb::from(
            lab::Lab {
                l: 30.0,
                a: value,
                b: 0.0,
            }
            .to_rgb(),
        )
    }
    fn rgb_from_trd(&self, value: f32) -> Rgb {
        Rgb::from(
            lab::Lab {
                l: 30.0,
                a: 0.0,
                b: value,
            }
            .to_rgb(),
        )
    }
}


struct LChabSpace;

impl Space for LChabSpace {
    fn get_file_suffix(&self) -> &str { "lchab" }

    fn tripple_from_rgb(&self, rgb: Rgb) -> Tripple {
        let lch = lab::LCh::from_rgb(&rgb.0);
        (lch.l, lch.c, lch.h)
    }

    fn rgb_from_fst(&self, value: f32) -> Rgb {
        Rgb::from(
            lab::LCh {
                l: value,
                c: 0.0,
                h: 0.0,
            }
            .to_rgb(),
        )
    }
    fn rgb_from_snd(&self, value: f32) -> Rgb {
        Rgb::from(
            lab::LCh {
                l: value / 1.338088,
                c: 0.0,
                h: 0.0,
            }
            .to_rgb(),
        )
    }
    fn rgb_from_trd(&self, value: f32) -> Rgb {
        Rgb::from(
            lab::LCh {
                l: 50.0,
                c: 133.8088 * 0.5,
                h: value,
            }
            .to_rgb(),
        )
    }
}


struct LuvSpace;

impl Space for LuvSpace {
    fn get_file_suffix(&self) -> &str { "luv" }

    fn tripple_from_rgb(&self, rgb: Rgb) -> Tripple {
        let luv = luv::Luv::from_rgb(&rgb.0);
        (luv.l, luv.u, luv.v)
    }

    fn rgb_from_fst(&self, value: f32) -> Rgb {
        Rgb::from(
            luv::Luv {
                l: value,
                u: 0.0,
                v: 0.0,
            }
            .to_rgb(),
        )
    }
    fn rgb_from_snd(&self, value: f32) -> Rgb {
        Rgb::from(
            luv::Luv {
                l: 30.0,
                u: value,
                v: 0.0,
            }
            .to_rgb(),
        )
    }
    fn rgb_from_trd(&self, value: f32) -> Rgb {
        Rgb::from(
            luv::Luv {
                l: 30.0,
                u: 0.0,
                v: value,
            }
            .to_rgb(),
        )
    }
}


struct LChuvSpace;

impl Space for LChuvSpace {
    fn get_file_suffix(&self) -> &str { "lchuv" }

    fn tripple_from_rgb(&self, rgb: Rgb) -> Tripple {
        let lch = luv::LCh::from_rgb(&rgb.0);
        (lch.l, lch.c, lch.h)
    }

    fn rgb_from_fst(&self, value: f32) -> Rgb {
        Rgb::from(
            luv::LCh {
                l: value,
                c: 0.0,
                h: 0.0,
            }
            .to_rgb(),
        )
    }
    fn rgb_from_snd(&self, value: f32) -> Rgb {
        Rgb::from(
            luv::LCh {
                l: value / 1.790383,
                c: 0.0,
                h: 0.0,
            }
            .to_rgb(),
        )
    }
    fn rgb_from_trd(&self, value: f32) -> Rgb {
        Rgb::from(
            luv::LCh {
                l: 50.0,
                c: 179.0383 * 0.5,
                h: value,
            }
            .to_rgb(),
        )
    }
}


pub static SPACES: [&dyn Space; 11] = [
    &RgbSpace,
    &LinearRgbSpace,
    &XYZSpace,
    &XYYSpace,
    &HslSpace,
    &HsvSpace,
    &HwbSpace,
    &LabSpace,
    &LChabSpace,
    &LuvSpace,
    &LChuvSpace,
];
