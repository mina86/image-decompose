use std::io::BufRead;
use std::str::FromStr;

use clap::Clap;
use image::GenericImageView;


#[macro_export]
macro_rules! perr {
    ($path:expr, $fmt:literal, $($arg:tt)*) => {{
        let path: &::std::ffi::OsStr = $path.as_ref();
        crate::cli::perr_impl(path, std::format_args!($fmt, $($arg)*));
    }};
    ($path:expr, $msg:expr) => {
        crate::perr!($path, "{}", $msg);
    };
}

pub fn perr_impl(path: &std::ffi::OsStr, msg: std::fmt::Arguments) {
    fn inner(
        mut out: impl std::io::Write,
        path: &[u8],
        msg: std::fmt::Arguments,
    ) -> bool {
        out.write_all(path).is_ok() &&
            out.write_all(b": ").is_ok() &&
            out.write_fmt(msg).is_ok() &&
            out.write_all(b"\n").is_ok()
    }
    let path = std::os::unix::ffi::OsStrExt::as_bytes(path);
    inner(std::io::stderr().lock(), path, msg);
}


struct Quality(pub f32);

impl std::str::FromStr for Quality {
    type Err = std::string::String;

    fn from_str(q: &str) -> Result<Self, Self::Err> {
        if q.eq_ignore_ascii_case("lossless") {
            return Ok(Self(f32::INFINITY));
        }
        match f32::from_str(q) {
            Err(err) => Err(format!("expected number or ‘lossless’: {}", err)),
            Ok(q) if 0.0 <= q && q <= 100.0 => Ok(Self(q)),
            Ok(q) => Err(format!("expected number from 0 to 100; got {}", q)),
        }
    }
}


#[derive(PartialEq, Eq, Debug)]
pub struct Crop {
    width: u32,
    height: u32,
    is_west: bool,
    is_north: bool,
    x: u32,
    y: u32,
}

#[derive(PartialEq, Eq, Debug)]
pub struct Dimensions {
    width: u32,
    height: u32,
}

impl std::str::FromStr for Crop {
    type Err = &'static str;

    fn from_str(arg: &str) -> Result<Self, Self::Err> {
        parse_crop_str(arg.as_bytes()).ok_or("expected ‘<w>x<h>+<x>+<y>’")
    }
}

impl std::str::FromStr for Dimensions {
    type Err = &'static str;

    fn from_str(arg: &str) -> Result<Self, Self::Err> {
        if let Some((w, sep, h, rest)) = parse_number_pair(arg.as_bytes()) {
            if w > 0 && sep == b'x' && h > 0 && rest.is_empty() {
                return Ok(Dimensions {
                    width: w,
                    height: h,
                });
            }
        }
        Err("expected ‘<width>x<height>’")
    }
}

fn parse_number_pair(arg: &[u8]) -> Option<(u32, u8, u32, &[u8])> {
    let n = arg.iter().take_while(|&&d| b'0' <= d && d <= b'9').count();
    let (a, arg) = arg.split_at(n);
    let a = u32::from_str(unsafe { std::str::from_utf8_unchecked(a) }).ok()?;
    let (&sep, arg) = arg.split_first()?;
    if sep <= 32 || sep >= 127 {
        return None;
    }
    let n = arg.iter().take_while(|&&d| b'0' <= d && d <= b'9').count();
    let (b, arg) = arg.split_at(n);
    let b = u32::from_str(unsafe { std::str::from_utf8_unchecked(b) }).ok()?;
    Some((a, sep, b, arg))
}

#[test]
fn test_parse_number_pair() {
    fn ok(arg: &str, a: u32, ch: char, b: u32, rest: &str) {
        let got = parse_number_pair(arg.as_bytes()).map(|(a, ch, b, rest)| {
            (a, ch as char, b, std::str::from_utf8(rest).unwrap())
        });
        assert_eq!(Some((a, ch, b, rest)), got);
    }

    ok("10x20", 10, 'x', 20, "");
    ok("0x0", 0, 'x', 0, "");
    ok("10*20", 10, '*', 20, "");
    ok("010x020", 10, 'x', 20, "");
    ok("10x20+5", 10, 'x', 20, "+5");

    assert_eq!(None, parse_number_pair(b""));
    assert_eq!(None, parse_number_pair(b"10"));
    assert_eq!(None, parse_number_pair(b"10x"));
    assert_eq!(None, parse_number_pair(b"x20"));
}

#[test]
fn test_dimensions_from_str() {
    fn dim(width: u32, height: u32) -> Dimensions {
        Dimensions { width, height }
    }

    assert_eq!(Ok(dim(10, 20)), Dimensions::from_str("10x20"));
    assert_eq!(Ok(dim(10, 20)), Dimensions::from_str("010x020"));
    assert_eq!(None, Dimensions::from_str("").ok());
    assert_eq!(None, Dimensions::from_str("0x0").ok());
    assert_eq!(None, Dimensions::from_str("10X20").ok());
    assert_eq!(None, Dimensions::from_str("10X20+0+0").ok());
}

fn parse_crop_str(arg: &[u8]) -> Option<Crop> {
    let (width, sep, height, arg) = parse_number_pair(arg)?;
    if sep != b'x' || width == 0 || height == 0 {
        return None;
    }
    let (xch, arg) = match arg.split_first() {
        Some((&ch, _)) if ch != b'+' && ch != b'-' => return None,
        Some((&ch, rest)) => (ch, rest),
        None => (b'+', &b"0+0"[..]),
    };
    let (x, ych, y, arg) = parse_number_pair(arg)?;
    if (ych == b'+' || ych == b'-') && arg.is_empty() {
        Some(Crop {
            width,
            height,
            is_west: xch == b'+',
            is_north: ych == b'+',
            x,
            y,
        })
    } else {
        None
    }
}

#[test]
fn test_crop_from_str() {
    fn ok(want: &str, arg: &str) {
        let got = Crop::from_str(arg).map(|crop| {
            format!(
                "{}x{}{}{}{}{}",
                crop.width,
                crop.height,
                if crop.is_west { '+' } else { '-' },
                crop.x,
                if crop.is_north { '+' } else { '-' },
                crop.y
            )
        });
        assert_eq!(Ok(std::string::String::from(want)), got);
    }

    ok("10x20+0+0", "10x20");
    ok("10x20+0+0", "10x20+0+0");
    ok("10x20+30+40", "10x20+30+40");
    ok("10x20-30+40", "10x20-30+40");
    ok("10x20+30-40", "10x20+30-40");

    assert_eq!(None, Crop::from_str("").ok());
    assert_eq!(None, Crop::from_str("10X20").ok());
    assert_eq!(None, Crop::from_str("10x20+30*40").ok());
    assert_eq!(None, Crop::from_str("10x20++30+40").ok());
    assert_eq!(None, Crop::from_str("10x20+-30+40").ok());
}


pub struct SpaceArg(pub &'static super::spaces::Space);

impl std::str::FromStr for SpaceArg {
    type Err = std::string::String;

    fn from_str(arg: &str) -> Result<Self, Self::Err> {
        if let Some(space) = super::spaces::SPACES
            .iter()
            .find(|&space| arg.eq_ignore_ascii_case(space.name))
        {
            Ok(SpaceArg(space))
        } else {
            let spaces = super::spaces::SPACES
                .iter()
                .map(|space| space.name)
                .collect::<Vec<&'static str>>()
                .join(", ");
            Err(["supported colour spaces: ", &spaces].concat())
        }
    }
}


#[derive(Clap)]
#[clap(
    max_term_width = 80,
    setting = clap::AppSettings::ArgRequiredElseHelp,
    setting = clap::AppSettings::UnifiedHelpMessage,
    version = env!("CARGO_PKG_VERSION"),
    about = "Decomposes images into individual channels",
    help_template = r#"{about}
usage: {usage}

{all-args}

Loads specified image files and decomposes it into channels constructing a new
image with all the individual channels side-by-side."#)]
pub struct Opts {
    /// Directory to save output files in.  If not present, output files will be
    /// located in the same directory as the input.
    #[clap(short, long, parse(from_os_str))]
    pub out_dir: Option<std::path::PathBuf>,
    /// List of image files to process.
    #[clap(parse(from_os_str))]
    pub files: Vec<std::path::PathBuf>,

    /// Overwrite existing files without asking.  Overrides the `-i` flag.
    /// Without this or `-i` flag, output files which already exist will be
    /// skipped.
    #[clap(short, long, overrides_with = "interactive")]
    pub yes: bool,
    /// Ask before overwriting existing files.  Overrides the `-y` flag. Without
    /// this or `-y` flag, output files which already exist will be skipped.
    #[clap(short, long)]
    pub interactive: bool,

    /// Generate decomposition images for specified colours spaces.  If not
    /// provided, generate images for all supported colour spaces.  Supported
    /// spaces are RGB, lin-RGB (linear RGB w/o gamma correction), XYZ, xyY,
    /// HSL, HSV, HWB, Lab, LCHab, Luv and LCHuv.  Names are compared
    /// case-insensitively.
    #[clap(short, long, value_delimiter(","))]
    pub spaces: Vec<SpaceArg>,

    /// Save resulting WebP images with given quality.  Quality can be any
    /// number from 0 to 100 or ‘lossless’ to save as a lossless WebP.  The
    /// default quality is 90
    #[clap(short, long, default_value = "90")]
    quality: Quality,
    /// Alias of ‘--quality=lossless’.
    #[clap(long, overrides_with = "quality")]
    lossless: bool,

    /// Resize the source image to specified size.  The size is specified in
    /// ‘<width>x<height>` format.
    ///
    /// If specified together with `--crop`, resizing happens first.
    ///
    /// Note that if multiple images are specified, the resize operation will be
    /// applied to all of them.  To be able to resize different images to
    /// different sizes, the command needs to be called multiple times.
    #[clap(long)]
    resize: Option<Dimensions>,
    /// Crop the source image according to the specified geometry.  The geometry
    /// is in ‘<width>x<height>+<offset-x>+<offset-y>’ form.  The offset is
    /// optional and if it’s not specified it’s assumed to be ‘+0+0’.  Either
    /// coordinate of the offset can be negative to specify offset from the
    /// other side of the image (e.g. `320x200-50+10` selects a rectangle 50
    /// pixels from the right edge of the image and 10 pixels from the top).
    ///
    /// Note that if multiple images are specified, the cropping will be
    /// performed on all of them.  To be able to crop different images based on
    /// different specifications, the command needs to be called multiple times.
    #[clap(long)]
    crop: Option<Crop>,

    /// Run at most given number of threads in parallel.  By default, program
    /// will run one thread per logical CPU core.  Specifying zero or one
    /// effectively disables parallelism.
    #[clap(short, long)]
    pub jobs: Option<usize>,
}

impl Opts {
    pub fn encode(&self, enc: webp::Encoder) -> webp::WebPMemory {
        let q = self.quality.0;
        if self.lossless || q == f32::INFINITY {
            enc.encode_lossless()
        } else {
            enc.encode(q.clamp(0.0, 100.0))
        }
    }

    pub fn resize_image(
        &self,
        img: image::DynamicImage,
    ) -> image::DynamicImage {
        if let Some(Dimensions {
            width: w,
            height: h,
        }) = self.resize
        {
            img.resize_exact(w, h, image::imageops::Lanczos3)
        } else {
            img
        }
    }

    pub fn crop_image(&self, img: image::DynamicImage) -> image::DynamicImage {
        if let Some(crop) = &self.crop {
            let (img_width, img_height) = img.dimensions();
            let width = crop.width.min(img_width);
            let height = crop.height.min(img_height);
            if width == img_width && height == img_height {
                return img;
            }
            let x = crop.x.min(img_width - width);
            let y = crop.y.min(img_height - height);
            let x = if crop.is_west {
                x
            } else {
                img_width - width - x
            };
            let y = if crop.is_north {
                y
            } else {
                img_height - height - y
            };
            img.crop_imm(x, y, width, height)
        } else {
            img
        }
    }

    pub fn resize_and_crop_image(
        &self,
        i: image::DynamicImage,
    ) -> image::DynamicImage {
        self.crop_image(self.resize_image(i))
    }
}


pub enum Confirmer {
    Skip,
    Overwrite,
    Interactive(std::sync::Mutex<ConfirmerInner>),
}

#[allow(private_in_public)]
struct ConfirmerInner;

impl Confirmer {
    pub fn new(opts: &Opts) -> Self {
        if opts.yes {
            Self::Overwrite
        } else if opts.interactive {
            Self::Interactive(std::sync::Mutex::new(ConfirmerInner))
        } else {
            Self::Skip
        }
    }

    pub fn confirm(&self, file: &std::path::Path) -> bool {
        match self {
            Self::Overwrite => return true,
            _ if !file.exists() => return true,
            Self::Skip => (),
            Self::Interactive(mutex) => {
                let res = mutex
                    .lock()
                    .map_err(|p| p.into_inner())
                    .into_ok_or_err()
                    .confirm(file);
                match res {
                    Ok(ans) => return ans,
                    Err((a, b)) => eprintln!("{}: {}", a, b),
                }
            }
        }
        super::perr!(file, "file already exists, skipping");
        false
    }
}

fn write_prompt(
    mut out: impl std::io::Write,
    file: &std::path::Path,
) -> std::io::Result<()> {
    out.write_all(std::os::unix::ffi::OsStrExt::as_bytes(file.as_os_str()))?;
    write!(out, ": file exists, overwrite? [y/N] ")?;
    out.flush()
}

impl ConfirmerInner {
    fn confirm(
        &self,
        file: &std::path::Path,
    ) -> std::result::Result<bool, (&'static str, std::io::Error)> {
        let mut buf = Vec::<u8>::new();
        loop {
            if let Err(err) = write_prompt(std::io::stdout().lock(), file) {
                break Err(("stdout", err));
            }
            buf.clear();
            if let Err(err) =
                std::io::stdin().lock().read_until(b'\n', &mut buf)
            {
                break Err(("stdin", err));
            } else if buf.is_empty() {
                println!("N");
                break Ok(false);
            }
            while !buf.is_empty() &&
                (buf[buf.len() - 1] == b'\n' || buf[buf.len() - 1] == b'\r')
            {
                buf.pop();
            }
            if buf == b"y" || buf == b"Y" {
                break Ok(true);
            } else if buf.is_empty() || buf == b"n" || buf == b"N" {
                break Ok(false);
            }
        }
    }
}
