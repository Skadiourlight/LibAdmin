//! Thin wrapper over the bundled DjVuLibre command line tools (djvused / ddjvu).
use crate::config::djvu_tools_dir;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::LazyLock;

fn tool(name: &str) -> PathBuf {
    djvu_tools_dir().join(format!("{name}.exe"))
}

pub fn available() -> bool {
    tool("djvused").exists() && tool("ddjvu").exists()
}

fn run(args: &[&std::ffi::OsStr], exe: &str) -> Option<Output> {
    let mut cmd = Command::new(tool(exe));
    cmd.args(args).current_dir(djvu_tools_dir());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd.output().ok()
}

static META: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r#"(?m)^(\w+)\s+"((?:[^"\\]|\\.)*)""#).unwrap());

/// Returns (pages, title, author).
pub fn read_info(path: &Path) -> (Option<i64>, String, String) {
    if !available() {
        return (None, String::new(), String::new());
    }
    let Some(out) = run(&["-u".as_ref(), "-e".as_ref(), "n; print-meta".as_ref(), path.as_os_str()], "djvused") else {
        return (None, String::new(), String::new());
    };
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let pages = text.lines().next().and_then(|l| l.trim().parse().ok());
    let (mut title, mut author) = (String::new(), String::new());
    for c in META.captures_iter(&text) {
        match &c[1] {
            "title" => title = c[2].to_string(),
            "author" => author = c[2].to_string(),
            _ => {}
        }
    }
    (pages, title, author)
}

pub fn render_first_page(path: &Path, out_ppm: &Path, width: u32) -> bool {
    if !available() {
        return false;
    }
    let size = format!("-size={}x{}", width, width * 2);
    let ok = run(
        &["-format=ppm".as_ref(), "-page=1".as_ref(), size.as_ref(), path.as_os_str(), out_ppm.as_os_str()],
        "ddjvu",
    )
    .is_some_and(|o| o.status.success());
    ok && out_ppm.exists()
}
