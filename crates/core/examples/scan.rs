//! Benchmark the engine from the command line:
//!
//!     cargo run --release -p disklens-core --example scan -- C:\ [treemap.png]
//!
//! Prints scan timing and totals, the top file types and largest files, and
//! optionally renders a 1920x1080 treemap to a PNG.

use std::time::Instant;

use disklens_core::scan::{ScanControl, ScanOptions, scan};
use disklens_core::treemap::{Layout, Options, render};
use disklens_core::{SizeMode, ext, palette, top};

fn main() {
    let mut args = std::env::args().skip(1);
    let root = args.next().unwrap_or_else(|| "C:\\".to_string());
    let png = args.next();

    let control = ScanControl::default();
    let started = Instant::now();
    let tree = scan(&root, &ScanOptions::default(), &control).expect("scan failed");
    let scanned = started.elapsed();
    let r = tree.root();
    println!(
        "scanned {} in {:.2?}: {} files, {} dirs, {:.2} GiB on disk ({:.2} GiB logical), {} unreadable dirs",
        root,
        scanned,
        r.files,
        r.dirs,
        r.physical as f64 / (1u64 << 30) as f64,
        r.logical as f64 / (1u64 << 30) as f64,
        tree.errors
    );

    let started = Instant::now();
    let colors = if std::env::var("PALETTE").as_deref() == Ok("classic") { &palette::CLASSIC } else { &palette::FLUENT };
    let stats = ext::compute(&tree, SizeMode::Physical, colors);
    println!("file types in {:.2?}:", started.elapsed());
    for s in stats.stats.iter().take(8) {
        println!("  {:>10} {:>10.2} MiB {:>8} files", if s.ext.is_empty() { "(none)" } else { &s.ext }, s.bytes as f64 / 1048576.0, s.files);
    }

    let started = Instant::now();
    let largest = top::largest_files(&tree, 0, SizeMode::Physical, 5);
    println!("largest files in {:.2?}:", started.elapsed());
    for id in largest {
        println!("  {:>10.2} MiB {}", tree.node(id).physical as f64 / 1048576.0, tree.path(id));
    }

    let mut layout = Layout::default();
    let options = Options::default();
    for pass in 0..2 {
        let started = Instant::now();
        let pixels = render(&tree, 0, 1920, 1080, SizeMode::Physical, &options, &stats.colors, None, &mut layout);
        println!("treemap 1920x1080 render {} in {:.2?}", if pass == 0 { "(cold)" } else { "(warm)" }, started.elapsed());
        if pass == 1 {
            if let Some(path) = &png {
                let file = std::fs::File::create(path).expect("create png");
                let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), 1920, 1080);
                encoder.set_color(png::ColorType::Rgba);
                encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
                println!("wrote {path}");
            }
        }
    }
}
