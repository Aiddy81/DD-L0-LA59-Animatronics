//! A command palette: a filterable list of rows in a modal, run by click or
//! Enter. It knows nothing about what a row does; the caller supplies rows
//! carrying any `Clone` item and gets the picked item back.
//!
//! The state lives in each egui context's own memory, so every window (main
//! or detached) has an independent palette.

use bevy_egui::egui::{self, Align2, Id, Key, Modifiers, Sense, Stroke, TextStyle};

use crate::theme::Palette;

/// Most the list grows to before it scrolls.
const LIST_MAX_HEIGHT: f32 = 360.0;
/// Widest the palette gets; narrower windows shrink it.
const WIDTH: f32 = 520.0;
/// Room left at the window's sides.
const MARGIN: f32 = 32.0;

/// One entry. `group` and `label` are what the filter matches; `keys` is shown
/// on the right. A disabled row is listed but can't be picked.
pub struct Row<T> {
    pub group: &'static str,
    pub label: String,
    pub keys: &'static str,
    pub enabled: bool,
    pub item: T,
}

#[derive(Clone, Default)]
struct State {
    open: bool,
    query: String,
    /// Position in the filtered list.
    selected: usize,
    /// Set on opening, to focus the text box once.
    just_opened: bool,
}

fn with_state<R>(ctx: &egui::Context, f: impl FnOnce(&mut State) -> R) -> R {
    ctx.data_mut(|d| f(d.get_temp_mut_or_default::<State>(Id::new("command_palette"))))
}

pub fn open(ctx: &egui::Context) {
    with_state(ctx, |s| {
        *s = State {
            open: true,
            just_opened: true,
            ..State::default()
        };
    });
}

pub fn is_open(ctx: &egui::Context) -> bool {
    with_state(ctx, |s| s.open)
}

/// Indices of the `texts` that match `query`, best first. Every
/// whitespace-separated word of the query must match, case-insensitively. A
/// word scores 3 as a prefix of the whole text, 2 at the start of any word in
/// it, 1 anywhere; the scores add up. Ties keep their order, and an empty
/// query keeps everything in order.
fn rank<S: AsRef<str>>(query: &str, texts: &[S]) -> Vec<usize> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let mut scored: Vec<(usize, usize)> = texts
        .iter()
        .enumerate()
        .filter_map(|(i, text)| {
            let text = text.as_ref().to_lowercase();
            let mut total = 0;
            for word in &words {
                total += if text.starts_with(word.as_str()) {
                    3
                } else if text
                    .split(|c: char| !c.is_alphanumeric())
                    .any(|w| w.starts_with(word.as_str()))
                {
                    2
                } else if text.contains(word.as_str()) {
                    1
                } else {
                    return None;
                };
            }
            Some((i, total))
        })
        .collect();
    // Stable, so equal scores stay in table order.
    scored.sort_by_key(|&(_, score)| std::cmp::Reverse(score));
    scored.into_iter().map(|(i, _)| i).collect()
}

/// The next enabled position after (or before) `from`, wrapping round.
fn next_enabled(enabled: &[bool], from: usize, forward: bool) -> Option<usize> {
    let n = enabled.len();
    (1..=n)
        .map(|k| {
            if forward {
                (from + k) % n
            } else {
                (from + n - k % n) % n
            }
        })
        .find(|&i| enabled[i])
}

fn first_enabled(enabled: &[bool]) -> usize {
    enabled.iter().position(|e| *e).unwrap_or(0)
}

/// One full-width line: `Group: label` on the left, the keys on the right.
fn row_ui<T>(ui: &mut egui::Ui, row: &Row<T>, selected: bool) -> egui::Response {
    let height = ui.text_style_height(&TextStyle::Body) + 2.0 * ui.spacing().button_padding.y;
    let size = egui::vec2(ui.available_width(), height);
    ui.add_enabled_ui(row.enabled, |ui| {
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        if ui.is_rect_visible(rect) {
            let visuals = ui.style().interact_selectable(&response, selected);
            if selected || response.hovered() {
                ui.painter()
                    .rect_filled(rect, visuals.corner_radius, visuals.weak_bg_fill);
            }
            let pad = ui.spacing().button_padding.x;
            ui.painter().text(
                rect.left_center() + egui::vec2(pad, 0.0),
                Align2::LEFT_CENTER,
                format!("{}: {}", row.group, row.label),
                TextStyle::Body.resolve(ui.style()),
                visuals.fg_stroke.color,
            );
            ui.painter().text(
                rect.right_center() - egui::vec2(pad, 0.0),
                Align2::RIGHT_CENTER,
                row.keys,
                TextStyle::Monospace.resolve(ui.style()),
                ui.visuals().weak_text_color(),
            );
        }
        response
    })
    .inner
}

/// Draws the palette if it is open and returns the item of the row run this
/// frame. Up and Down move the highlight over the enabled rows, Enter or a
/// click runs one, Esc or a click outside closes.
pub fn show<T: Clone>(ctx: &egui::Context, rows: &[Row<T>], palette: &Palette) -> Option<T> {
    let mut st = with_state(ctx, |s| s.clone());
    if !st.open {
        return None;
    }
    let texts: Vec<String> = rows
        .iter()
        .map(|r| format!("{} {}", r.group, r.label))
        .collect();
    let mut visible = rank(&st.query, &texts);
    let enabled_of =
        |visible: &[usize]| -> Vec<bool> { visible.iter().map(|&i| rows[i].enabled).collect() };
    let mut enabled = enabled_of(&visible);
    if st.just_opened {
        st.selected = first_enabled(&enabled);
    }
    st.selected = st.selected.min(visible.len().saturating_sub(1));

    // Taken before the text box is added, which would otherwise move its
    // cursor on the arrows and give up focus on Enter.
    let (up, down, enter, escape) = ctx.input_mut(|i| {
        (
            i.consume_key(Modifiers::NONE, Key::ArrowUp),
            i.consume_key(Modifiers::NONE, Key::ArrowDown),
            i.consume_key(Modifiers::NONE, Key::Enter),
            i.consume_key(Modifiers::NONE, Key::Escape),
        )
    });
    let mut moved = st.just_opened;
    if up || down {
        if let Some(next) = next_enabled(&enabled, st.selected, down) {
            st.selected = next;
            moved = true;
        }
    }
    let mut picked = if enter && enabled.get(st.selected).copied().unwrap_or(false) {
        Some(visible[st.selected])
    } else {
        None
    };
    let mut close = escape;

    let width = WIDTH
        .min(ctx.screen_rect().width() - 2.0 * MARGIN)
        .max(200.0);
    let frame = egui::Frame::popup(&ctx.style())
        .fill(palette.surface)
        .stroke(Stroke::new(palette.stroke_width, palette.border));
    let modal = egui::Modal::new(Id::new("command_palette_modal"))
        .frame(frame)
        .show(ctx, |ui| {
            ui.set_width(width);
            let edit = ui.add(
                egui::TextEdit::singleline(&mut st.query)
                    .hint_text("Type a command\u{2026}")
                    .desired_width(f32::INFINITY),
            );
            // Kept while open, so a click on the scroll bar doesn't strand typing.
            if st.just_opened || !edit.has_focus() {
                edit.request_focus();
            }
            if edit.changed() {
                visible = rank(&st.query, &texts);
                enabled = enabled_of(&visible);
                st.selected = first_enabled(&enabled);
                moved = true;
            }
            ui.add_space(ui.spacing().item_spacing.y);
            if visible.is_empty() {
                ui.weak("No matching command");
            }
            egui::ScrollArea::vertical()
                .max_height(LIST_MAX_HEIGHT)
                // The modal gives the list little room, so without this egui
                // falls back to its 64 px minimum and shows two rows.
                .min_scrolled_height(LIST_MAX_HEIGHT)
                .auto_shrink([false, true])
                .show(ui, |list| {
                    for (pos, &i) in visible.iter().enumerate() {
                        let selected = pos == st.selected;
                        let response = row_ui(list, &rows[i], selected);
                        if selected && moved {
                            response.scroll_to_me(None);
                        }
                        if response.clicked() {
                            picked = Some(i);
                        }
                    }
                });
        });
    if modal.should_close() {
        close = true;
    }

    st.just_opened = false;
    let result = picked.map(|i| rows[i].item.clone());
    if result.is_some() || close {
        st = State::default();
    }
    with_state(ctx, |s| *s = st);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_beats_word_start_beats_substring() {
        let texts = ["Options Board", "Run Pause", "Panel Plot", "Motion Nudge"];
        // "pa": prefix of "Panel Plot", a word start in "Run Pause", nowhere else.
        assert_eq!(rank("pa", &texts), [2, 1]);
        // "ause": only a substring, still matches.
        assert_eq!(rank("ause", &texts), [1]);
    }

    #[test]
    fn every_word_must_match() {
        let texts = ["Theme Dark", "Light Dark", "Theme Light"];
        assert_eq!(rank("dark th", &texts), [0]);
        assert!(rank("dark zzz", &texts).is_empty());
    }

    #[test]
    fn empty_query_keeps_order() {
        assert_eq!(rank("  ", &["b", "a", "c"]), [0, 1, 2]);
    }

    #[test]
    fn ties_keep_table_order() {
        assert_eq!(rank("run", &["Run b", "Run a", "x Run"]), [0, 1, 2]);
    }

    #[test]
    fn highlight_skips_disabled_and_wraps() {
        let enabled = [true, false, true, false];
        assert_eq!(next_enabled(&enabled, 0, true), Some(2));
        assert_eq!(next_enabled(&enabled, 2, true), Some(0));
        assert_eq!(next_enabled(&enabled, 0, false), Some(2));
        assert_eq!(next_enabled(&enabled, 2, false), Some(0));
        assert_eq!(next_enabled(&[false, false], 0, true), None);
        assert_eq!(next_enabled(&[], 0, true), None);
    }
}
