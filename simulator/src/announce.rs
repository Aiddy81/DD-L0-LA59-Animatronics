//! The status bar's spoken line: what just happened, in words, for people who
//! can't see the 3D view or the plots or hear the sound. The status bar shows
//! it in a polite AccessKit live region, so a screen reader reads each new
//! message without moving focus.
//!
//! Events come in bursts (a boop, then its sound, then the behaviour change),
//! so they are gathered and joined into one message, and a new message goes
//! out at most once every `GAP` seconds. Errors queue apart and are never
//! dropped. Everything here is plain data driven by `now` (seconds), so it is
//! tested without egui.

use std::collections::VecDeque;
use std::path::Path;

use crate::hw::MarkerKind;
use crate::ui::capitalised;

/// Least time between two messages, so speech isn't cut off or flooded.
pub const GAP: f64 = 1.5;
/// How long a burst may keep growing before it is said.
pub const GATHER: f64 = 0.25;
/// How many events one message joins; the rest of a busy burst is dropped.
const MAX_PARTS: usize = 3;
/// Errors kept waiting; any beyond are counted and said with the last one.
const MAX_ERRORS: usize = 32;
/// How long a behaviour must last before it is said, so a task chain passing
/// through a link for a frame isn't read out.
pub const BEHAVIOUR_HOLD: f64 = 0.3;
/// What `ui::current_behaviour` shows when code.py does nothing.
const NO_BEHAVIOUR: &str = "\u{2014}";

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Priority {
    Info,
    Important,
    Error,
}

struct Pending {
    text: String,
    priority: Priority,
    count: u32,
}

#[derive(Default)]
pub struct Announcer {
    /// Events not yet said, oldest first; errors are in `errors` instead.
    pending: Vec<Pending>,
    /// When the oldest of `pending` arrived.
    first_at: Option<f64>,
    errors: VecDeque<String>,
    /// Errors that arrived while `errors` was full.
    overflow: usize,
    shown: String,
    shown_at: Option<f64>,
}

impl Announcer {
    pub fn push(&mut self, text: impl Into<String>, priority: Priority, now: f64) {
        let text = text.into();
        if text.is_empty() {
            return;
        }
        if priority == Priority::Error {
            if self.errors.len() < MAX_ERRORS {
                self.errors.push_back(text);
            } else {
                self.overflow += 1;
            }
            return;
        }
        // A repeat counts up and moves to the end, as it is now the latest.
        let entry = match self.pending.iter().position(|p| p.text == text) {
            Some(i) => {
                let mut p = self.pending.remove(i);
                p.count += 1;
                p.priority = p.priority.max(priority);
                p
            }
            None => Pending {
                text,
                priority,
                count: 1,
            },
        };
        self.pending.push(entry);
        self.first_at.get_or_insert(now);
    }

    /// The text to show this frame. When a new message is the same as the one
    /// showing, the line is blank for this one frame: screen readers are told
    /// about a change of text, and the same text again is no change.
    pub fn tick(&mut self, now: f64) -> &str {
        let ready = self.shown_at.is_none_or(|t| now - t >= GAP);
        if ready {
            if let Some(next) = self.next(now) {
                let repeat = next == self.shown;
                self.shown = next;
                self.shown_at = Some(now);
                if repeat {
                    return "";
                }
            }
        }
        &self.shown
    }

    /// The oldest error, else the gathered events once the burst has had
    /// `GATHER` to finish.
    fn next(&mut self, now: f64) -> Option<String> {
        if let Some(error) = self.errors.pop_front() {
            if self.errors.is_empty() && self.overflow > 0 {
                let more = std::mem::take(&mut self.overflow);
                return Some(format!("{error}. {more} more errors in the log"));
            }
            return Some(error);
        }
        let first = self.first_at?;
        if now - first < GATHER {
            return None;
        }
        self.first_at = None;
        let mut parts = std::mem::take(&mut self.pending);
        if parts.len() > MAX_PARTS {
            // Keep the important ones, then the latest; say them in order.
            let mut keep: Vec<usize> = (0..parts.len()).collect();
            keep.sort_by_key(|&i| (std::cmp::Reverse(parts[i].priority), std::cmp::Reverse(i)));
            keep.truncate(MAX_PARTS);
            let mut i = 0;
            parts.retain(|_| {
                i += 1;
                keep.contains(&(i - 1))
            });
        }
        let said: Vec<String> = parts
            .into_iter()
            .map(|p| match p.count {
                1 => p.text,
                n => format!("{}, {n} times", p.text),
            })
            .collect();
        Some(said.join(". "))
    }
}

/// Says a new behaviour once it has lasted `BEHAVIOUR_HOLD`.
#[derive(Default)]
pub struct BehaviourWatch {
    said: Option<String>,
    seen: Option<(String, f64)>,
}

impl BehaviourWatch {
    /// `behaviour` is `ui::current_behaviour`'s text this frame.
    pub fn update(&mut self, now: f64, behaviour: &str) -> Option<String> {
        if behaviour == NO_BEHAVIOUR || behaviour.is_empty() {
            self.seen = None;
            return None;
        }
        if self.seen.as_ref().is_none_or(|(b, _)| b != behaviour) {
            self.seen = Some((behaviour.to_owned(), now));
        }
        let (b, since) = self.seen.as_ref()?;
        if now - since < BEHAVIOUR_HOLD || self.said.as_ref() == Some(b) {
            return None;
        }
        self.said = Some(b.clone());
        Some(format!("Now {}", b.replace('_', " ")))
    }

    /// A restart says the first behaviour again.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// What a plot marker says. Restarts matter most: everything after them is a
/// new run.
pub fn marker_text(kind: MarkerKind, label: &str) -> (String, Priority) {
    match kind {
        MarkerKind::Motion => (capitalised(label), Priority::Info),
        MarkerKind::Sound => (format!("Sound: {}", sound_name(label)), Priority::Info),
        MarkerKind::Idle => ("Idle trigger".to_owned(), Priority::Info),
        MarkerKind::Restart if label == "restart" => ("Restarted".to_owned(), Priority::Important),
        MarkerKind::Restart => (
            format!("{} started", capitalised(label)),
            Priority::Important,
        ),
    }
}

/// "noseBoops.wav" → "noseBoops": the extension says nothing when spoken.
fn sound_name(file: &str) -> &str {
    Path::new(file)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ticks every 10 ms from `from` to `to`, returning each change of text.
    fn said(a: &mut Announcer, from: f64, to: f64) -> Vec<String> {
        let mut out = Vec::new();
        let mut last = a.shown.clone();
        let mut t = from;
        while t <= to {
            let now = a.tick(t).to_owned();
            if now != last {
                if !now.is_empty() {
                    out.push(now.clone());
                }
                last = now;
            }
            t += 0.01;
        }
        out
    }

    #[test]
    fn a_burst_becomes_one_message() {
        let mut a = Announcer::default();
        a.push("Boop 1.5 g", Priority::Info, 0.0);
        a.push("Sound: noseBoops", Priority::Info, 0.05);
        a.push("Now nose boops", Priority::Info, 0.2);
        assert_eq!(a.tick(0.1), "", "waits for the burst to finish");
        assert_eq!(
            said(&mut a, 0.1, 1.0),
            ["Boop 1.5 g. Sound: noseBoops. Now nose boops"]
        );
    }

    #[test]
    fn at_most_one_message_per_gap() {
        let mut a = Announcer::default();
        a.push("Nudge", Priority::Info, 0.0);
        assert_eq!(a.tick(GATHER), "Nudge");
        a.push("Shake", Priority::Info, GATHER + 0.1);
        assert_eq!(a.tick(1.0), "Nudge", "still inside the gap");
        assert_eq!(a.tick(GATHER + GAP), "Shake");
    }

    #[test]
    fn repeats_are_counted() {
        let mut a = Announcer::default();
        for t in [0.0, 0.05, 0.1] {
            a.push("Nudge", Priority::Info, t);
        }
        assert_eq!(said(&mut a, 0.0, 1.0), ["Nudge, 3 times"]);
    }

    #[test]
    fn a_busy_burst_keeps_the_important_and_the_latest() {
        let mut a = Announcer::default();
        a.push("Restarted", Priority::Important, 0.0);
        for (i, t) in ["One", "Two", "Three", "Four"].iter().enumerate() {
            a.push(*t, Priority::Info, 0.01 * i as f64);
        }
        assert_eq!(said(&mut a, 0.0, 1.0), ["Restarted. Three. Four"]);
    }

    #[test]
    fn errors_come_first_and_are_never_dropped() {
        let mut a = Announcer::default();
        a.push("Nudge", Priority::Info, 0.0);
        a.push("Traceback: one", Priority::Error, 0.0);
        a.push("Traceback: two", Priority::Error, 0.0);
        assert_eq!(a.tick(0.0), "Traceback: one", "errors don't wait to gather");
        assert_eq!(
            said(&mut a, 0.01, 3.0 * GAP + 0.1),
            ["Traceback: two", "Nudge"]
        );
    }

    #[test]
    fn errors_past_the_cap_are_counted() {
        let mut a = Announcer::default();
        for i in 0..MAX_ERRORS + 5 {
            a.push(format!("E{i}"), Priority::Error, 0.0);
        }
        let all = said(&mut a, 0.0, GAP * (MAX_ERRORS as f64 + 1.0));
        assert_eq!(all.len(), MAX_ERRORS);
        assert_eq!(all.last().unwrap(), "E31. 5 more errors in the log");
    }

    #[test]
    fn the_same_message_again_blanks_one_frame() {
        let mut a = Announcer::default();
        a.push("Nudge", Priority::Info, 0.0);
        assert_eq!(a.tick(GATHER), "Nudge");
        a.push("Nudge", Priority::Info, 1.0);
        assert_eq!(a.tick(GATHER + GAP), "", "blank so it reads as a change");
        assert_eq!(a.tick(GATHER + GAP + 0.01), "Nudge");
    }

    #[test]
    fn nothing_pending_keeps_the_last_message() {
        let mut a = Announcer::default();
        assert_eq!(a.tick(0.0), "");
        a.push("", Priority::Info, 0.0);
        assert_eq!(a.tick(1.0), "", "empty text is ignored");
        a.push("Idle trigger", Priority::Info, 1.0);
        assert_eq!(a.tick(1.3), "Idle trigger");
        assert_eq!(a.tick(10.0), "Idle trigger");
    }

    #[test]
    fn behaviour_is_said_once_it_holds() {
        let mut w = BehaviourWatch::default();
        assert_eq!(w.update(0.0, "breathing"), None);
        assert_eq!(w.update(0.2, "breathing"), None);
        assert_eq!(w.update(0.3, "breathing").as_deref(), Some("Now breathing"));
        assert_eq!(w.update(0.5, "breathing"), None, "said once");
        // A link passed through for a frame is not read out.
        assert_eq!(w.update(0.6, "nose_boops"), None);
        assert_eq!(w.update(0.65, "breathing"), None);
        assert_eq!(w.update(1.0, "angry_animation"), None);
        assert_eq!(
            w.update(1.3, "angry_animation").as_deref(),
            Some("Now angry animation")
        );
    }

    #[test]
    fn no_behaviour_is_not_said_and_restart_says_it_again() {
        let mut w = BehaviourWatch::default();
        assert_eq!(w.update(0.0, NO_BEHAVIOUR), None);
        assert_eq!(w.update(1.0, NO_BEHAVIOUR), None);
        w.update(1.0, "breathing");
        assert!(w.update(2.0, "breathing").is_some());
        w.reset();
        w.update(3.0, "breathing");
        assert_eq!(w.update(3.5, "breathing").as_deref(), Some("Now breathing"));
    }

    #[test]
    fn markers_read_as_words() {
        let t = marker_text;
        assert_eq!(
            t(MarkerKind::Motion, "boop 1.5 g"),
            ("Boop 1.5 g".into(), Priority::Info)
        );
        assert_eq!(t(MarkerKind::Sound, "noseBoops.wav").0, "Sound: noseBoops");
        assert_eq!(t(MarkerKind::Sound, "odd").0, "Sound: odd");
        assert_eq!(t(MarkerKind::Idle, "idle").0, "Idle trigger");
        assert_eq!(
            t(MarkerKind::Restart, "restart"),
            ("Restarted".into(), Priority::Important)
        );
        assert_eq!(
            t(MarkerKind::Restart, "scenario boop").0,
            "Scenario boop started"
        );
    }
}
