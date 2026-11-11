use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// A tiny filesystem abstraction rooted at `/` by default.
///
/// Setting `LOQ_SYSFS_ROOT` makes every lookup relative to that directory,
/// which lets the test suite run against a fake sysfs tree without touching
/// the real hardware.
#[derive(Debug, Clone)]
pub struct Sysfs {
    root: PathBuf,
}

impl Default for Sysfs {
    fn default() -> Self {
        Self::new()
    }
}

impl Sysfs {
    pub fn new() -> Self {
        let root = std::env::var_os("LOQ_SYSFS_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        Self { root }
    }

    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Resolve an absolute sysfs path against the configured root.
    pub fn path(&self, path: &str) -> PathBuf {
        self.root.join(path.trim_start_matches('/'))
    }

    pub fn exists(&self, path: &str) -> bool {
        self.path(path).exists()
    }

    /// Read a file and trim surrounding whitespace. Returns `None` on any
    /// I/O error, which is the common case for optional sysfs attributes.
    pub fn read(&self, path: &str) -> Option<String> {
        std::fs::read_to_string(self.path(path))
            .ok()
            .map(|value| value.trim().to_string())
    }

    pub fn read_required(&self, path: &str) -> Result<String> {
        let full = self.path(path);
        std::fs::read_to_string(&full)
            .map(|value| value.trim().to_string())
            .map_err(|source| Error::Read { path: full, source })
    }

    pub fn read_i64(&self, path: &str) -> Option<i64> {
        self.read(path)?.parse().ok()
    }

    pub fn read_u64(&self, path: &str) -> Option<u64> {
        self.read(path)?.parse().ok()
    }

    pub fn read_f64(&self, path: &str) -> Option<f64> {
        self.read(path)?.parse().ok()
    }

    /// List directory entries, sorted for stable output.
    pub fn list_dir(&self, path: &str) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(self.path(path)) else {
            return Vec::new();
        };

        let mut names: Vec<String> = entries
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| entry.file_name().into_string().ok())
            .collect();
        names.sort();
        names
    }

    /// Glob an absolute pattern against the configured root.
    pub fn glob(&self, pattern: &str) -> Vec<PathBuf> {
        let full = self.path(pattern);
        let pattern = full.to_string_lossy().to_string();
        let Ok(paths) = glob::glob(&pattern) else {
            return Vec::new();
        };

        let mut matches: Vec<PathBuf> = paths.filter_map(|entry| entry.ok()).collect();
        matches.sort();
        matches
    }

    /// Convert a real path returned by [`Sysfs::glob`] back into a
    /// `/`-rooted path string for display.
    pub fn display_path(&self, path: &Path) -> String {
        match path.strip_prefix(&self.root) {
            Ok(rest) => format!("/{}", rest.display()),
            Err(_) => path.display().to_string(),
        }
    }
}
