use clap::Parser;
use image_hasher::{HashAlg, Hasher, HasherConfig};
use img2avif::img2avif;
use indicatif::{ProgressBar, ProgressState, ProgressStyle};
use rayon::prelude::*;
use std::{
    fmt,
    fs::{File, read_dir},
    path::Path,
    sync::OnceLock,
    time::Duration,
};

#[derive(Parser)]
struct Args {
    #[clap(short, long)]
    source_dir: String,

    #[clap(short, long, default_value = "./output")]
    output_dir: String,
}

static IMAGE_FORMATS: [&str; 3] = ["jpg", "png", "jpeg"];

static HASHER: OnceLock<Hasher> = OnceLock::new();

fn main() {
    let args = Args::parse();
    let images = find_all_img_recusive(&args.source_dir);

    let output_dir = Path::new(&args.output_dir);
    if !output_dir.exists() {
        std::fs::create_dir_all(output_dir).unwrap();
    }

    // 进度条
    let pb = init_pb(images.len());

    HASHER
        .set(
            HasherConfig::new()
                .hash_alg(HashAlg::Blockhash)
                .hash_size(64, 64)
                .to_hasher(),
        )
        .unwrap_or_else(|_| panic!("Failed to create hasher"));

    images.par_iter().for_each(|img_path| {
        let img_path = Path::new(img_path);

        // 找到相同的图片
        match compare_hash_with_dir(&img_path, &output_dir) {
            Ok(hash) => {
                // 转换图片格式
                let file = File::open(img_path).unwrap();
                let img = img2avif(file, Some(1), Some(70)).unwrap();

                let output_path = output_dir.join(format!("{}.avif", hash));
                std::fs::write(output_path, img).unwrap();
                pb.inc(1);
            }
            Err(path) => {
                pb.println(format!(
                    "Image {} already exists in output directory: {}",
                    img_path.display(),
                    path
                ));
            }
        }
    });
}

fn find_all_img_recusive<P: AsRef<Path>>(path: P) -> Vec<String> {
    let mut images = Vec::new();
    if let Ok(entries) = read_dir(path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                images.extend(find_all_img_recusive(path));
            } else if IMAGE_FORMATS
                .iter()
                .any(|&ext| path.extension().is_some_and(|e| e == ext))
            {
                if let Some(path_str) = path.to_str() {
                    images.push(path_str.to_string());
                }
            }
        }
    }
    images
}

// 获取目标文件夹内所有文件名，然后与传入的Hash值进行对比
// 如果相同则返回False
// 如果不同则返回True
fn compare_hash_with_dir<P: AsRef<Path>>(img_path: P, output_dir: P) -> Result<String, String> {
    let img = image::open(img_path).unwrap();
    let hasher = HASHER.get().unwrap();
    let origin_hash = hasher.hash_image(&img);

    if let Ok(entries) = read_dir(output_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(file_name) = path.file_name() {
                    let target_hash: image_hasher::ImageHash<_> =
                        image_hasher::ImageHash::from_base64(file_name.to_str().unwrap()).unwrap();

                    if origin_hash.dist(&target_hash) < 10 {
                        return Err(path.to_str().unwrap().to_string());
                    }
                }
            }
        }
    }
    Ok(origin_hash.to_base64())
}

fn init_pb(len: usize) -> ProgressBar {
    let pb = ProgressBar::new(len as u64);
    pb.set_style(
        ProgressStyle::with_template(
            "[{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} {msg}",
        )
        .unwrap()
        .with_key("eta", |state: &ProgressState, w: &mut dyn fmt::Write| {
            write!(w, "{:.1}s", state.eta().as_secs_f64()).unwrap()
        })
        .progress_chars("#>-"),
    );
    pb.enable_steady_tick(Duration::from_millis(100));
    pb
}
