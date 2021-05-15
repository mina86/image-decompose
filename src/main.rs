#![feature(
    maybe_uninit_write_slice,
    process_exitcode_placeholder,
    result_into_ok_or_err,
    slice_as_chunks,
    new_uninit,
    vec_into_raw_parts
)]

use std::io::Write;

use rayon::prelude::*;

#[macro_use]
mod cli;
mod spaces;


fn load(path: &std::path::PathBuf) -> Option<image::RgbImage> {
    match image::io::Reader::open(path) {
        Err(e) => {
            perr!(path, e);
            None
        }
        Ok(rd) => match rd.decode() {
            Err(e) => {
                perr!(path, "error decoding: {}", e);
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
    space: &spaces::Space,
    out_dir: &std::path::Path,
    file_stem: &std::ffi::OsStr,
) -> std::path::PathBuf {
    let bytes = std::os::unix::ffi::OsStrExt::as_bytes(file_stem);
    let suffix = space.name;
    let mut buf = Vec::<u8>::with_capacity(bytes.len() + suffix.len() + 6);
    buf.extend_from_slice(bytes);
    buf.push(b'-');
    buf.extend_from_slice(suffix.as_bytes());
    buf.extend_from_slice(b".webp");
    let file_name: std::ffi::OsString =
        std::os::unix::ffi::OsStringExt::from_vec(buf);
    out_dir.join(file_name)
}


fn process_file(
    opts: &cli::Opts,
    confirmer: &cli::Confirmer,
    file: &std::path::PathBuf,
) -> bool {
    let out_dir = match output_directory(&opts.out_dir, file) {
        Ok(dir) => dir,
        Err(err) => {
            perr!(file, "unable to determine parent directory: {}", err);
            return false;
        }
    };
    let file_stem = match file.file_stem() {
        Some(name) => name,
        None => {
            perr!(file, "unable to determine file stem");
            return false;
        }
    };
    eprintln!("Loading {}...", file.to_string_lossy());
    let img = if let Some(img) = load(file) {
        opts.resize_and_crop_image(img)
    } else {
        return false;
    };
    let errors = opts
        .spaces
        .par_iter()
        .filter(|space| {
            let out_file =
                output_file_name(space.0, out_dir.as_ref(), file_stem);
            if !confirmer.confirm(&out_file) {
                return true;
            }
            eprintln!("Generating {}...", out_file.to_string_lossy());
            let (width, height, img) = spaces::build_image(space.0, &img);
            let enc =
                opts.encode(webp::Encoder::from_rgb(&img[..], width, height));
            if let Err(err) = std::fs::File::create(&out_file)
                .and_then(|mut fd| fd.write_all(&enc))
            {
                perr!(out_file, err);
                false
            } else {
                true
            }
        })
        .count();
    errors == 0
}

fn main() -> std::process::ExitCode {
    let mut opts = <cli::Opts as clap::Clap>::parse();
    if let Some(dir) = &opts.out_dir {
        if let Err(err) = std::fs::create_dir_all(dir) {
            perr!(dir, err);
            return std::process::ExitCode::FAILURE;
        }
    }
    if opts.spaces.is_empty() {
        opts.spaces.extend(spaces::SPACES.iter().map(cli::SpaceArg));
    } else {
        opts.spaces
            .sort_by_key(|space| space.0 as *const _ as usize);
        opts.spaces.dedup_by_key(|space| space.0 as *const _);
    }
    let opts = opts;
    if let Some(num) = opts.jobs {
        rayon::ThreadPoolBuilder::new()
            .num_threads(num.max(1))
            .build_global()
            .err()
            .map(|err| eprintln!("{}", err));
    }
    let confirmer = cli::Confirmer::new(&opts);
    let errors = opts
        .files
        .par_iter()
        .filter(|file| !process_file(&opts, &confirmer, file))
        .count();
    if errors == 0 {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
