//! A size-capped log file: `name`, `name.1` … `name.<keep-1>`, newest first.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Cheap to clone; every clone writes to the same file.
#[derive(Clone)]
pub struct RotatingLog {
    inner: Arc<Mutex<Inner>>,
}

struct Inner {
    dir: PathBuf,
    name: String,
    max_bytes: u64,
    keep: usize,
    file: Option<File>,
    written: u64,
}

impl RotatingLog {
    /// Append to `dir/name`, rotating once a write would pass `max_bytes`.
    /// `keep` counts the current file; it is at least 1.
    pub fn open(dir: &Path, name: &str, max_bytes: u64, keep: usize) -> io::Result<Self> {
        fs::create_dir_all(dir)?;
        let path = dir.join(name);
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        let written = file.metadata().map(|m| m.len()).unwrap_or(0);
        Ok(Self {
            inner: Arc::new(Mutex::new(Inner {
                dir: dir.to_path_buf(),
                name: name.to_string(),
                max_bytes: max_bytes.max(1),
                keep: keep.max(1),
                file: Some(file),
                written,
            })),
        })
    }
}

impl Inner {
    fn path(&self, n: usize) -> PathBuf {
        if n == 0 {
            self.dir.join(&self.name)
        } else {
            self.dir.join(format!("{}.{n}", self.name))
        }
    }

    fn rotate(&mut self) -> io::Result<()> {
        self.file = None;
        let last = self.keep - 1;
        if last == 0 {
            let _ = fs::remove_file(self.path(0));
        } else {
            let _ = fs::remove_file(self.path(last));
            for n in (0..last).rev() {
                let from = self.path(n);
                if from.exists() {
                    fs::rename(&from, self.path(n + 1))?;
                }
            }
        }
        self.file = Some(
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.path(0))?,
        );
        self.written = 0;
        Ok(())
    }
}

impl Write for RotatingLog {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let len = buf.len() as u64;
        if inner.written > 0 && inner.written + len > inner.max_bytes {
            inner.rotate()?;
        }
        let file = match inner.file.as_mut() {
            Some(file) => file,
            None => return Err(io::Error::other("log file closed")),
        };
        file.write_all(buf)?;
        inner.written += len;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        match inner.file.as_mut() {
            Some(file) => file.flush(),
            None => Ok(()),
        }
    }
}

/// Existing log files for `name` in `dir`, newest first.
pub fn log_files(dir: &Path, name: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let current = dir.join(name);
    if current.is_file() {
        found.push(current);
    }
    let mut rotated: Vec<(usize, PathBuf)> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let file_name = entry.file_name();
            let n = file_name
                .to_str()?
                .strip_prefix(name)?
                .strip_prefix('.')?
                .parse::<usize>()
                .ok()?;
            Some((n, entry.path()))
        })
        .collect();
    rotated.sort_by_key(|(n, _)| *n);
    found.extend(rotated.into_iter().map(|(_, path)| path));
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mystique-log-{tag}-{}",
            uuid::Uuid::new_v4().simple()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn rotates_at_the_cap_and_keeps_only_the_newest_files() {
        let dir = temp_dir("rotate");
        let mut log = RotatingLog::open(&dir, "app.log", 10, 3).unwrap();
        for line in ["aaaaaaaa\n", "bbbbbbbb\n", "cccccccc\n", "dddddddd\n"] {
            log.write_all(line.as_bytes()).unwrap();
        }
        let files = log_files(&dir, "app.log");
        let names: Vec<_> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["app.log", "app.log.1", "app.log.2"]);
        assert_eq!(fs::read_to_string(&files[0]).unwrap(), "dddddddd\n");
        assert_eq!(fs::read_to_string(&files[1]).unwrap(), "cccccccc\n");
        assert_eq!(fs::read_to_string(&files[2]).unwrap(), "bbbbbbbb\n");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn appends_to_an_existing_file_and_counts_its_size() {
        let dir = temp_dir("append");
        fs::write(dir.join("app.log"), "old-line\n").unwrap();
        let mut log = RotatingLog::open(&dir, "app.log", 12, 2).unwrap();
        log.write_all(b"new-line\n").unwrap();
        assert_eq!(
            fs::read_to_string(dir.join("app.log")).unwrap(),
            "new-line\n"
        );
        assert_eq!(
            fs::read_to_string(dir.join("app.log.1")).unwrap(),
            "old-line\n"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_line_larger_than_the_cap_is_still_written() {
        let dir = temp_dir("large");
        let mut log = RotatingLog::open(&dir, "app.log", 4, 1).unwrap();
        log.write_all(b"0123456789\n").unwrap();
        log.write_all(b"x\n").unwrap();
        assert_eq!(fs::read_to_string(dir.join("app.log")).unwrap(), "x\n");
        assert_eq!(log_files(&dir, "app.log").len(), 1);
        fs::remove_dir_all(dir).unwrap();
    }
}
