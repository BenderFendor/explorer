//! The CPU rasterizer from `anyrender_vello_cpu`, with a chosen worker count.
//! Upstream always starts one worker per core (up to 8), which costs memory and CPU.

use anyrender::ImageRenderer;
use anyrender_vello_cpu::{SoftbufferWindowRenderer, VelloCpuScenePainter};
use std::{
    sync::atomic::{AtomicU16, Ordering},
    time::{Duration, Instant},
};
use vello_cpu::{RenderContext, RenderMode, RenderSettings};

/// Workers used during animation; 0 always rasterizes on the calling thread.
static THREADS: AtomicU16 = AtomicU16::new(DEFAULT_THREADS);
const DEFAULT_THREADS: u16 = 4;

pub type CpuWindowRenderer = SoftbufferWindowRenderer<CpuRaster>;

pub fn configure_from_env() -> Result<(), String> {
    if let Ok(value) = std::env::var("EXPLORER_RENDER_THREADS") {
        let threads = value
            .parse()
            .map_err(|_| format!("EXPLORER_RENDER_THREADS must be a number, not {value:?}"))?;
        THREADS.store(threads, Ordering::Relaxed);
    }
    Ok(())
}

pub fn threads() -> u16 {
    THREADS.load(Ordering::Relaxed)
}

fn context(width: u32, height: u32, threads: u16) -> RenderContext {
    let settings = RenderSettings {
        num_threads: threads,
        render_mode: RenderMode::OptimizeSpeed,
        ..RenderSettings::default()
    };
    RenderContext::new_with(width as u16, height as u16, settings)
}

/// A frame this soon after the previous one means something is animating.
const BURST: Duration = Duration::from_millis(60);

/// Single repaints (hover, a new folder) rasterize on the calling thread, which keeps
/// memory low. Back-to-back frames switch to a worker pool for smooth motion; the pool
/// and its buffers are freed on the next single repaint.
pub struct CpuRaster {
    light: VelloCpuScenePainter,
    heavy: Option<VelloCpuScenePainter>,
    size: (u32, u32),
    last: Option<Instant>,
}

impl CpuRaster {
    fn painter(&mut self) -> &mut VelloCpuScenePainter {
        let now = Instant::now();
        let burst = self.last.is_some_and(|last| now - last < BURST);
        self.last = Some(now);
        let workers = threads();
        if !burst || workers == 0 {
            if self.heavy.take().is_some() {
                release_freed_memory();
            }
            return &mut self.light;
        }
        let (width, height) = self.size;
        self.heavy
            .get_or_insert_with(|| VelloCpuScenePainter(context(width, height, workers)))
    }
}

/// glibc keeps freed pages for reuse; return the pool's buffers to the system.
fn release_freed_memory() {
    // SAFETY: malloc_trim only releases unused heap pages; it has no preconditions.
    unsafe {
        libc::malloc_trim(0);
    }
}

impl ImageRenderer for CpuRaster {
    type ScenePainter<'a> = VelloCpuScenePainter;

    fn new(width: u32, height: u32) -> Self {
        Self {
            light: VelloCpuScenePainter(context(width, height, 0)),
            heavy: None,
            size: (width, height),
            last: None,
        }
    }

    fn resize(&mut self, width: u32, height: u32) {
        *self = Self::new(width, height);
    }

    fn reset(&mut self) {
        self.light.0.reset();
        if let Some(painter) = &mut self.heavy {
            painter.0.reset();
        }
    }

    fn render<F: FnOnce(&mut Self::ScenePainter<'_>)>(&mut self, draw_fn: F, buffer: &mut [u8]) {
        let painter = self.painter();
        painter.0.reset();
        draw_fn(painter);
        painter.0.flush();
        let (width, height) = (painter.0.width(), painter.0.height());
        painter
            .0
            .render_to_buffer(buffer, width, height, RenderMode::OptimizeSpeed);
    }

    fn render_to_vec<F: FnOnce(&mut Self::ScenePainter<'_>)>(
        &mut self,
        draw_fn: F,
        buffer: &mut Vec<u8>,
    ) {
        let (width, height) = self.size;
        buffer.resize(width as usize * height as usize * 4, 0);
        self.render(draw_fn, buffer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presenting_frames_keeps_workers_until_the_next_idle_repaint() {
        let mut raster = CpuRaster::new(64, 64);
        let mut pixels = Vec::new();
        raster.render_to_vec(|_| {}, &mut pixels);
        raster.reset();
        raster.render_to_vec(|_| {}, &mut pixels);
        assert!(raster.heavy.is_some());
        raster.reset();
        assert!(
            raster.heavy.is_some(),
            "softbuffer resets after every presentation"
        );
        raster.last = Some(Instant::now() - BURST - Duration::from_millis(1));
        raster.render_to_vec(|_| {}, &mut pixels);
        assert!(
            raster.heavy.is_none(),
            "a lone repaint releases the workers"
        );
    }
}
