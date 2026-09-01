//! Bounded file-tail for detached process logs (server/client).
//!
//! Does not re-pipe stdout into the Hub. Shared-read of append-only files under
//! `logs/dev-tools/`. Missing or locked files yield an empty view, not failure.

use std::collections::VecDeque;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::config::{ACTIVITY_LOG_CAP, ACTIVITY_VIEW_LINES};

/// How far back to seek on first open when the file is already large.
const INITIAL_TAIL_BYTES: u64 = 64 * 1024;

#[derive(Debug)]
pub struct FileTail {
    path: PathBuf,
    offset: u64,
    lines: VecDeque<String>,
    prefix: String,
    primed: bool,
}

impl FileTail {
    #[must_use]
    pub fn new(path: PathBuf, prefix: &str) -> Self {
        Self {
            path,
            offset: 0,
            lines: VecDeque::new(),
            prefix: prefix.to_string(),
            primed: false,
        }
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Poll the file for new content. Safe if the file is missing or briefly locked.
    pub fn poll(&mut self) {
        let Ok(mut file) = open_shared_read(&self.path) else {
            return;
        };
        let Ok(len) = file.seek(SeekFrom::End(0)) else {
            return;
        };
        if !self.primed {
            self.offset = len.saturating_sub(INITIAL_TAIL_BYTES);
            self.primed = true;
            if self.offset > 0 {
                // Skip partial first line after a mid-file seek.
                let _ = file.seek(SeekFrom::Start(self.offset));
                let mut skip = [0u8; 1];
                while self.offset < len {
                    match file.read(&mut skip) {
                        Ok(1) => {
                            self.offset += 1;
                            if skip[0] == b'\n' {
                                break;
                            }
                        }
                        _ => break,
                    }
                }
            }
        }
        if len < self.offset {
            // Truncated / rotated.
            self.offset = 0;
            self.lines.clear();
        }
        if len == self.offset {
            return;
        }
        let Ok(_) = file.seek(SeekFrom::Start(self.offset)) else {
            return;
        };
        let mut buf = Vec::new();
        if file.read_to_end(&mut buf).is_err() {
            return;
        }
        self.offset += buf.len() as u64;
        let text = String::from_utf8_lossy(&buf);
        for raw in text.split_inclusive('\n') {
            let line = raw.trim_end_matches(['\r', '\n']);
            if line.is_empty() {
                continue;
            }
            // Only commit complete lines; keep remainder in offset by rewinding
            // if the chunk did not end with newline.
            let complete = raw.ends_with('\n') || raw.ends_with("\r\n");
            if !complete {
                self.offset -= raw.len() as u64;
                break;
            }
            let display = {
                let body = if self.prefix.is_empty() {
                    line.to_string()
                } else {
                    format!("{} | {line}", self.prefix)
                };
                crate::log_buffer::stamp_line(&body)
            };
            if self.lines.len() >= ACTIVITY_LOG_CAP {
                self.lines.pop_front();
            }
            self.lines.push_back(display);
        }
    }

    #[must_use]
    pub fn view_lines(&self) -> Vec<String> {
        let skip = self.lines.len().saturating_sub(ACTIVITY_VIEW_LINES);
        self.lines.iter().skip(skip).cloned().collect()
    }

    /// Clear the in-memory tail view and advance the file offset to EOF
    /// so previously-read lines are not re-ingested on the next poll.
    /// Does not truncate the log file on disk.
    pub fn clear_view(&mut self) {
        self.lines.clear();
        if let Ok(mut file) = open_shared_read(&self.path)
            && let Ok(len) = file.seek(SeekFrom::End(0))
        {
            self.offset = len;
            self.primed = true;
        }
    }
}

fn open_shared_read(path: &Path) -> std::io::Result<File> {
    let mut opts = std::fs::OpenOptions::new();
    opts.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE
        opts.share_mode(0x7);
    }
    opts.open(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    fn temp_log(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("purgatory-log-tail-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir.join("server.log")
    }

    #[test]
    fn missing_file_is_empty() {
        let path = temp_log("missing");
        let mut tail = FileTail::new(path, "server");
        tail.poll();
        assert!(tail.view_lines().is_empty());
    }

    #[test]
    fn reads_appended_lines() {
        let path = temp_log("append");
        fs::write(&path, "first\n").unwrap();
        let mut tail = FileTail::new(path.clone(), "server");
        tail.poll();
        let first = &tail.view_lines()[0];
        assert!(first.ends_with("server | first"), "got {first}");
        assert!(
            first.as_bytes().get(2) == Some(&b':') && first.as_bytes().get(5) == Some(&b':'),
            "expected HH:MM:SS prefix, got {first}"
        );
        let mut f = fs::OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(f, "handshake accepted connection_id=1").unwrap();
        drop(f);
        tail.poll();
        let view = tail.view_lines();
        assert_eq!(view.len(), 2);
        assert!(view[1].contains("handshake accepted"));
        assert!(view[1].contains("server |"));
    }

    #[test]
    fn view_is_bounded() {
        let path = temp_log("bound");
        let mut f = fs::File::create(&path).unwrap();
        for i in 0..(ACTIVITY_LOG_CAP + 20) {
            writeln!(f, "line {i}").unwrap();
        }
        drop(f);
        let mut tail = FileTail::new(path, "server");
        tail.poll();
        assert!(tail.view_lines().len() <= ACTIVITY_VIEW_LINES);
        assert!(
            tail.view_lines()
                .last()
                .unwrap()
                .contains(&format!("line {}", ACTIVITY_LOG_CAP + 19))
        );
    }
}
