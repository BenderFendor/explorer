use super::*;
use blitz_traits::events::{BlitzMouseButtonEvent, MouseEventButton, MouseEventButtons, UiEvent};
use blitz_traits::net::NetCallback;
use dioxus_native::prelude::{Key, Modifiers};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Session {
    document: DioxusDocument,
    resources: UnboundedReceiver<(usize, Resource)>,
    provider: Arc<Provider<Resource>>,
}

impl Session {
    fn new(folder: std::path::PathBuf) -> Self {
        let (resources, callback) = MpscCallback::new();
        let fonts = Arc::new(AtomicUsize::new(0));
        let loaded = fonts.clone();
        let provider = Arc::new(Provider::new(Arc::new(
            move |id, result: Result<Resource, Option<String>>| {
                if let Ok(Resource::Font(bytes)) = &result {
                    assert_eq!(&bytes[..4], &[0, 1, 0, 0]);
                    loaded.fetch_add(1, Ordering::Relaxed);
                }
                callback.call(id, result);
            },
        )));
        let mut vdom = VirtualDom::new(crate::ui::app);
        vdom.insert_any_root_context(Box::new(folder));
        let mut document = DioxusDocument::new(
            vdom,
            DocumentConfig {
                viewport: Some(Viewport::new(WIDTH, HEIGHT, 1.0, ColorScheme::Light)),
                net_provider: Some(provider.clone()),
                ..Default::default()
            },
        );
        document.initial_build();
        let mut session = Self {
            document,
            resources,
            provider,
        };
        session.settle();
        assert_eq!(
            fonts.load(Ordering::Relaxed),
            4,
            "All bundled font faces must load through Blitz"
        );
        session
    }

    #[track_caller]
    fn settle(&mut self) {
        if let Err(error) = settle(&mut self.document, &mut self.resources, &self.provider) {
            panic!("{error}; attached text: {}", self.text());
        }
        if crate::shell::apply_caret_request(&mut self.document) {
            self.settle();
        }
    }

    #[track_caller]
    fn key(&mut self, key: Key, modifiers: Modifiers) {
        self.document
            .handle_ui_event(crate::shell::key_down(key, modifiers));
        self.settle();
    }

    #[track_caller]
    fn keys(&mut self, text: &str) {
        for character in text.chars() {
            self.key(Key::Character(character.to_string()), Modifiers::empty());
        }
    }

    #[track_caller]
    fn input_text(&mut self, text: &str) {
        self.key(Key::Character(text.to_owned()), Modifiers::empty());
    }

    #[track_caller]
    fn console(&mut self, text: &str) {
        self.keys(":");
        self.assert_focus("Command");
        self.input_text(text);
        self.key(Key::Enter, Modifiers::empty());
    }

    fn mouse(&mut self, x: f32, y: f32, button: MouseEventButton) {
        let buttons = match button {
            MouseEventButton::Secondary => MouseEventButtons::Secondary,
            _ => MouseEventButtons::Primary,
        };
        let event = BlitzMouseButtonEvent {
            x,
            y,
            button,
            buttons,
            mods: Modifiers::empty(),
        };
        self.document
            .handle_ui_event(UiEvent::MouseMove(event.clone()));
        self.document
            .handle_ui_event(UiEvent::MouseDown(event.clone()));
        self.document.handle_ui_event(UiEvent::MouseUp(event));
        self.settle();
    }

    fn text(&self) -> String {
        self.document
            .tree()
            .iter()
            .filter(|(_, node)| node.flags.is_in_document())
            .filter_map(|(_, node)| node.text_data())
            .filter(|text| text.content.len() < 1000)
            .map(|text| text.content.as_str())
            .collect::<Vec<_>>()
            .join("|")
    }

    #[track_caller]
    fn assert_text(&self, expected: &str) {
        let text = self.text();
        assert!(text.contains(expected), "Missing {expected:?} in {text:?}");
    }

    #[track_caller]
    fn assert_no_text(&self, unexpected: &str) {
        let text = self.text();
        assert!(
            !text.contains(unexpected),
            "Unexpected {unexpected:?} in {text:?}"
        );
    }

    #[track_caller]
    fn assert_focus(&self, label: &str) {
        let focused = self
            .document
            .get_focussed_node_id()
            .and_then(|id| self.document.get_node(id))
            .and_then(|node| node.element_data())
            .unwrap();
        assert!(
            focused
                .attrs
                .iter()
                .any(|attr| attr.name.local.as_ref() == "aria-label" && attr.value == label),
            "Expected focus on {label}; got {:?}",
            focused.attrs
        );
    }

    /// Saves the current frame when EXPLORER_CAPTURE_DIR is set, for visual review.
    fn capture(&self, name: &str) {
        let Some(folder) = std::env::var_os("EXPLORER_CAPTURE_DIR") else {
            return;
        };
        let path = std::path::Path::new(&folder).join(format!("{name}.png"));
        let _ = std::fs::remove_file(&path);
        save_png(&path, WIDTH, HEIGHT, &render(&self.document)).unwrap();
    }
}

struct Fixture(std::path::PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fixture() -> Fixture {
    static SERIAL: AtomicUsize = AtomicUsize::new(0);
    let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
    let root =
        std::env::temp_dir().join(format!("explorer-native-{}-{serial}", std::process::id()));
    let studio = root.join("Studio");
    std::fs::create_dir_all(studio.join("Alpha")).unwrap();
    std::fs::create_dir(studio.join("Beta")).unwrap();
    std::fs::write(
        studio.join("notes.txt"),
        "Plain text preview\nsecond line\n",
    )
    .unwrap();
    for name in ["file2.txt", "file10.txt", ".secret"] {
        std::fs::write(studio.join(name), "x").unwrap();
    }
    let large = root.join("Large");
    std::fs::create_dir(&large).unwrap();
    for index in 0..500 {
        std::fs::write(large.join(format!("item-{index:03}.txt")), "").unwrap();
    }
    Fixture(root)
}

#[test]
fn ranger_keys_file_operations_and_menus_use_native_events() {
    let fixture = fixture();
    let studio = fixture.0.join("Studio");
    unsafe {
        std::env::set_var("EXPLORER_STATE_DIR", fixture.0.join("state"));
    }
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _guard = runtime.enter();
    let mut session = Session::new(studio.clone());
    session.assert_text("6 items");
    session.capture("01-grid");

    // Grid arrows: Right steps to the neighbour, Down moves a whole row (5 columns),
    // Left/Up return. Then descend with l and come back with h.
    session.key(Key::ArrowRight, Modifiers::empty());
    session.key(Key::Enter, Modifiers::empty());
    // Inside an empty folder, only the breadcrumb still names Beta.
    session.assert_text("0 items");
    session.assert_text("|Beta|");
    session.key(Key::Backspace, Modifiers::empty());
    session.key(Key::ArrowLeft, Modifiers::empty());
    session.keys("l");
    session.assert_text("Empty folder");
    session.keys("h");
    session.assert_text("6 items");
    session.assert_text("Studio");

    // Hidden files, natural order, and the console.
    session.keys("zh");
    session.assert_text("5 items");
    session.keys("zh:");
    session.assert_focus("Command");
    session.keys("mkdir Gamma");
    session.capture("02-console");
    session.key(Key::Enter, Modifiers::empty());
    session.assert_text("7 items");
    assert!(studio.join("Gamma").is_dir());
    session.assert_focus("File navigation");

    // Filter, copy, paste with a "(copy)" name, then delete with confirmation.
    session.keys("/");
    session.assert_focus("Filter this folder");
    session.keys("notes");
    session.assert_text("1 items");
    session.key(Key::Enter, Modifiers::empty());
    session.keys("yy");
    session.key(Key::Escape, Modifiers::empty());
    session.assert_text("7 items");
    session.keys("pp");
    session.assert_text("8 items");
    assert!(studio.join("notes (copy).txt").is_file());
    session.keys("dD");
    session.assert_text("permanently");
    session.capture("03-confirm");
    session.keys("y");
    session.assert_text("7 items");
    assert!(!studio.join("notes (copy).txt").exists());
    assert!(studio.join("notes.txt").is_file());

    // Rename through the console prefill, like ranger's cw.
    session.keys("gg");
    session.keys("cw");
    session.assert_focus("Command");
    session.keys("2");
    session.key(Key::Enter, Modifiers::empty());
    session.assert_text("Gamma2");
    assert!(studio.join("Gamma2").is_dir());

    // Views: list and Miller columns with a text preview.
    session.key(Key::Character("2".into()), Modifiers::CONTROL);
    session.assert_text("file10.txt");
    session.keys("G");
    session.key(Key::Character("3".into()), Modifiers::CONTROL);
    session.assert_text("Plain text preview");
    session.capture("04-columns");
    session.key(Key::Character("1".into()), Modifiers::CONTROL);

    // Context menu on an item, then on empty space.
    session.mouse(320.0, 130.0, MouseEventButton::Secondary);
    session.assert_text("Move to trash");
    session.capture("05-menu");
    session.key(Key::Escape, Modifiers::empty());
    session.assert_no_text("Move to trash");
    session.mouse(900.0, 600.0, MouseEventButton::Secondary);
    session.assert_text("New folder");
    session.key(Key::Escape, Modifiers::empty());

    // Grid zoom through Thunar's Ctrl+= and ranger-style plain keys.
    session.key(Key::Character("=".into()), Modifiers::CONTROL);
    session.keys("+");
    session.capture("08-zoom-in");
    session.keys("++++++++++++");
    session.capture("08-max-zoom");
    session.key(Key::Character("0".into()), Modifiers::CONTROL);
    session.keys("--");
    session.capture("09-zoom-out");
    session.key(Key::Character("0".into()), Modifiers::CONTROL);

    // Tabs and help.
    session.keys("gn");
    session.assert_text("Studio|Studio");
    session.keys("q");
    session.assert_no_text("Studio|Studio");
    session.keys("?");
    session.assert_text("Set bookmark");
    session.capture("06-help");
    session.key(Key::Escape, Modifiers::empty());

    // Bookmarks and a large folder rendered through the virtual viewport.
    session.keys("on");
    session.keys("ma");
    session.key(Key::Character("l".into()), Modifiers::CONTROL);
    session.assert_focus("Folder path");
    session.key(Key::Character("a".into()), Modifiers::CONTROL);
    session.keys(&fixture.0.join("Large").to_string_lossy());
    session.key(Key::Enter, Modifiers::empty());
    session.assert_text("500 items");
    session.assert_text("item-000.txt");
    session.assert_no_text("item-499.txt");
    // Wheel events go through the same shell channel as the window and move in one poll.
    crate::shell::publish(crate::shell::ShellEvent::Wheel {
        delta: 56.0,
        x: 600.0,
        y: 400.0,
        zoom: false,
    });
    session.document.poll(None);
    let scroll = session
        .document
        .tree()
        .iter()
        .filter(|(_, node)| node.flags.is_in_document())
        .filter_map(|(_, node)| node.element_data())
        .flat_map(|element| element.attrs.iter())
        .find(|attr| attr.name.local.as_ref() == "data-scroll")
        .unwrap()
        .value
        .parse::<f64>()
        .unwrap();
    assert_eq!(
        scroll, 56.0,
        "wheel input must reach its target in the first UI poll"
    );
    crate::shell::publish(crate::shell::ShellEvent::Wheel {
        delta: -56.0,
        x: 600.0,
        y: 400.0,
        zoom: false,
    });
    session.settle();
    // Two rows down in a 5-column grid lands on the eleventh item.
    session.key(Key::ArrowDown, Modifiers::empty());
    session.key(Key::ArrowDown, Modifiers::empty());
    session.keys("dD");
    session.assert_text("item-010.txt\u{201d} permanently");
    session.keys("n");
    assert!(fixture.0.join("Large/item-010.txt").exists());
    session.keys("G");
    session.assert_text("item-499.txt");
    session.assert_no_text("item-000.txt");
    session.capture("07-large");
    session.keys("'a");
    session.assert_text("Gamma2");
    // Returning restores the remembered cursor and scrolls it into view.
    session.keys("H");
    session.assert_text("item-499.txt");
    session.assert_no_text("item-000.txt");
    // Console download and Save as are exercised through native text-field events.
    let (url, server) = crate::imports::tests::serve(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 4\r\n\r\ntext",
    );
    session.console(&format!("dl {url}"));
    server.join().unwrap();
    session.assert_focus("Save download as");
    session.key(Key::Character("a".into()), Modifiers::CONTROL);
    session.input_text("downloaded.txt");
    session.key(Key::Enter, Modifiers::empty());
    assert_eq!(
        std::fs::read(fixture.0.join("Large/downloaded.txt")).unwrap(),
        b"text"
    );
    let (url, server) =
        crate::imports::tests::serve("HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nnext");
    session.console(&format!("dl {url} explicit name.txt"));
    server.join().unwrap();
    assert_eq!(
        std::fs::read(fixture.0.join("Large/explicit name.txt")).unwrap(),
        b"next"
    );
    let (url, server) =
        crate::imports::tests::serve("HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\ndrop");
    session.console(&format!("dl {url}"));
    server.join().unwrap();
    session.key(Key::Escape, Modifiers::empty());
    assert!(
        !std::fs::read_dir(fixture.0.join("Large"))
            .unwrap()
            .any(|entry| entry
                .unwrap()
                .file_name()
                .as_encoded_bytes()
                .starts_with(b".explorer-import-"))
    );
}

#[test]
#[ignore = "Uses the private Wayland clipboard; run with scripts/headless_wayland.sh"]
fn native_clipboard_paste_into_folder() {
    let fixture = fixture();
    let folder = fixture.0.join("Studio");
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _guard = runtime.enter();
    crate::clipboard::enable();
    let mut session = Session::new(folder.clone());
    let mut encoded = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut encoded, 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[200, 100, 50, 255])
            .unwrap();
    }
    let offer = |mime: &str, bytes: &[u8]| {
        wl_clipboard_rs::copy::Options::new()
            .copy(
                wl_clipboard_rs::copy::Source::Bytes(bytes.to_vec().into_boxed_slice()),
                wl_clipboard_rs::copy::MimeType::Specific(mime.into()),
            )
            .unwrap();
        std::thread::sleep(Duration::from_millis(100));
    };
    offer("image/png", &encoded);
    session.key(Key::Character("v".into()), Modifiers::CONTROL);
    assert_eq!(
        std::fs::read(folder.join("clipboard.png")).unwrap(),
        encoded
    );
    session.key(Key::Character("v".into()), Modifiers::CONTROL);
    assert_eq!(
        std::fs::read(folder.join("clipboard (copy).png")).unwrap(),
        encoded
    );
    offer("text/plain", b"hello\nworld");
    session.key(Key::Character("v".into()), Modifiers::CONTROL);
    assert_eq!(
        std::fs::read(folder.join("clipboard.txt")).unwrap(),
        b"hello\nworld"
    );
    let (url, server) = crate::imports::tests::serve(
        "HTTP/1.1 200 OK\r\nContent-Disposition: attachment; filename=link.txt\r\nContent-Length: 4\r\n\r\nlink",
    );
    offer("text/plain", url.as_bytes());
    session.key(Key::Character("v".into()), Modifiers::CONTROL);
    server.join().unwrap();
    assert_eq!(std::fs::read(folder.join("link.txt")).unwrap(), b"link");
    let sample = "https://encrypted-tbn0.gstatic.com/images?q=tbn:ANd9GcQNGFxdO-Ztczq0IlXUL6-7MkqOCEjE3eduDvNvwYKOSxnlcvYPgeSjguHvgH7gDPitmj4LPQU_Bm5W-FTfIO3Px5bE_Ucul7yNiZtsiVzA8w&s=10";
    session.console(&format!("dl {sample}"));
    let started = Instant::now();
    while !session.text().contains("Save as") && started.elapsed() < Duration::from_secs(15) {
        std::thread::sleep(Duration::from_millis(50));
        session.settle();
    }
    session.assert_focus("Save download as");
    session.capture("clipboard-save-as");
    session.key(Key::Character("a".into()), Modifiers::CONTROL);
    session.input_text("sample.webp");
    session.key(Key::Enter, Modifiers::empty());
    let bytes = std::fs::read(folder.join("sample.webp")).unwrap();
    assert_eq!(&bytes[..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WEBP");
    assert!(bytes.len() > 1000);
}
