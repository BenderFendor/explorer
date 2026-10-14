//! `--bench`: drives the app off-screen and reports renderer cost as JSON.
//! Nothing is shown on the desktop; input uses the same wheel channel as the window.

use crate::{screenshot::Offscreen, shell::Backend};
use anyrender::ImageRenderer;
use std::time::{Duration, Instant};

const FRAME: Duration = Duration::from_nanos(6_944_444);
const IDLE: Duration = Duration::from_secs(2);
/// Frames of scrolling: wheel down events, then the same number up.
const SCROLL_FRAMES: usize = 240;
const PRESS_EVERY: usize = 8;

pub fn run(backend: Backend) -> Result<(), String> {
    let runtime = tokio::runtime::Runtime::new().map_err(|error| error.to_string())?;
    let _guard = runtime.enter();
    let mut offscreen = Offscreen::new()?;
    let (width, height) = crate::screenshot::SIZE;
    let report = match backend {
        Backend::Cpu => measure(&mut offscreen, crate::raster::CpuRaster::new(width, height)),
        Backend::Gpu => measure(
            &mut offscreen,
            anyrender_vello::VelloImageRenderer::new(width, height),
        ),
    };
    let threads = crate::raster::threads();
    println!("{{\"renderer\":\"{backend:?}\",\"cpu_threads\":{threads},{report}}}");
    Ok(())
}

fn measure<R: ImageRenderer>(offscreen: &mut Offscreen, mut renderer: R) -> String {
    let mut pixels = Vec::new();
    offscreen.paint(&mut renderer, &mut pixels);
    let idle = idle_redraws(offscreen);
    let (first, settle) = wheel_latency(offscreen, &mut renderer, &mut pixels);
    let cpu_before = process_cpu();
    let started = Instant::now();
    let mut paint = Vec::with_capacity(SCROLL_FRAMES);
    let mut layout_total = 0.0;
    for frame in 0..SCROLL_FRAMES {
        if frame % PRESS_EVERY == 0 {
            crate::shell::publish(crate::shell::ShellEvent::Wheel {
                delta: if frame < SCROLL_FRAMES / 2 {
                    56.0
                } else {
                    -56.0
                },
                x: 600.0,
                y: 400.0,
                zoom: false,
            });
        }
        let begun = Instant::now();
        offscreen.pump();
        layout_total += begun.elapsed().as_secs_f64() * 1000.0;
        offscreen.paint(&mut renderer, &mut pixels);
        // Softbuffer resets after each presented frame. Exercise that same lifecycle.
        renderer.reset();
        let spent = begun.elapsed();
        paint.push(spent.as_secs_f64() * 1000.0);
        std::thread::sleep(FRAME.saturating_sub(spent));
    }
    let wall = started.elapsed().as_secs_f64();
    let rss_scrolling = rss_mb();
    // A lone repaint after motion stops, as a hover would cause.
    std::thread::sleep(Duration::from_millis(200));
    offscreen.pump();
    offscreen.paint(&mut renderer, &mut pixels);
    let cpu = 100.0 * (process_cpu() - cpu_before) / wall;
    paint.sort_by(f64::total_cmp);
    let mean = paint.iter().sum::<f64>() / paint.len() as f64;
    let p95 = paint[paint.len() * 95 / 100];
    format!(
        "\"wheel_first_ms\":{first:.2},\"wheel_settle_ms\":{settle:.2},\"frame_ms_mean\":{mean:.2},\"layout_ms_mean\":{:.2},\"paint_ms_mean\":{:.2},\"frame_ms_p95\":{p95:.2},\"scroll_cpu_pct\":{cpu:.1},\
         \"idle_redraws_per_s\":{idle:.1},\"rss_scrolling_mb\":{rss_scrolling},\"rss_after_mb\":{}",
        layout_total / SCROLL_FRAMES as f64,
        mean - layout_total / SCROLL_FRAMES as f64,
        rss_mb()
    )
}

/// Measures the first rendered movement and arrival of one wheel notch at 144 Hz.
fn wheel_latency<R: ImageRenderer>(
    offscreen: &mut Offscreen,
    renderer: &mut R,
    pixels: &mut Vec<u8>,
) -> (f64, f64) {
    let initial = offscreen.scroll();
    let started = Instant::now();
    crate::shell::publish(crate::shell::ShellEvent::Wheel {
        delta: 56.0,
        x: 600.0,
        y: 400.0,
        zoom: false,
    });
    let mut first = None;
    loop {
        offscreen.pump();
        offscreen.paint(renderer, pixels);
        renderer.reset();
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        if offscreen.scroll() > initial {
            first.get_or_insert(elapsed);
        }
        if offscreen.scroll() >= initial + 56.0 || elapsed > 2000.0 {
            return (
                first.unwrap_or(-1.0),
                if offscreen.scroll() >= initial + 56.0 {
                    elapsed
                } else {
                    -1.0
                },
            );
        }
        std::thread::sleep(FRAME);
    }
}

/// Frames the window would redraw per second while nothing happens.
fn idle_redraws(offscreen: &mut Offscreen) -> f64 {
    let started = Instant::now();
    let mut redraws = 0;
    while started.elapsed() < IDLE {
        offscreen.pump();
        if offscreen.document.is_animating() {
            redraws += 1;
        }
        std::thread::sleep(FRAME);
    }
    f64::from(redraws) / IDLE.as_secs_f64()
}

fn process_cpu() -> f64 {
    let stat = std::fs::read_to_string("/proc/self/stat").unwrap_or_default();
    // After the command name: state is index 0, utime 11, stime 12, in 100 Hz ticks.
    let fields: Vec<&str> = stat
        .rsplit_once(')')
        .map(|(_, rest)| rest.split_whitespace().collect())
        .unwrap_or_default();
    let ticks = |index: usize| {
        fields
            .get(index)
            .and_then(|field| field.parse::<f64>().ok())
    };
    ticks(11)
        .zip(ticks(12))
        .map_or(0.0, |(user, system)| (user + system) / 100.0)
}

fn rss_mb() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .unwrap_or_default()
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))
        .and_then(|value| value.split_whitespace().next()?.parse::<u64>().ok())
        .map_or(0, |kib| kib / 1024)
}
