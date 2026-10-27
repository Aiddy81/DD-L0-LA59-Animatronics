//! The tunables `config.py` reads from `settings.toml`, for the Settings
//! panel: which keys exist (found in config.py, so a new tunable shows up with
//! no simulator change), their defaults and help, and line-based edits that
//! leave the rest of settings.toml (comments, order, other keys) untouched.
//!
//! Only std, so the tests run with plain `rustc --test`.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::time::SystemTime;

/// Key prefixes that hold the left-wing calibration. They match the physical
/// build, so the panel shows them but never writes them.
const CALIBRATION_PREFIXES: [&str; 2] = ["SERVO2_", "BREATH_SERVO2_"];

/// One `_int("KEY", default)` in config.py.
#[derive(Debug, Clone, PartialEq)]
pub struct Tunable {
    pub key: String,
    /// None when the default isn't a plain integer or a product of them.
    pub default: Option<i64>,
    /// The comment block above the key (or above its group) in config.py.
    pub help: String,
    pub read_only: bool,
}

/// Every `_int("KEY", default)` in config.py, in file order.
pub fn discover(config_py: &str) -> Vec<Tunable> {
    let lines: Vec<&str> = config_py.lines().collect();
    let mut found = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if line.trim_start().starts_with('#') {
            continue;
        }
        let Some((key, default)) = int_call(line) else {
            continue;
        };
        if found.iter().any(|t: &Tunable| t.key == key) {
            continue;
        }
        found.push(Tunable {
            read_only: CALIBRATION_PREFIXES.iter().any(|p| key.starts_with(p)),
            help: help_above(&lines, i),
            key,
            default,
        });
    }
    found
}

/// `("KEY", default)` from a line containing `_int("KEY", <default>)`.
fn int_call(line: &str) -> Option<(String, Option<i64>)> {
    let rest = &line[line.find("_int(\"")? + 6..];
    let (key, rest) = rest.split_once('"')?;
    let args = rest.trim_start().strip_prefix(',')?;
    let default = args.split_once(')').and_then(|(expr, _)| product(expr));
    let valid = !key.is_empty() && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
    valid.then(|| (key.to_owned(), default))
}

/// `15 * 60` -> 900. Anything else (names, other operators) -> None.
fn product(expr: &str) -> Option<i64> {
    expr.split('*')
        .try_fold(1i64, |acc, part| acc.checked_mul(parse_int(part)?))
}

/// The `#` lines directly above line `i`, skipping sibling `_int(` lines so
/// every key of a group gets the group's comment.
fn help_above(lines: &[&str], i: usize) -> String {
    let mut j = i;
    while j > 0 && int_call(lines[j - 1]).is_some() {
        j -= 1;
    }
    let mut help = Vec::new();
    while j > 0 {
        let Some(text) = lines[j - 1].trim().strip_prefix('#') else {
            break;
        };
        help.push(text.trim());
        j -= 1;
    }
    help.reverse();
    help.join(" ")
}

/// An integer the way CircuitPython's settings.toml (and the simulator's
/// os.getenv) accepts one: optional sign, decimal or 0x/0o/0b, underscores.
pub fn parse_int(text: &str) -> Option<i64> {
    let text = text.trim();
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let lower = digits.to_ascii_lowercase();
    let (radix, digits) = [("0x", 16), ("0o", 8), ("0b", 2)]
        .iter()
        .find_map(|(p, r)| lower.strip_prefix(p).map(|d| (*r, d)))
        .unwrap_or((10, lower.as_str()));
    let digits = digits.replace('_', "");
    if digits.is_empty() || digits.starts_with(['+', '-']) {
        return None;
    }
    let value = i64::from_str_radix(&digits, radix).ok()?;
    Some(if negative { -value } else { value })
}

/// Where a `KEY = value` line's value starts and ends (byte offsets), and its
/// key. None for blank lines, comments and lines without `=`.
fn assignment(line: &str) -> Option<(&str, usize, usize)> {
    let (name, _) = line.split_once('=')?;
    let key = name.trim();
    if key.is_empty() || key.starts_with('#') {
        return None;
    }
    let after = name.len() + 1;
    let start = after + (line[after..].len() - line[after..].trim_start().len());
    let value = &line[start..];
    let len = if value.starts_with('"') {
        quoted_len(value).unwrap_or(value.len())
    } else {
        value.split('#').next().unwrap_or("").trim_end().len()
    };
    Some((key, start, start + len))
}

/// Length of a leading `"..."` including both quotes, honouring `\` escapes.
fn quoted_len(value: &str) -> Option<usize> {
    let mut escaped = false;
    for (i, c) in value.char_indices().skip(1) {
        match c {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '"' => return Some(i + 1),
            _ => {}
        }
    }
    None
}

/// Top-level `KEY = value` text in settings.toml (comment stripped), first
/// line wins, stopping at the first `[table]`: the rules os.getenv uses.
pub fn read_values(toml: &str) -> HashMap<String, String> {
    let mut values = HashMap::new();
    for line in toml.lines() {
        if line.trim_start().starts_with('[') {
            break;
        }
        if let Some((key, start, end)) = assignment(line) {
            values
                .entry(key.to_owned())
                .or_insert_with(|| line[start..end].to_owned());
        }
    }
    values
}

/// settings.toml with each `(key, value)` written in: the value of the key's
/// line is replaced (spacing and trailing comment kept); a key with no line
/// gets `KEY = value` added at the end of the top-level section. Every other
/// byte is unchanged.
pub fn apply_edits(toml: &str, edits: &[(&str, i64)]) -> String {
    let newline = if toml.contains("\r\n") { "\r\n" } else { "\n" };
    let mut out = String::with_capacity(toml.len() + 32 * edits.len());
    let mut done: Vec<&str> = Vec::new();
    let mut in_top = true;
    let append_missing = |out: &mut String, done: &[&str]| {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push_str(newline);
        }
        for (key, value) in edits.iter().filter(|(k, _)| !done.contains(k)) {
            out.push_str(&format!("{key} = {value}{newline}"));
        }
    };
    for line in toml.split_inclusive('\n') {
        if in_top && line.trim_start().starts_with('[') {
            append_missing(&mut out, &done);
            in_top = false;
        }
        let edit = in_top
            .then(|| assignment(line))
            .flatten()
            .and_then(|(key, start, end)| {
                let (k, v) = edits.iter().find(|(k, _)| *k == key)?;
                (!done.contains(k)).then_some((*k, *v, start, end))
            });
        match edit {
            Some((key, value, start, end)) => {
                out.push_str(&line[..start]);
                out.push_str(&value.to_string());
                out.push_str(&line[end..]);
                done.push(key);
            }
            None => out.push_str(line),
        }
    }
    if in_top {
        append_missing(&mut out, &done);
    }
    out
}

/// What the Settings panel shows and edits.
#[derive(Default)]
pub struct SettingsState {
    pub tunables: Vec<Tunable>,
    /// Raw value text of each key present in settings.toml.
    pub file: HashMap<String, String>,
    /// Unsaved values typed into the panel.
    pub edits: BTreeMap<String, i64>,
    /// Why config.py or settings.toml couldn't be read, if so.
    pub error: Option<String>,
    mtimes: Option<[Option<SystemTime>; 2]>,
}

fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// A file's text; a missing file reads as empty.
fn read_or_empty(path: &Path) -> std::io::Result<String> {
    match std::fs::read_to_string(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        other => other,
    }
}

impl SettingsState {
    /// Re-reads config.py and settings.toml in `dir` when either changed on
    /// disk, unless there are unsaved edits (those would be overwritten).
    pub fn refresh(&mut self, dir: &Path) {
        let now = [
            mtime(&dir.join("config.py")),
            mtime(&dir.join("settings.toml")),
        ];
        if self.mtimes == Some(now) || !self.edits.is_empty() {
            return;
        }
        self.mtimes = Some(now);
        self.load(dir);
    }

    fn load(&mut self, dir: &Path) {
        self.error = None;
        match std::fs::read_to_string(dir.join("config.py")) {
            Ok(text) => self.tunables = discover(&text),
            Err(e) => {
                self.tunables.clear();
                self.error = Some(format!("could not read config.py: {e}"));
            }
        }
        match read_or_empty(&dir.join("settings.toml")) {
            Ok(text) => self.file = read_values(&text),
            Err(e) => {
                self.file.clear();
                self.error = Some(format!("could not read settings.toml: {e}"));
            }
        }
    }

    /// The value settings.toml holds for `t`, or its default when the key is
    /// missing or not a whole number (as config.py does).
    pub fn saved(&self, t: &Tunable) -> Option<i64> {
        self.file
            .get(&t.key)
            .and_then(|v| parse_int(v))
            .or(t.default)
    }

    /// `saved` for the tunable called `key`; None when config.py has no such key.
    pub fn saved_value(&self, key: &str) -> Option<i64> {
        self.tunables
            .iter()
            .find(|t| t.key == key)
            .and_then(|t| self.saved(t))
    }

    /// The value text in settings.toml when it isn't a whole number, so
    /// config.py ignores it.
    pub fn invalid(&self, t: &Tunable) -> Option<&str> {
        self.file
            .get(&t.key)
            .filter(|v| parse_int(v).is_none())
            .map(String::as_str)
    }

    /// Records `value` for `t`; setting it back to what's saved drops the edit.
    pub fn set(&mut self, t: &Tunable, value: i64) {
        if t.read_only {
            return;
        }
        let unchanged = Some(value) == self.saved(t) && self.invalid(t).is_none();
        if unchanged {
            self.edits.remove(&t.key);
        } else {
            self.edits.insert(t.key.clone(), value);
        }
    }

    /// Drops the unsaved edits and re-reads both files.
    pub fn revert(&mut self, dir: &Path) {
        self.edits.clear();
        self.mtimes = None;
        self.refresh(dir);
    }

    /// Writes the edits into settings.toml in `dir` and re-reads it. Returns
    /// how many keys were written.
    pub fn save(&mut self, dir: &Path) -> std::io::Result<usize> {
        let path = dir.join("settings.toml");
        let text = read_or_empty(&path)?;
        let editable = |k: &String| self.tunables.iter().any(|t| &t.key == k && !t.read_only);
        let edits: Vec<(&str, i64)> = self
            .edits
            .iter()
            .filter(|(k, _)| editable(k))
            .map(|(k, v)| (k.as_str(), *v))
            .collect();
        let count = edits.len();
        let updated = apply_edits(&text, &edits);
        if updated != text {
            // Renamed into place so the reload never sees a half-written file.
            let tmp = dir.join("settings.toml.tmp");
            std::fs::write(&tmp, updated)?;
            std::fs::rename(&tmp, &path)?;
        }
        self.revert(dir);
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = "import os\r\n\
\r\n\
def _int(key, default):\r\n\
    value = os.getenv(key)\r\n\
\r\n\
# Idle: seek attention after a random quiet spell.\r\n\
IDLE_MIN_S = _int(\"IDLE_MIN_S\", 15 * 60)\r\n\
IDLE_MAX_S = _int(\"IDLE_MAX_S\", 120 * 60)\r\n\
# Trench run length.\r\n\
TRENCH_RUN_S = _int(\"TRENCH_RUN_S\", 150)\r\n\
\r\n\
# Motion thresholds;\r\n\
# in milli-g.\r\n\
NUDGE_Z_G = _int(\"NUDGE_Z_MG\", 250) / 1000\r\n\
ODD = _int(\"ODD\", SOME_NAME)\r\n\
# Left wing calibration.\r\n\
SERVO2_MIN = _int(\"SERVO2_MIN\", 40)\r\n\
BREATH_SERVO2_MAX = _int(\"BREATH_SERVO2_MAX\", 50)\r\n";

    #[test]
    fn discovers_keys_defaults_and_help() {
        let found = discover(CONFIG);
        let keys: Vec<&str> = found.iter().map(|t| t.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "IDLE_MIN_S",
                "IDLE_MAX_S",
                "TRENCH_RUN_S",
                "NUDGE_Z_MG",
                "ODD",
                "SERVO2_MIN",
                "BREATH_SERVO2_MAX"
            ]
        );
        assert_eq!(found[0].default, Some(900));
        assert_eq!(found[1].default, Some(7200));
        assert_eq!(found[3].default, Some(250));
        assert_eq!(found[4].default, None);
        assert_eq!(
            found[1].help,
            "Idle: seek attention after a random quiet spell."
        );
        assert_eq!(found[2].help, "Trench run length.");
        assert_eq!(found[3].help, "Motion thresholds; in milli-g.");
        assert!(!found[0].read_only);
        assert!(found[5].read_only && found[6].read_only);
    }

    #[test]
    fn discovers_the_real_config() {
        let text = include_str!("../../PropMaker_Feather/config.py");
        let found = discover(text);
        assert!(found.iter().all(|t| t.default.is_some()));
        assert_eq!(found.iter().filter(|t| t.read_only).count(), 4);
        let idle = found.iter().find(|t| t.key == "IDLE_MAX_S").unwrap();
        assert_eq!(idle.default, Some(7200));
    }

    #[test]
    fn parses_ints_like_circuitpython() {
        assert_eq!(parse_int(" 42 "), Some(42));
        assert_eq!(parse_int("-7"), Some(-7));
        assert_eq!(parse_int("0x1F"), Some(31));
        assert_eq!(parse_int("1_000"), Some(1000));
        assert_eq!(parse_int("1.5"), None);
        assert_eq!(parse_int("\"60\""), None);
        assert_eq!(parse_int(""), None);
        assert_eq!(parse_int("--1"), None);
    }

    #[test]
    fn reads_top_level_values() {
        let toml = "# c\nA = 1  # one\nB=\"x # y\"\nA = 2\nC = 1.5\n[t]\nD = 4\n";
        let v = read_values(toml);
        assert_eq!(v["A"], "1");
        assert_eq!(v["B"], "\"x # y\"");
        assert_eq!(v["C"], "1.5");
        assert!(!v.contains_key("D"));
    }

    #[test]
    fn edits_keep_comments_order_and_line_endings() {
        let toml = "# head\r\nA = 1\r\n\r\n# about B\r\n  B=20   # twenty\r\nSECRET = \"x\"\r\n";
        let out = apply_edits(toml, &[("B", 25), ("A", 3)]);
        assert_eq!(
            out,
            "# head\r\nA = 3\r\n\r\n# about B\r\n  B=25   # twenty\r\nSECRET = \"x\"\r\n"
        );
    }

    #[test]
    fn missing_keys_are_appended_before_tables() {
        assert_eq!(apply_edits("A = 1", &[("B", 2)]), "A = 1\nB = 2\n");
        assert_eq!(apply_edits("", &[("B", 2)]), "B = 2\n");
        assert_eq!(
            apply_edits("A = 1\n[t]\nB = 9\n", &[("B", 2)]),
            "A = 1\nB = 2\n[t]\nB = 9\n"
        );
    }

    #[test]
    fn only_the_first_line_of_a_key_changes() {
        assert_eq!(apply_edits("A = 1\nA = 2\n", &[("A", 5)]), "A = 5\nA = 2\n");
    }

    #[test]
    fn replaces_a_quoted_or_invalid_value() {
        assert_eq!(
            apply_edits("A = \"6\" # c\nB = 1.5\n", &[("A", 6), ("B", 2)]),
            "A = 6 # c\nB = 2\n"
        );
    }

    #[test]
    fn no_edits_leaves_the_file_identical() {
        let text = include_str!("../../PropMaker_Feather/settings.toml");
        assert_eq!(apply_edits(text, &[]), text);
    }

    #[test]
    fn set_and_save_round_trip() {
        let dir = std::env::temp_dir().join(format!("ddl0-settings-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.py"), CONFIG).unwrap();
        std::fs::write(
            dir.join("settings.toml"),
            "# keep\nTRENCH_RUN_S = 150\nSERVO2_MIN = 40\n",
        )
        .unwrap();
        let mut s = SettingsState::default();
        s.refresh(&dir);
        let trench = s.tunables[2].clone();
        let servo = s.tunables[5].clone();
        let idle = s.tunables[0].clone();
        s.set(&trench, 150);
        assert!(s.edits.is_empty(), "same as saved is not an edit");
        s.set(&trench, 30);
        s.set(&servo, 99);
        s.set(&idle, 60);
        assert_eq!(s.edits.len(), 2, "calibration keys are never edited");
        assert_eq!(s.save(&dir).unwrap(), 2);
        let text = std::fs::read_to_string(dir.join("settings.toml")).unwrap();
        assert_eq!(
            text,
            "# keep\nTRENCH_RUN_S = 30\nSERVO2_MIN = 40\nIDLE_MIN_S = 60\n"
        );
        assert_eq!(s.saved(&trench), Some(30));
        assert!(s.edits.is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
