//! One-click deploy of the firmware to a real CIRCUITPY drive.
//!
//! The deploy set is hard-coded: top-level `*.py` and `settings.toml`,
//! `behaviours/**` and `sounds/*.wav`. Nothing else is ever written, so the
//! board's `lib/`, `boot_out.txt`, `sd/` and logs cannot be touched.
//! Everything but the drive lookup is plain std so `rustc --test` covers it.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// One file to write to the board.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Copy {
    /// '/'-separated path relative to both the firmware dir and the drive.
    pub rel: String,
    /// Not on the board yet (otherwise its contents differ).
    pub new: bool,
}

pub const READ_ONLY_MSG: &str = "CIRCUITPY is read-only: the board has it (log mode is on). \
Turn it off in the serial console (Ctrl-C, then import log_off) and reset the board.";

/// Files that would change on `drive`, in copy order (code.py last).
pub fn plan(fw_dir: &Path, drive: &Path) -> io::Result<Vec<Copy>> {
    let mut rels = Vec::new();
    for entry in fs::read_dir(fw_dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if is_file(&entry.path()) && (name == "settings.toml" || name.ends_with(".py")) {
            rels.push(name);
        }
    }
    collect(&fw_dir.join("behaviours"), "behaviours", &mut rels)?;
    collect_sounds(&fw_dir.join("sounds"), &mut rels)?;

    // code.py last: CircuitPython reloads on every write, so the final reload sees all files.
    rels.sort_by(|a, b| {
        (a == "code.py")
            .cmp(&(b == "code.py"))
            .then_with(|| a.cmp(b))
    });
    rels.dedup();

    let mut out = Vec::new();
    for rel in rels {
        let src = fw_dir.join(&rel);
        let dst = drive.join(&rel);
        match fs::metadata(&dst) {
            Ok(m) if m.is_file() => {
                if !same_bytes(&src, &dst, m.len())? {
                    out.push(Copy { rel, new: false });
                }
            }
            _ => out.push(Copy { rel, new: true }),
        }
    }
    Ok(out)
}

/// A real file, not a symlink: the plan must not follow links out of fw_dir.
fn is_file(p: &Path) -> bool {
    fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_file())
}

fn is_dir(p: &Path) -> bool {
    fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_dir())
}

/// Recursive walk of `dir`, skipping `__pycache__`, `*.pyc` and symlinks.
fn collect(dir: &Path, prefix: &str, out: &mut Vec<String>) -> io::Result<()> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        let rel = format!("{prefix}/{name}");
        if is_dir(&path) {
            if name != "__pycache__" {
                collect(&path, &rel, out)?;
            }
        } else if is_file(&path) && !name.ends_with(".pyc") {
            out.push(rel);
        }
    }
    Ok(())
}

/// `sounds/*.wav` only, without macOS `._` resource-fork files.
fn collect_sounds(dir: &Path, out: &mut Vec<String>) -> io::Result<()> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if is_file(&entry.path())
            && !name.starts_with("._")
            && name.to_ascii_lowercase().ends_with(".wav")
        {
            out.push(format!("sounds/{name}"));
        }
    }
    Ok(())
}

/// FAT mtimes are 2 s resolution and drift with timezones, so compare content;
/// length first because it is free.
fn same_bytes(src: &Path, dst: &Path, dst_len: u64) -> io::Result<bool> {
    if fs::metadata(src)?.len() != dst_len {
        return Ok(false);
    }
    Ok(fs::read(src)? == fs::read(dst)?)
}

/// Copies `plan` in order, calling `report` for each file (Ok = its path) and
/// stopping at the first error, which is also reported.
pub fn copy_all(
    fw_dir: &Path,
    drive: &Path,
    plan: &[Copy],
    mut report: impl FnMut(Result<&str, String>),
) {
    for item in plan {
        match copy_one(fw_dir, drive, &item.rel) {
            Ok(()) => report(Ok(&item.rel)),
            Err(e) => {
                report(Err(describe(&item.rel, &e)));
                return;
            }
        }
    }
}

fn copy_one(fw_dir: &Path, drive: &Path, rel: &str) -> io::Result<()> {
    let dst = drive.join(rel);
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(fw_dir.join(rel), dst).map(|_| ())
}

fn describe(rel: &str, e: &io::Error) -> String {
    // 19 = ERROR_WRITE_PROTECT, which Windows reports for a read-only volume.
    // Plain "access denied" can mean other things (a read-only file attribute,
    // a file held open), so it keeps the OS wording rather than blaming log mode.
    let read_only = e.raw_os_error() == Some(19) || e.kind() == io::ErrorKind::ReadOnlyFilesystem;
    if read_only {
        READ_ONLY_MSG.to_string()
    } else {
        format!("cannot copy {rel}: {e}")
    }
}

/// The drive root of the volume labelled CIRCUITPY, if one is plugged in.
#[cfg(windows)]
pub fn find_circuitpy() -> Option<PathBuf> {
    use windows_sys::Win32::Storage::FileSystem::{GetLogicalDrives, GetVolumeInformationW};
    use windows_sys::Win32::System::Diagnostics::Debug::{
        SetThreadErrorMode, SEM_FAILCRITICALERRORS,
    };

    let mut old_mode = 0;
    // SAFETY: plain Win32 calls; `old_mode` is a valid out pointer.
    // FAILCRITICALERRORS stops an empty card reader popping an "insert disk" dialog.
    unsafe { SetThreadErrorMode(SEM_FAILCRITICALERRORS, &mut old_mode) };
    let found = (|| {
        // SAFETY: no arguments.
        let mask = unsafe { GetLogicalDrives() };
        for i in 0..26u32 {
            if mask & (1 << i) == 0 {
                continue;
            }
            let letter = char::from(b'A' + i as u8);
            let root: Vec<u16> = format!("{letter}:\\").encode_utf16().chain([0]).collect();
            let mut label = [0u16; 261];
            // SAFETY: `root` is NUL-terminated and `label` is writable for the
            // length passed; the unused out parameters are allowed to be null.
            let ok = unsafe {
                GetVolumeInformationW(
                    root.as_ptr(),
                    label.as_mut_ptr(),
                    label.len() as u32,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    0,
                )
            };
            if ok == 0 {
                continue;
            }
            let end = label.iter().position(|&c| c == 0).unwrap_or(label.len());
            if String::from_utf16_lossy(&label[..end]).eq_ignore_ascii_case("CIRCUITPY") {
                return Some(PathBuf::from(format!("{letter}:\\")));
            }
        }
        None
    })();
    // SAFETY: restores the mode read above; null previous-mode is allowed.
    unsafe { SetThreadErrorMode(old_mode, std::ptr::null_mut()) };
    found
}

#[cfg(not(windows))]
pub fn find_circuitpy() -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static N: AtomicU32 = AtomicU32::new(0);

    struct Tmp(PathBuf);
    impl Tmp {
        fn new() -> Self {
            let n = N.fetch_add(1, Ordering::SeqCst);
            let p = std::env::temp_dir().join(format!("ddl0-deploy-{}-{n}", std::process::id()));
            let _ = fs::remove_dir_all(&p);
            fs::create_dir_all(p.join("fw")).unwrap();
            fs::create_dir_all(p.join("drive")).unwrap();
            Tmp(p)
        }
        fn fw(&self) -> PathBuf {
            self.0.join("fw")
        }
        fn drive(&self) -> PathBuf {
            self.0.join("drive")
        }
    }
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn put(root: &Path, rel: &str, body: &str) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, body).unwrap();
    }

    fn rels(p: &[Copy]) -> Vec<&str> {
        p.iter().map(|c| c.rel.as_str()).collect()
    }

    #[test]
    fn never_plans_protected_paths() {
        let t = Tmp::new();
        for rel in [
            "lib/x.mpy",
            "boot_out.txt",
            "sd/placeholder.txt",
            "log.txt",
            "log.old.txt",
            "log_mode",
            "behaviours/__pycache__/a.pyc",
            "behaviours/a.pyc",
            "sounds/._boop.wav",
            "sounds/notes.txt",
        ] {
            put(&t.fw(), rel, "x");
        }
        put(&t.fw(), "config.py", "1");
        put(&t.fw(), "behaviours/a.py", "1");
        put(&t.fw(), "sounds/boop.wav", "1");
        let plan = plan(&t.fw(), &t.drive()).unwrap();
        assert_eq!(
            rels(&plan),
            ["behaviours/a.py", "config.py", "sounds/boop.wav"]
        );
    }

    #[test]
    fn skips_unchanged_and_flags_new() {
        let t = Tmp::new();
        put(&t.fw(), "config.py", "same");
        put(&t.drive(), "config.py", "same");
        put(&t.fw(), "hardware.py", "new text");
        put(&t.drive(), "hardware.py", "old text");
        put(&t.fw(), "log.py", "x");
        put(&t.fw(), "settings.toml", "a=1");
        put(&t.drive(), "settings.toml", "a=2");
        put(&t.drive(), "only_on_board.py", "keep");
        let plan = plan(&t.fw(), &t.drive()).unwrap();
        assert_eq!(
            plan,
            [
                Copy {
                    rel: "hardware.py".into(),
                    new: false
                },
                Copy {
                    rel: "log.py".into(),
                    new: true
                },
                Copy {
                    rel: "settings.toml".into(),
                    new: false
                },
            ]
        );
    }

    #[test]
    fn code_py_is_last() {
        let t = Tmp::new();
        for rel in ["code.py", "zeta.py", "behaviours/b.py", "sounds/a.wav"] {
            put(&t.fw(), rel, "x");
        }
        let plan = plan(&t.fw(), &t.drive()).unwrap();
        assert_eq!(plan.last().unwrap().rel, "code.py");
        assert_eq!(plan.len(), 4);
    }

    #[test]
    fn copy_all_copies_then_plan_is_empty() {
        let t = Tmp::new();
        put(&t.fw(), "code.py", "c");
        put(&t.fw(), "behaviours/deep/a.py", "a");
        put(&t.fw(), "lib/skip.mpy", "l");
        let p = plan(&t.fw(), &t.drive()).unwrap();
        let mut done = Vec::new();
        copy_all(&t.fw(), &t.drive(), &p, |r| done.push(r.map(str::to_owned)));
        assert_eq!(
            done,
            [
                Ok("behaviours/deep/a.py".to_string()),
                Ok("code.py".to_string())
            ]
        );
        assert_eq!(
            fs::read_to_string(t.drive().join("behaviours/deep/a.py")).unwrap(),
            "a"
        );
        assert!(!t.drive().join("lib").exists());
        assert!(plan(&t.fw(), &t.drive()).unwrap().is_empty());
    }

    #[test]
    fn copy_all_stops_at_first_error() {
        let t = Tmp::new();
        put(&t.fw(), "a.py", "a");
        put(&t.fw(), "b.py", "b");
        // A directory where a file belongs makes the first copy fail.
        fs::create_dir_all(t.drive().join("a.py")).unwrap();
        let p = vec![
            Copy {
                rel: "a.py".into(),
                new: true,
            },
            Copy {
                rel: "b.py".into(),
                new: true,
            },
        ];
        let mut results = Vec::new();
        copy_all(&t.fw(), &t.drive(), &p, |r| results.push(r.is_ok()));
        assert_eq!(results, [false]);
        assert!(!t.drive().join("b.py").exists());
    }
}
