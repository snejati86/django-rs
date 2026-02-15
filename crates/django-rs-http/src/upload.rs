//! Multipart form-data parsing for file uploads.
//!
//! Provides [`parse_multipart`] to extract form fields and uploaded files
//! from `multipart/form-data` request bodies, mirroring Django's file upload
//! handling in `django.http.multipartparser`.

use std::collections::HashMap;

use django_rs_core::DjangoResult;

/// Default maximum memory size for file uploads (2.5 MB).
pub const FILE_UPLOAD_MAX_MEMORY_SIZE: usize = 2_621_440;

/// An uploaded file from a multipart form submission.
#[derive(Debug, Clone)]
pub struct UploadedFile {
    /// The original filename as provided by the client.
    pub name: String,
    /// The MIME content type of the file.
    pub content_type: String,
    /// The size of the file content in bytes.
    pub size: usize,
    /// The raw file content.
    pub content: Vec<u8>,
}

/// The result of parsing a multipart form-data body.
#[derive(Debug, Clone)]
pub struct MultipartData {
    /// Regular form fields: name -> list of values.
    pub fields: HashMap<String, Vec<String>>,
    /// Uploaded files: field name -> list of uploaded files.
    pub files: HashMap<String, Vec<UploadedFile>>,
}

/// Extracts the boundary string from a `Content-Type: multipart/form-data` header.
///
/// The boundary is specified as `boundary=<value>` in the Content-Type header.
/// Returns `None` if the boundary cannot be found.
pub fn extract_boundary(content_type: &str) -> Option<&str> {
    for part in content_type.split(';') {
        let trimmed = part.trim();
        if let Some(boundary) = trimmed.strip_prefix("boundary=") {
            // Remove quotes if present
            let boundary = boundary.trim_matches('"');
            if boundary.is_empty() {
                return None;
            }
            return Some(boundary);
        }
    }
    None
}

/// Parses a multipart/form-data request body.
///
/// Splits the body by the boundary delimiter, then parses each part's
/// headers (particularly `Content-Disposition`) to determine whether
/// the part is a regular form field or a file upload.
///
/// # Arguments
///
/// * `body` - The raw request body bytes
/// * `boundary` - The boundary string from the Content-Type header
///
/// # Errors
///
/// Returns an error if the body cannot be parsed as valid multipart data.
pub fn parse_multipart(body: &[u8], boundary: &str) -> DjangoResult<MultipartData> {
    let mut fields: HashMap<String, Vec<String>> = HashMap::new();
    let mut files: HashMap<String, Vec<UploadedFile>> = HashMap::new();

    let delimiter = format!("--{boundary}");
    let end_delimiter = format!("--{boundary}--");

    // Convert body to string for easier parsing (multipart boundaries are ASCII)
    let body_str = String::from_utf8_lossy(body);

    // Split by delimiter
    let parts: Vec<&str> = body_str.split(&delimiter).collect();

    for part in parts {
        let part = part.trim_start_matches("\r\n").trim_end_matches("\r\n");

        // Skip empty parts and the ending delimiter
        if part.is_empty() || part == "--" || part.starts_with("--") {
            continue;
        }

        // Split headers from body (separated by double CRLF or double LF)
        let (headers_str, body_content) = if let Some(pos) = part.find("\r\n\r\n") {
            (&part[..pos], &part[pos + 4..])
        } else if let Some(pos) = part.find("\n\n") {
            (&part[..pos], &part[pos + 2..])
        } else {
            continue;
        };

        // Parse Content-Disposition header
        let mut field_name = None;
        let mut filename = None;
        let mut part_content_type = "text/plain".to_string();

        for header_line in headers_str.lines() {
            let header_line = header_line.trim();
            if header_line.is_empty() {
                continue;
            }

            let header_lower = header_line.to_lowercase();
            if header_lower.starts_with("content-disposition:") {
                let value = &header_line[header_line.find(':').unwrap_or(0) + 1..];
                let value = value.trim();

                // Extract name
                if let Some(name) = extract_header_param(value, "name") {
                    field_name = Some(name);
                }

                // Extract filename
                if let Some(fname) = extract_header_param(value, "filename") {
                    filename = Some(fname);
                }
            } else if header_lower.starts_with("content-type:") {
                let value = &header_line[header_line.find(':').unwrap_or(0) + 1..];
                part_content_type = value.trim().to_string();
            }
        }

        let Some(name) = field_name else {
            continue;
        };

        // Remove trailing boundary markers from body content
        let body_content = body_content
            .trim_end_matches("\r\n")
            .trim_end_matches(&end_delimiter)
            .trim_end_matches("\r\n");

        if let Some(fname) = filename {
            // This is a file upload
            if fname.is_empty() && body_content.is_empty() {
                // Empty file field, skip
                continue;
            }

            let content = body_content.as_bytes().to_vec();

            // Respect FILE_UPLOAD_MAX_MEMORY_SIZE
            if content.len() > FILE_UPLOAD_MAX_MEMORY_SIZE {
                return Err(django_rs_core::DjangoError::BadRequest(format!(
                    "File '{fname}' exceeds maximum upload size of {FILE_UPLOAD_MAX_MEMORY_SIZE} bytes"
                )));
            }

            let uploaded_file = UploadedFile {
                name: fname,
                content_type: part_content_type,
                size: content.len(),
                content,
            };

            files.entry(name).or_default().push(uploaded_file);
        } else {
            // Regular form field
            fields
                .entry(name)
                .or_default()
                .push(body_content.to_string());
        }
    }

    Ok(MultipartData { fields, files })
}

/// Extracts a parameter value from a header value string.
///
/// For example, from `form-data; name="field1"; filename="file.txt"`,
/// `extract_header_param(value, "name")` returns `Some("field1")`.
fn extract_header_param(header_value: &str, param_name: &str) -> Option<String> {
    let search = format!("{param_name}=\"");
    if let Some(start) = header_value.find(&search) {
        let value_start = start + search.len();
        if let Some(end) = header_value[value_start..].find('"') {
            return Some(header_value[value_start..value_start + end].to_string());
        }
    }

    // Try without quotes
    let search = format!("{param_name}=");
    if let Some(start) = header_value.find(&search) {
        let value_start = start + search.len();
        let rest = &header_value[value_start..];
        let end = rest.find(';').unwrap_or(rest.len());
        let value = rest[..end].trim().trim_matches('"');
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Boundary extraction tests ───────────────────────────────────

    #[test]
    fn test_extract_boundary_basic() {
        let ct = "multipart/form-data; boundary=----WebKitFormBoundary";
        assert_eq!(extract_boundary(ct), Some("----WebKitFormBoundary"));
    }

    #[test]
    fn test_extract_boundary_quoted() {
        let ct = "multipart/form-data; boundary=\"----boundary123\"";
        assert_eq!(extract_boundary(ct), Some("----boundary123"));
    }

    #[test]
    fn test_extract_boundary_missing() {
        let ct = "multipart/form-data";
        assert_eq!(extract_boundary(ct), None);
    }

    #[test]
    fn test_extract_boundary_empty() {
        let ct = "multipart/form-data; boundary=";
        assert_eq!(extract_boundary(ct), None);
    }

    // ── Single file upload ──────────────────────────────────────────

    #[test]
    fn test_parse_single_file() {
        let boundary = "boundary123";
        let body = format!(
            "--{boundary}\r\n\
             Content-Disposition: form-data; name=\"file\"; filename=\"test.txt\"\r\n\
             Content-Type: text/plain\r\n\
             \r\n\
             Hello, World!\r\n\
             --{boundary}--\r\n"
        );

        let result = parse_multipart(body.as_bytes(), boundary).unwrap();
        assert!(result.fields.is_empty());
        assert_eq!(result.files.len(), 1);
        let files = result.files.get("file").unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, "test.txt");
        assert_eq!(files[0].content_type, "text/plain");
        assert_eq!(files[0].content, b"Hello, World!");
        assert_eq!(files[0].size, 13);
    }

    // ── Multiple files ──────────────────────────────────────────────

    #[test]
    fn test_parse_multiple_files() {
        let boundary = "boundary123";
        let body = format!(
            "--{boundary}\r\n\
             Content-Disposition: form-data; name=\"files\"; filename=\"a.txt\"\r\n\
             Content-Type: text/plain\r\n\
             \r\n\
             File A\r\n\
             --{boundary}\r\n\
             Content-Disposition: form-data; name=\"files\"; filename=\"b.txt\"\r\n\
             Content-Type: text/plain\r\n\
             \r\n\
             File B\r\n\
             --{boundary}--\r\n"
        );

        let result = parse_multipart(body.as_bytes(), boundary).unwrap();
        let files = result.files.get("files").unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].name, "a.txt");
        assert_eq!(files[1].name, "b.txt");
    }

    // ── Mixed fields and files ──────────────────────────────────────

    #[test]
    fn test_parse_mixed_fields_and_files() {
        let boundary = "boundary123";
        let body = format!(
            "--{boundary}\r\n\
             Content-Disposition: form-data; name=\"title\"\r\n\
             \r\n\
             My Document\r\n\
             --{boundary}\r\n\
             Content-Disposition: form-data; name=\"description\"\r\n\
             \r\n\
             A test document\r\n\
             --{boundary}\r\n\
             Content-Disposition: form-data; name=\"file\"; filename=\"doc.pdf\"\r\n\
             Content-Type: application/pdf\r\n\
             \r\n\
             %PDF-1.4 fake content\r\n\
             --{boundary}--\r\n"
        );

        let result = parse_multipart(body.as_bytes(), boundary).unwrap();
        assert_eq!(result.fields.len(), 2);
        assert_eq!(result.fields.get("title").unwrap(), &vec!["My Document"]);
        assert_eq!(
            result.fields.get("description").unwrap(),
            &vec!["A test document"]
        );
        assert_eq!(result.files.len(), 1);
        let files = result.files.get("file").unwrap();
        assert_eq!(files[0].name, "doc.pdf");
        assert_eq!(files[0].content_type, "application/pdf");
    }

    // ── Empty body ──────────────────────────────────────────────────

    #[test]
    fn test_parse_empty_body() {
        let result = parse_multipart(b"", "boundary").unwrap();
        assert!(result.fields.is_empty());
        assert!(result.files.is_empty());
    }

    // ── Fields only ─────────────────────────────────────────────────

    #[test]
    fn test_parse_fields_only() {
        let boundary = "boundary123";
        let body = format!(
            "--{boundary}\r\n\
             Content-Disposition: form-data; name=\"field1\"\r\n\
             \r\n\
             value1\r\n\
             --{boundary}\r\n\
             Content-Disposition: form-data; name=\"field2\"\r\n\
             \r\n\
             value2\r\n\
             --{boundary}--\r\n"
        );

        let result = parse_multipart(body.as_bytes(), boundary).unwrap();
        assert_eq!(result.fields.len(), 2);
        assert!(result.files.is_empty());
    }

    // ── Multiple values for same field ──────────────────────────────

    #[test]
    fn test_parse_multiple_values_same_field() {
        let boundary = "boundary123";
        let body = format!(
            "--{boundary}\r\n\
             Content-Disposition: form-data; name=\"color\"\r\n\
             \r\n\
             red\r\n\
             --{boundary}\r\n\
             Content-Disposition: form-data; name=\"color\"\r\n\
             \r\n\
             blue\r\n\
             --{boundary}--\r\n"
        );

        let result = parse_multipart(body.as_bytes(), boundary).unwrap();
        let colors = result.fields.get("color").unwrap();
        assert_eq!(colors, &vec!["red", "blue"]);
    }

    // ── Empty file field ────────────────────────────────────────────

    #[test]
    fn test_parse_empty_file_field() {
        let boundary = "boundary123";
        let body = format!(
            "--{boundary}\r\n\
             Content-Disposition: form-data; name=\"file\"; filename=\"\"\r\n\
             Content-Type: application/octet-stream\r\n\
             \r\n\
             \r\n\
             --{boundary}--\r\n"
        );

        let result = parse_multipart(body.as_bytes(), boundary).unwrap();
        // Empty filename and empty content should be skipped
        assert!(result.files.is_empty() || result.files.get("file").map_or(true, Vec::is_empty));
    }

    // ── Large file size limit ───────────────────────────────────────

    #[test]
    fn test_parse_large_file_rejected() {
        let boundary = "boundary123";
        let large_content = "X".repeat(FILE_UPLOAD_MAX_MEMORY_SIZE + 1);
        let body = format!(
            "--{boundary}\r\n\
             Content-Disposition: form-data; name=\"file\"; filename=\"large.bin\"\r\n\
             Content-Type: application/octet-stream\r\n\
             \r\n\
             {large_content}\r\n\
             --{boundary}--\r\n"
        );

        let result = parse_multipart(body.as_bytes(), boundary);
        assert!(result.is_err());
    }

    // ── File with special characters in filename ────────────────────

    #[test]
    fn test_parse_file_special_chars_filename() {
        let boundary = "boundary123";
        let body = format!(
            "--{boundary}\r\n\
             Content-Disposition: form-data; name=\"file\"; filename=\"my file (1).txt\"\r\n\
             Content-Type: text/plain\r\n\
             \r\n\
             content\r\n\
             --{boundary}--\r\n"
        );

        let result = parse_multipart(body.as_bytes(), boundary).unwrap();
        let files = result.files.get("file").unwrap();
        assert_eq!(files[0].name, "my file (1).txt");
    }

    // ── Binary content ──────────────────────────────────────────────

    #[test]
    fn test_parse_binary_like_content() {
        let boundary = "boundary123";
        let body = format!(
            "--{boundary}\r\n\
             Content-Disposition: form-data; name=\"data\"; filename=\"data.bin\"\r\n\
             Content-Type: application/octet-stream\r\n\
             \r\n\
             \x00\x01\x02\x03\r\n\
             --{boundary}--\r\n"
        );

        let result = parse_multipart(body.as_bytes(), boundary).unwrap();
        assert!(result.files.contains_key("data"));
    }

    // ── LF line endings ─────────────────────────────────────────────

    #[test]
    fn test_parse_lf_line_endings() {
        let boundary = "boundary123";
        let body = format!(
            "--{boundary}\n\
             Content-Disposition: form-data; name=\"field\"\n\
             \n\
             value\n\
             --{boundary}--\n"
        );

        let result = parse_multipart(body.as_bytes(), boundary).unwrap();
        assert!(result.fields.contains_key("field"));
    }

    // ── No Content-Disposition ──────────────────────────────────────

    #[test]
    fn test_parse_missing_content_disposition() {
        let boundary = "boundary123";
        let body = format!(
            "--{boundary}\r\n\
             Content-Type: text/plain\r\n\
             \r\n\
             orphan data\r\n\
             --{boundary}--\r\n"
        );

        let result = parse_multipart(body.as_bytes(), boundary).unwrap();
        // Parts without Content-Disposition name should be skipped
        assert!(result.fields.is_empty());
        assert!(result.files.is_empty());
    }

    // ── Multiple files different fields ─────────────────────────────

    #[test]
    fn test_parse_multiple_files_different_fields() {
        let boundary = "boundary123";
        let body = format!(
            "--{boundary}\r\n\
             Content-Disposition: form-data; name=\"avatar\"; filename=\"me.jpg\"\r\n\
             Content-Type: image/jpeg\r\n\
             \r\n\
             JPEG data\r\n\
             --{boundary}\r\n\
             Content-Disposition: form-data; name=\"resume\"; filename=\"cv.pdf\"\r\n\
             Content-Type: application/pdf\r\n\
             \r\n\
             PDF data\r\n\
             --{boundary}--\r\n"
        );

        let result = parse_multipart(body.as_bytes(), boundary).unwrap();
        assert_eq!(result.files.len(), 2);
        assert!(result.files.contains_key("avatar"));
        assert!(result.files.contains_key("resume"));
    }

    // ── Header param extraction ─────────────────────────────────────

    #[test]
    fn test_extract_header_param_quoted() {
        let value = "form-data; name=\"field1\"; filename=\"test.txt\"";
        assert_eq!(
            extract_header_param(value, "name"),
            Some("field1".to_string())
        );
        assert_eq!(
            extract_header_param(value, "filename"),
            Some("test.txt".to_string())
        );
    }

    #[test]
    fn test_extract_header_param_missing() {
        let value = "form-data; name=\"field1\"";
        assert_eq!(extract_header_param(value, "filename"), None);
    }

    // ── File content type ───────────────────────────────────────────

    #[test]
    fn test_parse_file_content_type_preserved() {
        let boundary = "boundary123";
        let body = format!(
            "--{boundary}\r\n\
             Content-Disposition: form-data; name=\"img\"; filename=\"photo.png\"\r\n\
             Content-Type: image/png\r\n\
             \r\n\
             PNG data\r\n\
             --{boundary}--\r\n"
        );

        let result = parse_multipart(body.as_bytes(), boundary).unwrap();
        let files = result.files.get("img").unwrap();
        assert_eq!(files[0].content_type, "image/png");
    }
}
