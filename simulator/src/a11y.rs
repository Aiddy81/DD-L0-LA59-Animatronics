//! Screen readers and speech control (#94). bevy_egui hands egui's AccessKit
//! tree to each window's adapter; this module fills the gaps around that:
//! AccessKit actions back into egui, dictated text, and helpers that give
//! controls names people can say ("click Restart", not "click 🔄 Restart").

use bevy::a11y::ActionRequest;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use bevy_egui::egui::{self, accesskit};
use bevy_egui::input::{EguiInputEvent, ModifierKeysState, WindowToEguiContextMap};
use bevy_egui::{EguiContext, EguiInputSet};

pub struct A11yPlugin;

impl Plugin for A11yPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PreUpdate,
            (forward_actions, forward_packet_text).in_set(EguiInputSet::ReadBevyMessages),
        );
    }
}

/// Narrator's "activate", Voice Access's "click …" and focus moves arrive as
/// AccessKit action requests, which Bevy turns into `ActionRequest` messages
/// and bevy_egui never passes on. The message doesn't say which window asked,
/// so every context gets it; egui acts only where a widget has the target id.
fn forward_actions(
    mut requests: MessageReader<ActionRequest>,
    contexts: Query<Entity, With<EguiContext>>,
    mut out: MessageWriter<EguiInputEvent>,
) {
    for request in requests.read() {
        for context in &contexts {
            out.write(EguiInputEvent {
                context,
                event: egui::Event::AccessKitActionRequest(request.0.clone()),
            });
        }
    }
}

/// Text typed as Unicode characters rather than keys (Voice Access dictation,
/// on-screen keyboards, `SendInput` with `KEYEVENTF_UNICODE`) comes from
/// Windows as `VK_PACKET`. winit reports that key as `Unidentified` with the
/// character in `text`, and bevy_egui only turns `Key::Character` into text,
/// so it was dropped. Ordinary keys still go through bevy_egui alone.
fn forward_packet_text(
    mut keys: MessageReader<KeyboardInput>,
    modifiers: Res<ModifierKeysState>,
    map: Res<WindowToEguiContextMap>,
    mut out: MessageWriter<EguiInputEvent>,
) {
    for key in keys.read() {
        if !key.state.is_pressed() || !matches!(key.logical_key, Key::Unidentified(_)) {
            continue;
        }
        let Some(text) = key.text.as_ref() else {
            continue;
        };
        if text.is_empty() || text.chars().any(char::is_control) {
            continue;
        }
        if !modifiers.text_input_is_allowed() {
            continue;
        }
        for &context in map
            .window_to_contexts
            .get(&key.window)
            .into_iter()
            .flatten()
        {
            out.write(EguiInputEvent {
                context,
                event: egui::Event::Text(text.to_string()),
            });
        }
    }
}

/// `label` without the emoji or symbols around it, the part people say: "🔄
/// Restart" → "Restart", "Layouts ⏵" → "Layouts". ASCII stays, so "+10 ms"
/// keeps its plus. A label that is only a glyph comes back empty and needs a
/// name of its own.
pub fn spoken(label: &str) -> &str {
    label.trim_matches(|c: char| c.is_whitespace() || (!c.is_ascii() && !c.is_alphanumeric()))
}

/// Gives a button the accessible name `label`, replacing what egui took from
/// its text. For glyph-only buttons and ones whose text says too little.
pub fn name(resp: egui::Response, label: &str) -> egui::Response {
    let enabled = resp.enabled();
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
    resp
}

/// Names a control that isn't a button (a text box, say) without changing
/// its role or state.
pub fn label(resp: egui::Response, text: &str) -> egui::Response {
    resp.ctx
        .accesskit_node_builder(resp.id, |node| node.set_label(text));
    resp
}

/// Drops the leading emoji from the name egui gave a control, so speech
/// commands match what people read. Leaves names that are only a glyph alone;
/// those get `name`.
pub fn tidy(resp: egui::Response) -> egui::Response {
    resp.ctx.accesskit_node_builder(resp.id, |node| {
        let Some(label) = node.label() else {
            return;
        };
        let said = spoken(label);
        if !said.is_empty() && said.len() != label.len() {
            let said = said.to_owned();
            node.set_label(said);
        }
    });
    resp
}

/// egui_dock paints its tab buttons, and their close buttons, without telling
/// AccessKit what they are. The close button's id is egui_dock's own
/// (`tab id + "close-button"`); it is named only if drawn this frame, so a
/// dock without close buttons gets no phantom node.
pub fn tab(resp: &egui::Response, title: &str) {
    resp.ctx.accesskit_node_builder(resp.id, |node| {
        node.set_role(accesskit::Role::Tab);
        node.set_label(title);
    });
    let close = resp.id.with("close-button");
    if resp.ctx.read_response(close).is_some() {
        resp.ctx.accesskit_node_builder(close, |node| {
            node.set_role(accesskit::Role::Button);
            node.set_label(format!("Close {title}"));
        });
    }
}

/// Marks `ui` as a named group, so a screen reader announces the panel or
/// toolbar it is entering.
pub fn region(ui: &egui::Ui, title: &str) {
    ui.ctx().accesskit_node_builder(ui.unique_id(), |node| {
        node.set_role(accesskit::Role::Group);
        node.set_label(title);
    });
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// Draws `draw` with AccessKit on and returns (role, name) of every node
    /// that has a name. Two passes, as popups and modals size themselves
    /// invisibly on their first.
    pub fn named_nodes(mut draw: impl FnMut(&egui::Context)) -> Vec<(accesskit::Role, String)> {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let _ = ctx.run(egui::RawInput::default(), &mut draw);
        let out = ctx.run(egui::RawInput::default(), &mut draw);
        let update = out
            .platform_output
            .accesskit_update
            .expect("AccessKit is on, so the pass has a tree");
        update
            .nodes
            .iter()
            .filter_map(|(_, node)| node.label().map(|l| (node.role(), l.to_owned())))
            .collect()
    }

    #[test]
    fn spoken_drops_leading_glyphs_only() {
        assert_eq!(spoken("🔄 Restart"), "Restart");
        assert_eq!(spoken("⏹ Stop skip"), "Stop skip");
        assert_eq!(spoken("\u{2b06} Deploy"), "Deploy");
        assert_eq!(spoken("+10 ms"), "+10 ms");
        assert_eq!(spoken("Command palette…"), "Command palette");
        assert_eq!(spoken("Layouts ⏵"), "Layouts");
        assert_eq!(spoken("⌨"), "");
        assert_eq!(spoken("−"), "");
    }

    #[test]
    fn tidy_and_name_set_the_tree_name() {
        let nodes = named_nodes(|ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                region(ui, "Toolbar");
                tidy(ui.button("📷 Screenshot"));
                tidy(ui.button("⌨"));
                name(ui.button("⚙"), "View");
            });
        });
        let names: Vec<&str> = nodes.iter().map(|(_, n)| n.as_str()).collect();
        assert!(names.contains(&"Toolbar"), "{names:?}");
        assert!(names.contains(&"Screenshot"), "{names:?}");
        assert!(names.contains(&"View"), "{names:?}");
        // A glyph-only name is left for `name` to fix, not blanked.
        assert!(names.contains(&"⌨"), "{names:?}");
    }
}
