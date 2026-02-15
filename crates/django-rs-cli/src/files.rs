//! File handling and storage framework for django-rs.
//!
//! This module provides the [`Storage`] trait and [`FileSystemStorage`] for managing
//! file uploads and serving. It mirrors Django's `django.core.files.storage` module.
//!
//! All file operations use async I/O to avoid blocking the tokio runtime.

use std::path::PathBuf;

use async_trait::async_trait;

use django_rs_core::DjangoError;

/// A backend for file storage operations.
///
/// All methods are async and the trait requires `Send + Sync` to support
/// concurrent file operations from multiple tokio tasks.
#[async_trait]
pub trait Storage: Send + Sync {
    /// Saves content to a file with the given name.
    ///
    /// Returns the actual name used (which may differ from the input name
    /// to avoid collisions).
    async fn save(&self, name: &str, content: &[u8]) -> Result<String, DjangoError>;

    /// Opens a file and returns its contents.
    async fn open(&self, name: &str) -> Result<Vec<u8>, DjangoError>;

    /// Deletes a file by name.
    async fn delete(&self, name: &str) -> Result<(), DjangoError>;

    /// Checks whether a file with the given name exists.
    async fn exists(&self, name: &str) -> Result<bool, DjangoError>;

    /// Returns the URL for serving the file.
    fn url(&self, name: &str) -> String;

    /// Returns the size of the file in bytes.
    async fn size(&self, name: &str) -> Result<u64, DjangoError>;
}

/// Filesystem-based storage backend.
///
/// Stores files on the local filesystem under a configurable root directory.
/// All operations use async I/O via `tokio::fs`.
#[derive(Debug, Clone)]
pub struct FileSystemStorage {
    /// The root directory on the filesystem where files are stored.
    pub location: PathBuf,
    /// The base URL for serving stored files.
    pub base_url: String,
}

impl FileSystemStorage {
    /// Creates a new filesystem storage rooted at `location`.
    pub fn new(location: PathBuf, base_url: impl Into<String>) -> Self {
        Self {
            location,
            base_url: base_url.into(),
        }
    }

    /// Returns the full filesystem path for a given file name.
    pub fn path(&self, name: &str) -> PathBuf {
        self.location.join(name)
    }

    /// Generates a unique name to avoid overwriting existing files.
    async fn get_available_name(&self, name: &str) -> Result<String, DjangoError> {
        let path = self.path(name);
        if !path.exists() {
            return Ok(name.to_string());
        }

        // Add a suffix to make the name unique
        let stem = std::path::Path::new(name)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(name);
        let ext = std::path::Path::new(name)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("");

        for i in 1..=1000 {
            let candidate = if ext.is_empty() {
                format!("{stem}_{i}")
            } else {
                format!("{stem}_{i}.{ext}")
            };

            if !self.path(&candidate).exists() {
                return Ok(candidate);
            }
        }

        Err(DjangoError::InternalServerError(
            "Could not find an available filename".to_string(),
        ))
    }
}

#[async_trait]
impl Storage for FileSystemStorage {
    async fn save(&self, name: &str, content: &[u8]) -> Result<String, DjangoError> {
        let actual_name = self.get_available_name(name).await?;
        let full_path = self.path(&actual_name);

        // Ensure parent directory exists
        if let Some(parent) = full_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        tokio::fs::write(&full_path, content).await?;
        Ok(actual_name)
    }

    async fn open(&self, name: &str) -> Result<Vec<u8>, DjangoError> {
        let full_path = self.path(name);
        tokio::fs::read(&full_path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                DjangoError::NotFound(format!("File not found: {name}"))
            } else {
                DjangoError::IoError(e)
            }
        })
    }

    async fn delete(&self, name: &str) -> Result<(), DjangoError> {
        let full_path = self.path(name);
        tokio::fs::remove_file(&full_path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                DjangoError::NotFound(format!("File not found: {name}"))
            } else {
                DjangoError::IoError(e)
            }
        })
    }

    async fn exists(&self, name: &str) -> Result<bool, DjangoError> {
        let full_path = self.path(name);
        Ok(full_path.exists())
    }

    fn url(&self, name: &str) -> String {
        let base = self.base_url.trim_end_matches('/');
        let name = name.trim_start_matches('/');
        format!("{base}/{name}")
    }

    async fn size(&self, name: &str) -> Result<u64, DjangoError> {
        let full_path = self.path(name);
        let metadata = tokio::fs::metadata(&full_path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                DjangoError::NotFound(format!("File not found: {name}"))
            } else {
                DjangoError::IoError(e)
            }
        })?;
        Ok(metadata.len())
    }
}

/// Represents an uploaded file received from a multipart form submission.
///
/// Contains the file's metadata and content in memory.
#[derive(Debug, Clone)]
pub struct UploadedFile {
    /// The original filename from the upload.
    pub name: String,
    /// The MIME content type of the file.
    pub content_type: String,
    /// The size of the file in bytes.
    pub size: u64,
    /// The file content as raw bytes.
    pub content: Vec<u8>,
}

impl UploadedFile {
    /// Creates a new `UploadedFile` from its components.
    pub fn new(
        name: impl Into<String>,
        content_type: impl Into<String>,
        content: Vec<u8>,
    ) -> Self {
        let content_len = content.len() as u64;
        Self {
            name: name.into(),
            content_type: content_type.into(),
            size: content_len,
            content,
        }
    }

    /// Returns the file extension, if any.
    pub fn extension(&self) -> Option<&str> {
        std::path::Path::new(&self.name)
            .extension()
            .and_then(|ext| ext.to_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── UploadedFile tests ────────────────────────────────────────────

    #[test]
    fn test_uploaded_file_new() {
        let file = UploadedFile::new("test.txt", "text/plain", b"hello world".to_vec());
        assert_eq!(file.name, "test.txt");
        assert_eq!(file.content_type, "text/plain");
        assert_eq!(file.size, 11);
        assert_eq!(file.content, b"hello world");
    }

    #[test]
    fn test_uploaded_file_extension() {
        let file = UploadedFile::new("photo.jpg", "image/jpeg", vec![]);
        assert_eq!(file.extension(), Some("jpg"));
    }

    #[test]
    fn test_uploaded_file_no_extension() {
        let file = UploadedFile::new("README", "text/plain", vec![]);
        assert!(file.extension().is_none());
    }

    // ── FileSystemStorage tests ───────────────────────────────────────

    #[test]
    fn test_filesystem_storage_path() {
        let storage = FileSystemStorage::new(
            PathBuf::from("/media"),
            "/media/",
        );
        assert_eq!(storage.path("photo.jpg"), PathBuf::from("/media/photo.jpg"));
    }

    #[test]
    fn test_filesystem_storage_url() {
        let storage = FileSystemStorage::new(
            PathBuf::from("/media"),
            "/media/",
        );
        assert_eq!(storage.url("photo.jpg"), "/media/photo.jpg");
    }

    #[test]
    fn test_filesystem_storage_url_normalizes_slashes() {
        let storage = FileSystemStorage::new(
            PathBuf::from("/media"),
            "/media",
        );
        assert_eq!(storage.url("/photo.jpg"), "/media/photo.jpg");
    }

    #[tokio::test]
    async fn test_filesystem_storage_save_and_open() {
        let dir = tempfile::tempdir().unwrap();
        let storage = FileSystemStorage::new(
            dir.path().to_path_buf(),
            "/media/",
        );

        let name = storage.save("test.txt", b"hello").await.unwrap();
        assert_eq!(name, "test.txt");

        let content = storage.open("test.txt").await.unwrap();
        assert_eq!(content, b"hello");
    }

    #[tokio::test]
    async fn test_filesystem_storage_save_deduplicates_names() {
        let dir = tempfile::tempdir().unwrap();
        let storage = FileSystemStorage::new(
            dir.path().to_path_buf(),
            "/media/",
        );

        let name1 = storage.save("test.txt", b"first").await.unwrap();
        let name2 = storage.save("test.txt", b"second").await.unwrap();

        assert_eq!(name1, "test.txt");
        assert_ne!(name2, "test.txt");
        assert!(name2.starts_with("test_"));
    }

    #[tokio::test]
    async fn test_filesystem_storage_save_creates_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let storage = FileSystemStorage::new(
            dir.path().to_path_buf(),
            "/media/",
        );

        let name = storage.save("subdir/deep/file.txt", b"nested").await.unwrap();
        assert_eq!(name, "subdir/deep/file.txt");

        let content = storage.open("subdir/deep/file.txt").await.unwrap();
        assert_eq!(content, b"nested");
    }

    #[tokio::test]
    async fn test_filesystem_storage_delete() {
        let dir = tempfile::tempdir().unwrap();
        let storage = FileSystemStorage::new(
            dir.path().to_path_buf(),
            "/media/",
        );

        storage.save("to_delete.txt", b"data").await.unwrap();
        assert!(storage.exists("to_delete.txt").await.unwrap());

        storage.delete("to_delete.txt").await.unwrap();
        assert!(!storage.exists("to_delete.txt").await.unwrap());
    }

    #[tokio::test]
    async fn test_filesystem_storage_delete_nonexistent() {
        let dir = tempfile::tempdir().unwrap();
        let storage = FileSystemStorage::new(
            dir.path().to_path_buf(),
            "/media/",
        );

        let result = storage.delete("nonexistent.txt").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_filesystem_storage_exists() {
        let dir = tempfile::tempdir().unwrap();
        let storage = FileSystemStorage::new(
            dir.path().to_path_buf(),
            "/media/",
        );

        assert!(!storage.exists("file.txt").await.unwrap());

        storage.save("file.txt", b"content").await.unwrap();
        assert!(storage.exists("file.txt").await.unwrap());
    }

    #[tokio::test]
    async fn test_filesystem_storage_size() {
        let dir = tempfile::tempdir().unwrap();
        let storage = FileSystemStorage::new(
            dir.path().to_path_buf(),
            "/media/",
        );

        storage.save("sized.txt", b"12345").await.unwrap();
        let size = storage.size("sized.txt").await.unwrap();
        assert_eq!(size, 5);
    }

    #[tokio::test]
    async fn test_filesystem_storage_size_nonexistent() {
        let dir = tempfile::tempdir().unwrap();
        let storage = FileSystemStorage::new(
            dir.path().to_path_buf(),
            "/media/",
        );

        let result = storage.size("nonexistent.txt").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_filesystem_storage_open_nonexistent() {
        let dir = tempfile::tempdir().unwrap();
        let storage = FileSystemStorage::new(
            dir.path().to_path_buf(),
            "/media/",
        );

        let result = storage.open("nonexistent.txt").await;
        assert!(result.is_err());
    }
}
