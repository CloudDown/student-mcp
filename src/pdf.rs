//! Shared, on-demand PDF operations. No OCR or renderer runs for file delivery.
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

pub(crate) struct Workspace(PathBuf);

impl Workspace {
    pub fn new() -> Option<Self> {
        let root = std::env::temp_dir().join("student-mcp");
        private_dir(&root)?;
        let path = root.join(crate::config::random_hex(8));
        private_dir(&path)?;
        Some(Self(path))
    }
}

impl std::ops::Deref for Workspace {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for Workspace {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn private_dir(path: &Path) -> Option<()> {
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    if builder.create(path).is_err() && !path.is_dir() {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).ok()?;
    }
    Some(())
}

/// Capture stdout in a private file rather than a pipe that can deadlock when
/// full. Kill and reap a stalled tool; never leave a PDF worker waiting forever.
pub(crate) fn output(command: &mut Command) -> io::Result<Output> {
    output_before(command, Duration::from_secs(30))
}

fn output_before(command: &mut Command, timeout: Duration) -> io::Result<Output> {
    let dir = Workspace::new().ok_or_else(|| io::Error::other("Cannot create PDF workspace"))?;
    let path = dir.join("stdout");
    let stdout = std::fs::File::create(&path)?;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(Stdio::null())
        .spawn()?;
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            result => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(result.err().unwrap_or_else(|| {
                    io::Error::new(io::ErrorKind::TimedOut, "PDF tool timed out")
                }));
            }
        }
    };
    let mut stdout = Vec::new();
    const LIMIT: u64 = 8 * 1024 * 1024;
    std::fs::File::open(path)?
        .take(LIMIT + 1)
        .read_to_end(&mut stdout)?;
    if stdout.len() as u64 > LIMIT {
        return Err(io::Error::other("PDF text exceeds 8 MiB"));
    }
    Ok(Output {
        status,
        stdout,
        stderr: Vec::new(),
    })
}

pub(crate) fn split_pages(text: &str, total: u32) -> Vec<String> {
    let mut pages: Vec<String> = text
        .split('\u{0C}')
        .map(|page| page.trim_matches('\n').to_string())
        .collect();
    if total > 0 {
        pages.resize(total as usize, String::new());
    } else if pages.last().is_some_and(|page| page.trim().is_empty()) {
        pages.pop();
    }
    if pages.is_empty() {
        pages.push(String::new());
    }
    pages
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn stalled_pdf_process_is_killed_and_reaped() {
        let start = Instant::now();
        let error =
            output_before(Command::new("sleep").arg("10"), Duration::from_millis(30)).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(start.elapsed() < Duration::from_secs(3));
    }
}
