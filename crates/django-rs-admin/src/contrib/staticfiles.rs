//! Static files management.
//!
//! Provides utilities for finding and collecting static files from multiple
//! directories into a single output directory for deployment. This mirrors
//! Django's `django.contrib.staticfiles`.

use std::path::{Path, PathBuf};

use django_rs_core::DjangoError;

/// Finds static files across multiple directories.
///
/// The finder searches configured directories in order, resolving file paths
/// for serving and collecting.
///
/// # Examples
///
/// ```
/// use django_rs_admin::contrib::staticfiles::StaticFilesFinder;
/// use std::path::PathBuf;
///
/// let finder = StaticFilesFinder::new(vec![
///     PathBuf::from("/app/static"),
///     PathBuf::from("/shared/static"),
/// ]);
/// assert_eq!(finder.dirs().len(), 2);
/// ```
#[derive(Debug, Clone, Default)]
pub struct StaticFilesFinder {
    /// The directories to search for static files, in order of priority.
    pub dirs: Vec<PathBuf>,
}

impl StaticFilesFinder {
    /// Creates a new static files finder with the given directories.
    pub const fn new(dirs: Vec<PathBuf>) -> Self {
        Self { dirs }
    }

    /// Returns the configured directories.
    pub fn dirs(&self) -> &[PathBuf] {
        &self.dirs
    }

    /// Finds a static file by its relative path.
    ///
    /// Searches all configured directories in order, returning the first
    /// match found.
    pub fn find(&self, relative_path: &str) -> Option<PathBuf> {
        for dir in &self.dirs {
            let full_path = dir.join(relative_path);
            if full_path.exists() {
                return Some(full_path);
            }
        }
        None
    }

    /// Lists all static files found across all configured directories.
    ///
    /// Returns pairs of (`relative_path`, `absolute_path`). Files in earlier
    /// directories take priority over files in later directories.
    pub fn list_files(&self) -> Vec<(String, PathBuf)> {
        let mut files = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for dir in &self.dirs {
            if let Ok(entries) = list_dir_recursive(dir, dir) {
                for (relative, absolute) in entries {
                    if seen.insert(relative.clone()) {
                        files.push((relative, absolute));
                    }
                }
            }
        }

        files
    }
}

/// Recursively lists files in a directory, returning relative and absolute paths.
fn list_dir_recursive(
    base: &Path,
    current: &Path,
) -> Result<Vec<(String, PathBuf)>, std::io::Error> {
    let mut files = Vec::new();

    if !current.is_dir() {
        return Ok(files);
    }

    for entry in std::fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            files.extend(list_dir_recursive(base, &path)?);
        } else if path.is_file() {
            if let Ok(relative) = path.strip_prefix(base) {
                files.push((
                    relative.to_string_lossy().to_string(),
                    path.clone(),
                ));
            }
        }
    }

    Ok(files)
}

/// Collects all static files from the finder's directories into a destination.
///
/// Copies all found static files to the destination directory, preserving
/// the directory structure. Returns the number of files collected.
///
/// # Errors
///
/// Returns a `DjangoError::IoError` if any file operations fail.
#[allow(clippy::result_large_err)]
pub fn collect_static(
    finder: &StaticFilesFinder,
    dest: &Path,
) -> Result<usize, DjangoError> {
    let files = finder.list_files();
    let mut count = 0;

    for (relative_path, source_path) in &files {
        let dest_path = dest.join(relative_path);

        // Create parent directories
        if let Some(parent) = dest_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::copy(source_path, &dest_path)?;
        count += 1;
    }

    Ok(count)
}

/// Returns the MIME type for a file based on its extension.
///
/// This is used when serving static files to set the correct `Content-Type` header.
pub fn mime_type_for_extension(ext: &str) -> &'static str {
    match ext.to_lowercase().as_str() {
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" | "mjs" => "application/javascript",
        "json" | "map" => "application/json",
        "xml" => "application/xml",
        "txt" => "text/plain",
        "csv" => "text/csv",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_static_files_finder_new() {
        let finder = StaticFilesFinder::new(vec![
            PathBuf::from("/static1"),
            PathBuf::from("/static2"),
        ]);
        assert_eq!(finder.dirs().len(), 2);
    }

    #[test]
    fn test_static_files_finder_default() {
        let finder = StaticFilesFinder::default();
        assert!(finder.dirs().is_empty());
    }

    #[test]
    fn test_static_files_finder_find_nonexistent() {
        let finder = StaticFilesFinder::new(vec![PathBuf::from("/nonexistent")]);
        assert!(finder.find("style.css").is_none());
    }

    #[test]
    fn test_static_files_finder_list_empty() {
        let finder = StaticFilesFinder::new(vec![PathBuf::from("/nonexistent")]);
        assert!(finder.list_files().is_empty());
    }

    #[test]
    fn test_mime_type_html() {
        assert_eq!(mime_type_for_extension("html"), "text/html");
        assert_eq!(mime_type_for_extension("htm"), "text/html");
    }

    #[test]
    fn test_mime_type_css() {
        assert_eq!(mime_type_for_extension("css"), "text/css");
    }

    #[test]
    fn test_mime_type_js() {
        assert_eq!(mime_type_for_extension("js"), "application/javascript");
        assert_eq!(mime_type_for_extension("mjs"), "application/javascript");
    }

    #[test]
    fn test_mime_type_json() {
        assert_eq!(mime_type_for_extension("json"), "application/json");
    }

    #[test]
    fn test_mime_type_images() {
        assert_eq!(mime_type_for_extension("png"), "image/png");
        assert_eq!(mime_type_for_extension("jpg"), "image/jpeg");
        assert_eq!(mime_type_for_extension("gif"), "image/gif");
        assert_eq!(mime_type_for_extension("svg"), "image/svg+xml");
        assert_eq!(mime_type_for_extension("webp"), "image/webp");
    }

    #[test]
    fn test_mime_type_fonts() {
        assert_eq!(mime_type_for_extension("woff"), "font/woff");
        assert_eq!(mime_type_for_extension("woff2"), "font/woff2");
        assert_eq!(mime_type_for_extension("ttf"), "font/ttf");
    }

    #[test]
    fn test_mime_type_unknown() {
        assert_eq!(
            mime_type_for_extension("xyz"),
            "application/octet-stream"
        );
    }

    #[test]
    fn test_mime_type_case_insensitive() {
        assert_eq!(mime_type_for_extension("HTML"), "text/html");
        assert_eq!(mime_type_for_extension("CSS"), "text/css");
        assert_eq!(mime_type_for_extension("JS"), "application/javascript");
    }

    #[test]
    fn test_collect_static_no_dirs() {
        let finder = StaticFilesFinder::new(vec![PathBuf::from("/nonexistent")]);
        let result = collect_static(&finder, Path::new("/tmp/test-output"));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    fn test_collect_static_with_temp_dir() {
        use std::fs;

        // Create temp source and dest directories
        let source_dir = std::env::temp_dir().join("django_rs_test_static_src");
        let dest_dir = std::env::temp_dir().join("django_rs_test_static_dest");

        // Clean up from previous runs
        let _ = fs::remove_dir_all(&source_dir);
        let _ = fs::remove_dir_all(&dest_dir);

        fs::create_dir_all(&source_dir).unwrap();
        fs::write(source_dir.join("test.css"), "body {}").unwrap();
        fs::create_dir_all(source_dir.join("js")).unwrap();
        fs::write(source_dir.join("js/app.js"), "console.log('hello');").unwrap();

        let finder = StaticFilesFinder::new(vec![source_dir.clone()]);
        let result = collect_static(&finder, &dest_dir).unwrap();
        assert_eq!(result, 2);

        assert!(dest_dir.join("test.css").exists());
        assert!(dest_dir.join("js/app.js").exists());

        // Clean up
        let _ = fs::remove_dir_all(&source_dir);
        let _ = fs::remove_dir_all(&dest_dir);
    }
}
