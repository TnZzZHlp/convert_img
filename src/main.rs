use clap::Parser;
use image_hasher::{HashAlg, Hasher, HasherConfig, ImageHash};
use img2avif::img2avif;
use indicatif::{ProgressBar, ProgressState, ProgressStyle};
use rayon::prelude::*;
use std::io::Write;
use std::sync::RwLock;
use std::{
    fmt,
    fs::{File, read_dir},
    path::Path,
    sync::OnceLock,
    time::Duration,
};
use uuid::Timestamp;

#[derive(Parser)]
struct Args {
    #[clap(short, long)]
    source_dir: String,

    #[clap(short, long, default_value = "./output")]
    output_dir: String,

    #[clap(short, long, default_value = "6")]
    speed: u8,

    #[clap(short, long, default_value = "85")]
    quality: u8,
}

static IMAGE_FORMATS: [&str; 3] = ["jpg", "png", "jpeg"];

static HASHER: OnceLock<Hasher> = OnceLock::new();
static HASHES: OnceLock<RwLock<Vec<ImageHash>>> = OnceLock::new();

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

    HASHES
        .set(init_hashes(&args.output_dir))
        .unwrap_or_else(|_| panic!("Failed to create hashes"));

    images.par_iter().for_each(|img_path| {
        let img_path = Path::new(img_path);

        // 找到相同的图片
        match compare_hash(img_path) {
            Ok(Some(hash)) => {
                // 转换图片格式
                let file = File::open(img_path).unwrap();
                let img = img2avif(file, Some(args.speed), Some(args.quality))
                    .expect("Failed to convert image");

                let output_path = output_dir.join(format!(
                    "{}.avif",
                    uuid::Uuid::new_v7(Timestamp::now(uuid::NoContext))
                ));
                std::fs::write(output_path, img).unwrap();

                // 保存哈希值
                let hash_file_path = output_dir.join("hashes");
                let mut file = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(hash_file_path)
                    .unwrap();
                writeln!(file, "{}", hash.to_base64()).unwrap();
                HASHES.get().unwrap().write().unwrap().push(hash);
                pb.inc(1);
            }
            Err(e) => {
                pb.println(format!("Image {} error: {:?}", img_path.display(), e));
                pb.inc(1);
            }
            _ => {
                pb.inc(1);
            }
        }
    });

    pb.finish_with_message("Processing complete");
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

// 获取目标文件夹hashes文件内保存的哈希值，然后与传入的Hash值进行对比
fn compare_hash<P: AsRef<Path>>(
    img_path: P,
) -> Result<Option<ImageHash>, image::error::ImageError> {
    let img = image::ImageReader::open(img_path)?
        .with_guessed_format()?
        .decode()?;
    let hasher = HASHER.get().unwrap();
    let origin_hash = hasher.hash_image(&img);

    // 从文件读取哈希值
    let hashes = HASHES.get().unwrap().read().unwrap();

    for hash in hashes.iter() {
        if hash.dist(&origin_hash) < 10 {
            return Ok(None);
        }
    }

    Ok(Some(origin_hash))
}

// 初始化HASHES
fn init_hashes(output_dir: &str) -> RwLock<Vec<ImageHash>> {
    let hashes = RwLock::new(Vec::new());
    let hash_file_path = Path::new(output_dir).join("hashes");
    if hash_file_path.exists() {
        let file = std::fs::read_to_string(hash_file_path).unwrap();
        for line in file.lines() {
            if let Ok(hash) = ImageHash::from_base64(line) {
                hashes.write().unwrap().push(hash);
            }
        }
    }
    hashes
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
