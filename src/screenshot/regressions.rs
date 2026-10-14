use blitz_traits::shell::{ClipboardError, ShellProvider};
use blitz_traits::shell::{ColorScheme, Viewport};
use dioxus_native::{DioxusDocument, DocumentConfig, prelude::*};
use std::sync::Arc;

struct TextClipboard;

impl ShellProvider for TextClipboard {
    fn get_clipboard_text(&self) -> Result<String, ClipboardError> {
        arboard::Clipboard::new()
            .and_then(|mut clipboard| clipboard.get_text())
            .map_err(|_| ClipboardError)
    }
}

fn app() -> Element {
    rsx! {
        div { style: "width:500px;height:400px;",
            div { id: "target", style: "width:200px;height:100px;", "hover me" }
            input { aria_label: "Paste test", autofocus: true, value: "dl " }
        }
    }
}

fn document() -> DioxusDocument {
    let mut document = DioxusDocument::new(
        VirtualDom::new(app),
        DocumentConfig {
            viewport: Some(Viewport::new(500, 400, 1.0, ColorScheme::Light)),
            ..Default::default()
        },
    );
    document.initial_build();
    document.resolve(0.0);
    document
}

#[test]
fn repeated_layout_keeps_node_storage_bounded() {
    let mut document = document();
    document.add_user_agent_stylesheet("#target::before { content: 'kept'; display: block; }");
    document.resolve(0.0);
    let initial = document.tree().len();
    let pseudo = document
        .tree()
        .iter()
        .find_map(|(_, node)| node.before)
        .unwrap();
    let focused = document.get_focussed_node_id();
    document.set_hover_to(30.0, 30.0);
    for _ in 0..32 {
        document.resolve(0.0);
    }
    assert_eq!(document.tree().len(), initial);
    assert!(document.get_node(pseudo).is_some());
    assert_eq!(document.get_focussed_node_id(), focused);
    document.set_hover_to(300.0, 250.0);
}

#[test]
fn removed_hovered_node_does_not_panic_on_pointer_move() {
    let mut document = document();
    document.set_hover_to(30.0, 30.0);
    let hovered = document.get_hover_node_id().unwrap();
    assert_ne!(hovered, document.root_node().id);
    document.mutate().remove_and_drop_node(hovered);
    document.resolve(0.0);
    document.set_hover_to(300.0, 250.0);
}

#[test]
fn inline_hit_without_text_layout_does_not_panic() {
    let mut document = document();
    let target = document
        .tree()
        .iter()
        .find(|(_, node)| {
            node.flags.is_in_document() && node.attr(blitz_dom::local_name!("id")) == Some("target")
        })
        .unwrap()
        .0;
    let node = document.get_node_mut(target).unwrap();
    assert!(node.flags.is_inline_root());
    assert!(
        node.element_data_mut()
            .unwrap()
            .take_inline_layout()
            .is_some()
    );
    document.set_hover_to(30.0, 30.0);
}

#[test]
#[ignore = "Uses the private Wayland clipboard; run with scripts/headless_wayland.sh"]
fn native_editor_pastes_wayland_text_at_caret() {
    let url = "https://example.org/image.png";
    wl_clipboard_rs::copy::Options::new()
        .copy(
            wl_clipboard_rs::copy::Source::Bytes(url.as_bytes().to_vec().into_boxed_slice()),
            wl_clipboard_rs::copy::MimeType::Text,
        )
        .unwrap();
    let mut document = document();
    document.set_shell_provider(Arc::new(TextClipboard));
    document.handle_ui_event(crate::shell::key_down(Key::End, Modifiers::empty()));
    document.handle_ui_event(crate::shell::key_down(
        Key::Character("v".into()),
        Modifiers::CONTROL,
    ));
    let input = document
        .get_focussed_node_id()
        .and_then(|id| document.get_node(id))
        .and_then(|node| node.element_data())
        .and_then(|element| element.text_input_data())
        .unwrap();
    assert_eq!(input.editor.text(), format!("dl {url}").as_str());
    document.handle_ui_event(crate::shell::key_down(
        Key::Character("a".into()),
        Modifiers::CONTROL,
    ));
    document.handle_ui_event(crate::shell::key_down(
        Key::Character("v".into()),
        Modifiers::CONTROL,
    ));
    let input = document
        .get_focussed_node_id()
        .and_then(|id| document.get_node(id))
        .and_then(|node| node.element_data())
        .and_then(|element| element.text_input_data())
        .unwrap();
    assert_eq!(input.editor.text(), url);
}
use blitz_dom::Document;
