//! Performance benchmarks for the map paginator: proxy preview, filters, FFT
//! grid removal, page preview and full-resolution PDF export, run through the
//! service exactly as the UI calls it. Budgets: docs/perf.md.
//!
//! cargo bench -p sleuth-service --bench perf [-- --strict] [-- --size 6000x4500]
//! `--strict` exits non-zero when a step is over budget.

use std::path::Path;
use std::time::{Duration, Instant};

use image::RgbImage;
use sleuth_service::{JobState, MapDoc, Service, SizingMode};

struct Row {
    name: String,
    median: Duration,
    budget: Duration,
}

fn median(mut v: Vec<Duration>) -> Duration {
    v.sort();
    v[v.len() / 2]
}

/// A parchment-like map with a dark grid every `cell` px and some texture,
/// so filters and the FFT have real work to do.
fn map_png(path: &Path, w: u32, h: u32, cell: u32) {
    let img = RgbImage::from_fn(w, h, |x, y| {
        if x % cell < 3 || y % cell < 3 {
            return image::Rgb([40, 35, 30]);
        }
        let n = ((x.wrapping_mul(73) ^ y.wrapping_mul(151)) % 23) as u8;
        let r = (x / 7 + y / 11) % 40;
        image::Rgb([200 - n - r as u8, 180 - n, 140 - n / 2])
    });
    img.save(path).unwrap();
}

async fn wait(svc: &Service, id: &str) -> Duration {
    let t = Instant::now();
    loop {
        let st = svc.job_status(id).unwrap();
        match st.state {
            JobState::Running => tokio::time::sleep(Duration::from_millis(5)).await,
            JobState::Done => return t.elapsed(),
            _ => panic!("job failed: {:?}", st.error),
        }
    }
}

async fn time<F, Fut, T>(n: usize, mut f: F) -> Duration
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = T>,
{
    let mut v = Vec::with_capacity(n);
    for _ in 0..n {
        let t = Instant::now();
        std::hint::black_box(f().await);
        v.push(t.elapsed());
    }
    median(v)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let strict = args.iter().any(|a| a == "--strict");
    let (w, h) = args
        .iter()
        .position(|a| a == "--size")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.split_once('x'))
        .map(|(a, b)| (a.parse().unwrap(), b.parse().unwrap()))
        .unwrap_or((6000u32, 4500u32));
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
    let rows = rt.block_on(run(w, h));

    println!("\nmap paginator, {w}×{h} px source ({:.1} MP)", (w * h) as f64 / 1e6);
    println!("{:<44} {:>10} {:>10}  ", "step", "median", "budget");
    let mut over = 0;
    for r in &rows {
        let ok = r.median <= r.budget;
        if !ok {
            over += 1;
        }
        println!(
            "{:<44} {:>8.0}ms {:>8.0}ms  {}",
            r.name,
            r.median.as_secs_f64() * 1e3,
            r.budget.as_secs_f64() * 1e3,
            if ok { "ok" } else { "OVER" }
        );
    }
    if strict && over > 0 {
        eprintln!("{over} step(s) over budget");
        std::process::exit(1);
    }
}

async fn run(w: u32, h: u32) -> Vec<Row> {
    let dir = tempfile::tempdir().unwrap();
    let svc = Service::open(dir.path().join("data")).unwrap();
    let src = dir.path().join("Battle map.png");
    map_png(&src, w, h, 100);
    let ms = Duration::from_millis;
    let mut rows = vec![];
    let mut row = |name: &str, median: Duration, budget: Duration| {
        rows.push(Row { name: name.into(), median, budget });
    };

    let t = Instant::now();
    let doc = svc.map_open_file(src.clone()).await.unwrap();
    row("open file (import + 2048 px proxy)", t.elapsed(), ms(4000));
    let doc = MapDoc { mode: SizingMode::CellSide, side: 100.0, ..doc };

    // the cached proxy as PNG: what every preview pays at least
    row("preview, no filters", time(5, || svc.map_preview(doc.clone())).await, ms(250));

    // filters: each preset on the proxy (the UI debounces 120 ms on top)
    let catalog = svc.map_filter_catalog();
    let mut worst = (Duration::ZERO, String::new());
    let mut all = vec![];
    for p in catalog.presets.iter().filter(|p| !p.pipeline.is_empty()) {
        let d = MapDoc { filters: p.pipeline.clone(), ..doc.clone() };
        let m = time(3, || svc.map_preview(d.clone())).await;
        all.push(m);
        if m > worst.0 {
            worst = (m, p.name.clone());
        }
    }
    row("preview, filter presets (median)", median(all), ms(400));
    row(&format!("preview, slowest preset: {}", worst.1), worst.0, ms(800));

    // FFT grid removal on the proxy: first run computes, then it is cached
    let dg = MapDoc { remove_grid: true, ..doc.clone() };
    let t = Instant::now();
    svc.map_preview(dg.clone()).await.unwrap();
    row("preview, grid removal (cold, FFT)", t.elapsed(), ms(2500));
    row("preview, grid removal (cached)", time(5, || svc.map_preview(dg.clone())).await, ms(250));

    row(
        "page preview (0.5 px/pt)",
        time(5, || svc.map_preview_page(doc.clone(), 0, 0.5)).await,
        ms(250),
    );

    // full resolution export (a background job)
    let layout = svc.map_layout(doc.clone()).await.unwrap();
    let out = dir.path().join("map.pdf");
    let mut v = vec![];
    for _ in 0..2 {
        v.push(wait(&svc, &svc.map_export_pdf(doc.clone(), out.clone())).await);
    }
    row(&format!("export PDF, {} pages", layout.tiles.len()), median(v), ms(20000));
    let mut v = vec![];
    for _ in 0..2 {
        v.push(
            wait(
                &svc,
                &svc.map_export_pdf(MapDoc { remove_grid: true, ..doc.clone() }, out.clone()),
            )
            .await,
        );
    }
    row("export PDF with full-res grid removal", median(v), ms(45000));
    rows
}
