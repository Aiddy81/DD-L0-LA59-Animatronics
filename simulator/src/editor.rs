//! Click-to-open for traceback lines in the log panel.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The `(path, line)` of a `File "...", line N` traceback entry, if `text` has one.
fn parse_location(text: &str) -> Option<(&str, u32)> {
    let rest = &text[text.find("File \"")? + 6..];
    let (path, rest) = rest.split_once('"')?;
    let digits = rest.strip_prefix(", line ")?;
    let end = digits
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(digits.len());
    let line = digits[..end].parse().ok()?;
    (!path.is_empty()).then_some((path, line))
}

/// canonicalize() gives `\\?\C:\...` on Windows; plain drive paths are what
/// editors and explorer expect.
fn plain(path: PathBuf) -> PathBuf {
    match path.to_str().and_then(|s| s.strip_prefix(r"\\?\")) {
        Some(rest) if !rest.starts_with("UNC") => PathBuf::from(rest),
        _ => path,
    }
}

/// A traceback location whose file really is inside the firmware folder.
/// Touches the disk, so callers should cache the result.
pub fn traceback_location(text: &str, fw_dir: &Path) -> Option<(PathBuf, u32)> {
    let (path, line) = parse_location(text)?;
    let file = plain(std::fs::canonicalize(path).ok()?);
    let root = plain(std::fs::canonicalize(fw_dir).ok()?);
    // Log text is untrusted (firmware can print anything), so never open
    // files outside the firmware folder.
    (file.is_file() && file.starts_with(&root)).then_some((file, line))
}

/// Split on whitespace; double quotes group words and are dropped.
fn split_args(command: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut cur = String::new();
    let (mut quoted, mut started) = (false, false);
    for c in command.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    args.push(std::mem::take(&mut cur));
                    started = false;
                }
            }
            c => {
                cur.push(c);
                started = true;
            }
        }
    }
    if started {
        args.push(cur);
    }
    args
}

/// Runs the configured editor command directly (never via a shell, so file
/// names can't inject anything); falls back to revealing the file.
pub fn open_in_editor(command: &str, file: &Path, line: u32) -> Result<(), String> {
    let file_s = file.to_string_lossy();
    let line_s = line.to_string();
    let args: Vec<String> = split_args(command)
        .into_iter()
        .map(|a| a.replace("{file}", &file_s).replace("{line}", &line_s))
        .collect();
    let Some((prog, rest)) = args.split_first() else {
        return reveal(file);
    };
    match spawn(prog, rest) {
        Ok(()) => Ok(()),
        Err(e) => match reveal(file) {
            Ok(()) => Err(format!(
                "editor command failed ({e}); revealed the file instead"
            )),
            Err(r) => Err(format!("editor command failed ({e}); {r}")),
        },
    }
}

fn spawn(prog: &str, rest: &[String]) -> Result<(), String> {
    let mut result = Command::new(prog).args(rest).spawn();
    // VS Code's launcher is code.cmd, which CreateProcess won't find as "code".
    if cfg!(windows)
        && Path::new(prog).extension().is_none()
        && matches!(&result, Err(e) if e.kind() == ErrorKind::NotFound)
    {
        result = Command::new(format!("{prog}.cmd")).args(rest).spawn();
    }
    // A GUI launcher: not waited on.
    result.map(drop).map_err(|e| format!("{prog}: {e}"))
}

/// Show the file in the system file manager. Never opens the .py itself:
/// the default handler for it could run it.
fn reveal(file: &Path) -> Result<(), String> {
    let mut cmd;
    if cfg!(windows) {
        cmd = Command::new("explorer");
        // explorer exits nonzero even on success, so only the spawn is checked.
        cmd.arg(format!("/select,{}", file.display()));
    } else if cfg!(target_os = "macos") {
        cmd = Command::new("open");
        cmd.arg("-R").arg(file);
    } else {
        cmd = Command::new("xdg-open");
        cmd.arg(file.parent().unwrap_or(file));
    }
    cmd.spawn()
        .map(drop)
        .map_err(|e| format!("could not reveal {}: {e}", file.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_whitespace_and_quotes() {
        assert_eq!(
            split_args(r#"code -g "{file}:{line}" --reuse-window"#),
            ["code", "-g", "{file}:{line}", "--reuse-window"]
        );
        assert_eq!(
            split_args(r#""C:\Program Files\Code\code.exe"  -g x"#),
            [r"C:\Program Files\Code\code.exe", "-g", "x"]
        );
        assert_eq!(split_args(r#"a "" b"#), ["a", "", "b"]);
        assert!(split_args("   ").is_empty());
    }

    #[test]
    fn parses_traceback_lines() {
        assert_eq!(
            parse_location(r#"  File "C:\fw\behaviours\motion.py", line 42, in nose_boops"#),
            Some((r"C:\fw\behaviours\motion.py", 42))
        );
        assert_eq!(
            parse_location(r#"File "/a/b.py", line 7"#),
            Some(("/a/b.py", 7))
        );
    }

    #[test]
    fn rejects_non_tracebacks() {
        assert_eq!(parse_location("nothing here"), None);
        assert_eq!(parse_location(r#"File "x.py", line abc"#), None);
        assert_eq!(parse_location(r#"File "x.py" line 3"#), None);
        assert_eq!(parse_location(r#"File "", line 3"#), None);
        assert_eq!(parse_location(r#"File "unterminated, line 3"#), None);
    }
}
