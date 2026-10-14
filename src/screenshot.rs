use anyrender::ImageRenderer;
use blitz_dom::{Document, net::Resource};
use blitz_net::{MpscCallback, Provider};
use blitz_traits::shell::{ColorScheme, Viewport};
use dioxus_native::{DioxusDocument, DocumentConfig, prelude::VirtualDom};
use std::{
    fs::OpenOptions,
    io::Write,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::mpsc::UnboundedReceiver;

#[cfg(test)]
mod regressions;

const WIDTH: u32 = 1120;
const HEIGHT: u32 = 740;

/// The app's document without a window, at the default window size.
pub struct Offscreen {
    pub document: DioxusDocument,
    resources: UnboundedReceiver<(usize, Resource)>,
    provider: Arc<Provider<Resource>>,
    started: Instant,
}

impl Offscreen {
    /// Needs an entered Tokio runtime.
    pub fn new() -> Result<Self, String> {
        let (resources, callback) = MpscCallback::new();
        let provider = Arc::new(Provider::new(Arc::new(callback)));
        let mut document = DioxusDocument::new(
            VirtualDom::new(crate::ui::app),
            DocumentConfig {
                viewport: Some(Viewport::new(WIDTH, HEIGHT, 1.0, ColorScheme::Light)),
                net_provider: Some(provider.clone()),
                ..Default::default()
            },
        );
        document.initial_build();
        let mut offscreen = Self {
            document,
            resources,
            provider,
            started: Instant::now(),
        };
        settle(
            &mut offscreen.document,
            &mut offscreen.resources,
            &offscreen.provider,
        )?;
        Ok(offscreen)
    }

    /// One frame of work: run Dioxus, load finished resources, restyle and lay out.
    pub fn pump(&mut self) {
        self.document.poll(None);
        while let Ok((_, resource)) = self.resources.try_recv() {
            self.document.load_resource(resource);
        }
        self.document.resolve(self.started.elapsed().as_secs_f64());
    }

    pub fn scroll(&self) -> f64 {
        self.document
            .tree()
            .iter()
            .filter(|(_, node)| node.flags.is_in_document())
            .filter_map(|(_, node)| node.element_data())
            .flat_map(|element| element.attrs.iter())
            .find(|attr| attr.name.local.as_ref() == "data-scroll")
            .and_then(|attr| attr.value.parse().ok())
            .unwrap_or(0.0)
    }

    pub fn paint<R: ImageRenderer>(&self, renderer: &mut R, pixels: &mut Vec<u8>) {
        renderer.render_to_vec(
            |scene| blitz_paint::paint_scene(scene, &self.document, 1.0, WIDTH, HEIGHT),
            pixels,
        );
    }
}

pub const SIZE: (u32, u32) = (WIDTH, HEIGHT);

pub fn capture(path: &Path) -> Result<(), String> {
    let runtime = tokio::runtime::Runtime::new().map_err(|error| error.to_string())?;
    let _guard = runtime.enter();
    let offscreen = Offscreen::new()?;
    let mut pixels = Vec::new();
    offscreen.paint(
        &mut crate::raster::CpuRaster::new(WIDTH, HEIGHT),
        &mut pixels,
    );
    save_png(path, WIDTH, HEIGHT, &pixels)?;
    println!(
        "Screenshot saved: {} ({} × {})",
        path.display(),
        WIDTH,
        HEIGHT
    );
    Ok(())
}

#[cfg(test)]
fn render(document: &DioxusDocument) -> Vec<u8> {
    let mut pixels = Vec::new();
    crate::raster::CpuRaster::new(WIDTH, HEIGHT).render_to_vec(
        |scene| blitz_paint::paint_scene(scene, document, 1.0, WIDTH, HEIGHT),
        &mut pixels,
    );
    pixels
}

fn settle(
    document: &mut DioxusDocument,
    resources: &mut UnboundedReceiver<(usize, Resource)>,
    provider: &Provider<Resource>,
) -> Result<(), String> {
    let started = Instant::now();
    let mut quiet_since = Instant::now();
    while started.elapsed() < Duration::from_secs(20) {
        let mut changed = document.poll(None);
        while let Ok((_, resource)) = resources.try_recv() {
            document.load_resource(resource);
            changed = true;
        }
        document.resolve(started.elapsed().as_secs_f64());
        if changed || !provider.is_empty() || !ready(document) {
            quiet_since = Instant::now();
        }
        if quiet_since.elapsed() >= Duration::from_millis(300) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Err("Screenshot timed out waiting for folder contents, fonts, or thumbnails".into())
}

/// Ready when at least one capture marker exists and every attached marker is true.
fn ready(document: &DioxusDocument) -> bool {
    let mut markers = document
        .tree()
        .iter()
        .filter(|(_, node)| node.flags.is_in_document())
        .filter_map(|(_, node)| {
            node.element_data()?
                .attrs
                .iter()
                .find(|attribute| attribute.name.local.as_ref() == "data-capture-ready")
                .map(|attribute| attribute.value == "true")
        })
        .peekable();
    markers.peek().is_some() && markers.all(|ready| ready)
}

fn save_png(path: &Path, width: u32, height: u32, pixels: &[u8]) -> Result<(), String> {
    let mut encoded = Vec::new();
    let mut encoder = png::Encoder::new(&mut encoded, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut writer| writer.write_image_data(pixels))
        .map_err(|error| error.to_string())?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("Cannot save {}: {error}", path.display()))?;
    file.write_all(&encoded)
        .map_err(|error| format!("Cannot write {}: {error}", path.display()))
}

#[cfg(test)]
mod native_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_capture_preserves_existing_files() {
        let path =
            std::env::temp_dir().join(format!("explorer-png-test-{}.png", std::process::id()));
        save_png(&path, 1, 1, &[12, 34, 56, 255]).unwrap();
        let original = std::fs::read(&path).unwrap();
        let decoder = png::Decoder::new(original.as_slice());
        let mut reader = decoder.read_info().unwrap();
        let mut pixels = vec![0; reader.output_buffer_size()];
        reader.next_frame(&mut pixels).unwrap();
        assert_eq!(pixels, [12, 34, 56, 255]);
        assert!(save_png(&path, 1, 1, &[0, 0, 0, 255]).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        std::fs::remove_file(path).unwrap();
    }
}
