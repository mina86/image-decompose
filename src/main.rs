#![feature(
    process_exitcode_placeholder,
    vec_into_raw_parts,
    maybe_uninit_write_slice
)]

use std::io::BufRead;
use std::io::Write;

mod cli;
mod spaces;


fn perr_impl(
    mut out: impl std::io::Write,
    path: &std::ffi::OsStr,
    msg: impl std::fmt::Display,
) {
    let path = std::os::unix::ffi::OsStrExt::as_bytes(path);
    if out.write_all(path).is_ok() {
        writeln!(out, ": {}", msg).ok();
    }
}

fn perr(path: impl AsRef<std::ffi::OsStr>, msg: impl std::fmt::Display) {
    perr_impl(std::io::stderr().lock(), path.as_ref(), msg);
}


fn load(path: &std::path::PathBuf) -> Option<image::RgbImage> {
    match image::io::Reader::open(path) {
        Err(e) => {
            perr(path, e);
            None
        }
        Ok(rd) => match rd.decode() {
            Err(e) => {
                perr(path, format_args!("error decoding: {}", e));
                None
            }
            Ok(img) => Some(img.into_rgb8()),
        },
    }
}


fn output_directory<'a>(
    out_dir: &'a Option<std::path::PathBuf>,
    src_file: &'a std::path::Path,
) -> std::io::Result<std::borrow::Cow<'a, std::path::Path>> {
    if let Some(dir) = out_dir {
        Ok(std::borrow::Cow::Borrowed(dir.as_path()))
    } else if let Some(parent) = src_file.parent() {
        Ok(std::borrow::Cow::Borrowed(parent))
    } else {
        std::env::current_dir().map(|cwd| std::borrow::Cow::Owned(cwd))
    }
}


fn output_file_name(
    space: &dyn spaces::Space,
    out_dir: &std::path::Path,
    file_stem: &std::ffi::OsStr,
) -> std::path::PathBuf {
    let bytes = std::os::unix::ffi::OsStrExt::as_bytes(file_stem);
    let mut buf = Vec::<u8>::with_capacity(bytes.len() + 16);
    buf.extend_from_slice(bytes);
    buf.push(b'-');
    buf.extend_from_slice(space.get_file_suffix());
    buf.extend_from_slice(b".webp");
    let file_name: std::ffi::OsString =
        std::os::unix::ffi::OsStringExt::from_vec(buf);
    out_dir.join(file_name)
}


fn write_prompt(
    mut out: impl std::io::Write,
    file: &std::path::Path,
) -> std::io::Result<()> {
    out.write_all(std::os::unix::ffi::OsStrExt::as_bytes(file.as_os_str()))?;
    write!(out, "file exists, overwrite? [y/N] ")?;
    out.flush()
}

fn confirm_overwrite(opts: &cli::Opts, file: &std::path::Path) -> bool {
    if opts.force || !file.exists() {
        return true;
    } else if !opts.interactive {
        perr(file, "file already exists, skipping");
        return false;
    }

    let mut buf = Vec::<u8>::new();
    loop {
        if let Err(err) = write_prompt(std::io::stdout().lock(), file) {
            eprintln!("stdout: {}", err);
            perr(file, "file already exists, skipping");
            return false;
        }
        buf.clear();
        if let Err(err) = std::io::stdin().lock().read_until(b'\n', &mut buf) {
            eprintln!("stdin: {}", err);
            break false;
        } else if buf.is_empty() {
            break false;
        }
        while !buf.is_empty() &&
            (buf[buf.len() - 1] == b'\n' || buf[buf.len() - 1] == b'\r')
        {
            buf.pop();
        }
        if buf == b"y" || buf == b"Y" {
            break true;
        } else if buf.is_empty() || buf == b"n" || buf == b"N" {
            break false;
        }
    }
}


fn process_file(opts: &cli::Opts, file: &std::path::PathBuf) -> bool {
    let out_dir = match output_directory(&opts.out_dir, file) {
        Ok(dir) => dir,
        Err(err) => {
            perr(
                file,
                format_args!("unable to determine parent directory: {}", err),
            );
            return false;
        }
    };
    let file_stem = match file.file_stem() {
        Some(name) => name,
        None => {
            perr(file, "unable to determine file stem");
            return false;
        }
    };
    let img = if let Some(img) = load(file) {
        opts.resize_and_crop_image(img)
    } else {
        return false;
    };
    let mut ok = true;
    for space in spaces::SPACES.iter().copied() {
        let out_file = output_file_name(space, out_dir.as_ref(), file_stem);
        if !confirm_overwrite(opts, &out_file) {
            continue;
        }
        eprintln!("Generating {}...", out_file.to_string_lossy());
        let img = space.build_image(&img);
        let enc = opts.encode(webp::Encoder::from_image(
            &image::DynamicImage::ImageRgb8(img),
        ));
        if let Err(err) = std::fs::File::create(&out_file)
            .and_then(|mut fd| fd.write_all(&enc))
        {
            perr(&out_file, err);
            ok = false;
        }
    }
    ok
}

fn main() -> std::process::ExitCode {
    let opts = <cli::Opts as clap::Clap>::parse();
    if let Some(dir) = &opts.out_dir {
        if let Err(err) = std::fs::create_dir_all(dir) {
            perr(dir, err);
            return std::process::ExitCode::FAILURE;
        }
    }
    let errors = opts
        .files
        .iter()
        .filter(|file| !process_file(&opts, file))
        .count();
    if errors == 0 {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
