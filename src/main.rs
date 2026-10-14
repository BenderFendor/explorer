mod bench;
mod cli;
mod clipboard;
mod dnd;
mod file_manager1;
mod filesystem;
mod history;
mod image_metadata;
mod extensions;
mod icons;
mod imports;
mod operations;
mod places;
mod prefs;
mod preview;
mod raster;
mod screenshot;
mod shell;
mod theme;
mod thumbnails;
mod ui;
mod watch;

fn main() {
    if let Err(error) = run() {
        eprintln!("Explorer: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    if !cli::initialize()? {
        return Ok(());
    }
    if cli::options().dbus_service {
        return file_manager1::serve();
    }
    raster::configure_from_env()?;
    if cli::options().bench {
        return bench::run(cli::options().backend()?);
    }
    if let Some(path) = &cli::options().screenshot {
        return screenshot::capture(path);
    }
    use winit::platform::wayland::WindowAttributesExtWayland;
    // The app ID matches the desktop entry, so docks and xdg tools associate the window.
    clipboard::enable();
    let window = dioxus_native::WindowAttributes::default()
        .with_name("explorer", "explorer")
        .with_title("Explorer")
        .with_inner_size(dioxus_native::LogicalSize::new(1120.0, 740.0))
        .with_min_inner_size(dioxus_native::LogicalSize::new(560.0, 360.0));
    shell::launch(ui::app, window, cli::options().backend()?)
}
