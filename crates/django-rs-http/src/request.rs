//! HTTP request type.
//!
//! [`HttpRequest`] mirrors Django's `django.http.HttpRequest`, providing access
//! to the request method, path, headers, query parameters, POST data, and metadata.

use std::collections::HashMap;

use http::{HeaderMap, Method};

use crate::cookies::{self, CookieError};
use crate::querydict::QueryDict;
use crate::upload::UploadedFile;
use crate::urls::resolver::ResolverMatch;

/// An HTTP request, modeled after Django's `HttpRequest`.
///
/// Provides access to the request method, URL path, headers, GET/POST parameters,
/// and server metadata. Instances are typically created from an incoming Axum request
/// via [`HttpRequest::from_axum`].
///
/// # Examples
///
/// ```
/// use django_rs_http::HttpRequest;
///
/// let request = HttpRequest::builder()
///     .method(http::Method::GET)
///     .path("/articles/2024/")
///     .query_string("page=1")
///     .build();
///
/// assert_eq!(request.method(), &http::Method::GET);
/// assert_eq!(request.path(), "/articles/2024/");
/// assert_eq!(request.get().get("page"), Some("1"));
/// ```
#[derive(Debug)]
pub struct HttpRequest {
    method: Method,
    path: String,
    path_info: String,
    query_string: String,
    content_type: Option<String>,
    get: QueryDict,
    post: QueryDict,
    headers: HeaderMap,
    meta: HashMap<String, String>,
    body: Vec<u8>,
    resolver_match: Option<ResolverMatch>,
    scheme: String,
    cached_cookies: std::sync::OnceLock<HashMap<String, String>>,
    files: HashMap<String, Vec<UploadedFile>>,
}

impl HttpRequest {
    /// Creates a new [`HttpRequestBuilder`] for constructing an `HttpRequest`.
    pub fn builder() -> HttpRequestBuilder {
        HttpRequestBuilder::default()
    }

    /// Creates an `HttpRequest` from an Axum/hyper request and its body bytes.
    ///
    /// This extracts the method, URI, headers, and body from the Axum request
    /// and populates the `HttpRequest` fields accordingly.
    pub fn from_axum(parts: http::request::Parts, body: Vec<u8>) -> Self {
        let method = parts.method;
        let uri = parts.uri;
        let headers = parts.headers;

        let path = uri.path().to_string();
        let path_info = path.clone();
        let query_string = uri.query().unwrap_or("").to_string();
        let get = QueryDict::parse(&query_string);

        let content_type = headers
            .get(http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(String::from);

        // Parse POST data from form-encoded body
        let post = if content_type
            .as_deref()
            .is_some_and(|ct| ct.starts_with("application/x-www-form-urlencoded"))
        {
            let body_str = String::from_utf8_lossy(&body);
            QueryDict::parse(&body_str)
        } else {
            QueryDict::new()
        };

        // Build META dict
        let mut meta = HashMap::new();

        // HTTP_ headers
        for (name, value) in &headers {
            let meta_key = format!(
                "HTTP_{}",
                name.as_str().to_uppercase().replace('-', "_")
            );
            if let Ok(v) = value.to_str() {
                meta.insert(meta_key, v.to_string());
            }
        }

        // Standard META entries
        if let Some(host) = headers.get(http::header::HOST).and_then(|v| v.to_str().ok()) {
            meta.insert("SERVER_NAME".to_string(), host.to_string());
            meta.insert("HTTP_HOST".to_string(), host.to_string());
        }

        meta.insert("REQUEST_METHOD".to_string(), method.to_string());
        meta.insert("PATH_INFO".to_string(), path_info.clone());
        meta.insert("QUERY_STRING".to_string(), query_string.clone());

        if let Some(ct) = &content_type {
            meta.insert("CONTENT_TYPE".to_string(), ct.clone());
        }

        meta.insert(
            "CONTENT_LENGTH".to_string(),
            body.len().to_string(),
        );

        let scheme = if headers
            .get("x-forwarded-proto")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v == "https")
        {
            "https".to_string()
        } else {
            "http".to_string()
        };

        // Parse multipart data if content type is multipart/form-data
        let (post, files) = if content_type
            .as_deref()
            .is_some_and(|ct| ct.starts_with("multipart/form-data"))
        {
            if let Some(boundary) = content_type.as_deref().and_then(crate::upload::extract_boundary) {
                match crate::upload::parse_multipart(&body, boundary) {
                    Ok(multipart) => {
                        let mut post_dict = QueryDict::new_mutable();
                        for (name, values) in &multipart.fields {
                            for value in values {
                                let _ = post_dict.append(name, value);
                            }
                        }
                        (post_dict, multipart.files)
                    }
                    Err(_) => (post, HashMap::new()),
                }
            } else {
                (post, HashMap::new())
            }
        } else {
            (post, HashMap::new())
        };

        Self {
            method,
            path,
            path_info,
            query_string,
            content_type,
            get,
            post,
            headers,
            meta,
            body,
            resolver_match: None,
            scheme,
            cached_cookies: std::sync::OnceLock::new(),
            files,
        }
    }

    /// Returns the HTTP method.
    pub const fn method(&self) -> &Method {
        &self.method
    }

    /// Returns the request path (without query string).
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns the path info, which is the path portion suitable for routing.
    pub fn path_info(&self) -> &str {
        &self.path_info
    }

    /// Returns the raw query string (without the leading `?`).
    pub fn query_string(&self) -> &str {
        &self.query_string
    }

    /// Returns the content type of the request body, if set.
    pub fn content_type(&self) -> Option<&str> {
        self.content_type.as_deref()
    }

    /// Returns the GET query parameters as a [`QueryDict`].
    pub const fn get(&self) -> &QueryDict {
        &self.get
    }

    /// Returns the POST form parameters as a [`QueryDict`].
    pub const fn post(&self) -> &QueryDict {
        &self.post
    }

    /// Returns the request headers.
    pub const fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// Returns the META dictionary containing server-level metadata.
    ///
    /// Keys include `SERVER_NAME`, `REMOTE_ADDR`, `REQUEST_METHOD`, `HTTP_*` headers, etc.
    pub const fn meta(&self) -> &HashMap<String, String> {
        &self.meta
    }

    /// Returns a mutable reference to the META dictionary.
    ///
    /// This allows middleware to inject server-level metadata such as session data.
    pub fn meta_mut(&mut self) -> &mut HashMap<String, String> {
        &mut self.meta
    }

    /// Returns the raw request body bytes.
    pub fn body(&self) -> &[u8] {
        &self.body
    }

    /// Returns the resolver match information, if the URL has been resolved.
    pub const fn resolver_match(&self) -> Option<&ResolverMatch> {
        self.resolver_match.as_ref()
    }

    /// Sets the resolver match on this request.
    pub fn set_resolver_match(&mut self, resolver_match: ResolverMatch) {
        self.resolver_match = Some(resolver_match);
    }

    /// Returns `true` if the request uses HTTPS.
    ///
    /// Checks the scheme and the `X-Forwarded-Proto` header.
    pub fn is_secure(&self) -> bool {
        self.scheme == "https"
    }

    /// Returns `true` if the request was made via `XMLHttpRequest` (AJAX).
    ///
    /// Checks for the `X-Requested-With: XMLHttpRequest` header.
    pub fn is_ajax(&self) -> bool {
        self.headers
            .get("x-requested-with")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.eq_ignore_ascii_case("xmlhttprequest"))
    }

    /// Returns the host from the `Host` header or META.
    pub fn get_host(&self) -> &str {
        self.meta
            .get("HTTP_HOST")
            .or_else(|| self.meta.get("SERVER_NAME"))
            .map_or("localhost", String::as_str)
    }

    /// Returns the full path including the query string.
    ///
    /// # Examples
    ///
    /// ```
    /// use django_rs_http::HttpRequest;
    ///
    /// let request = HttpRequest::builder()
    ///     .path("/articles/")
    ///     .query_string("page=2")
    ///     .build();
    /// assert_eq!(request.get_full_path(), "/articles/?page=2");
    /// ```
    pub fn get_full_path(&self) -> String {
        if self.query_string.is_empty() {
            self.path.clone()
        } else {
            format!("{}?{}", self.path, self.query_string)
        }
    }

    /// Builds an absolute URI from the current request.
    ///
    /// If `location` is `None`, uses the request's full path. If `location` is
    /// already absolute (starts with `http://` or `https://`), it is returned as-is.
    pub fn build_absolute_uri(&self, location: Option<&str>) -> String {
        match location {
            Some(loc) if loc.starts_with("http://") || loc.starts_with("https://") => {
                loc.to_string()
            }
            Some(loc) => {
                let scheme = &self.scheme;
                let host = self.get_host();
                let path = if loc.starts_with('/') {
                    loc.to_string()
                } else {
                    format!("/{loc}")
                };
                format!("{scheme}://{host}{path}")
            }
            None => {
                let scheme = &self.scheme;
                let host = self.get_host();
                let full_path = self.get_full_path();
                format!("{scheme}://{host}{full_path}")
            }
        }
    }

    /// Returns the URL scheme (`"http"` or `"https"`).
    pub fn scheme(&self) -> &str {
        &self.scheme
    }

    /// Parses cookies from the `Cookie` header and returns them as a map.
    ///
    /// The result is cached after the first call. Cookie header format
    /// is `name1=value1; name2=value2`.
    pub fn cookies(&self) -> &HashMap<String, String> {
        self.cached_cookies.get_or_init(|| {
            self.headers
                .get(http::header::COOKIE)
                .and_then(|v| v.to_str().ok())
                .map_or_else(HashMap::new, cookies::parse_cookie_header)
        })
    }

    /// Gets a specific cookie value by name.
    pub fn cookie(&self, name: &str) -> Option<&str> {
        self.cookies().get(name).map(String::as_str)
    }

    /// Gets and verifies a signed cookie (HMAC-SHA256).
    ///
    /// The cookie value must have been set with `set_signed_cookie` on the
    /// response. The `max_age` parameter (in seconds) is optional; if provided,
    /// the cookie is rejected if it was signed more than `max_age` seconds ago.
    pub fn get_signed_cookie(
        &self,
        name: &str,
        salt: &str,
        secret_key: &str,
        max_age: Option<u64>,
    ) -> Result<String, CookieError> {
        let value = self.cookie(name).ok_or(CookieError::NotFound)?;
        cookies::verify_signed_cookie(value, secret_key, salt, max_age)
    }

    /// Returns the uploaded files parsed from a multipart request body.
    pub const fn files(&self) -> &HashMap<String, Vec<UploadedFile>> {
        &self.files
    }
}

/// Builder for constructing [`HttpRequest`] instances in tests.
///
/// This provides a fluent API for building requests without needing
/// a full Axum request.
#[derive(Debug)]
pub struct HttpRequestBuilder {
    method: Method,
    path: String,
    query_string: String,
    content_type: Option<String>,
    headers: HeaderMap,
    meta: HashMap<String, String>,
    body: Vec<u8>,
    scheme: String,
}

impl Default for HttpRequestBuilder {
    fn default() -> Self {
        Self {
            method: Method::GET,
            path: "/".to_string(),
            query_string: String::new(),
            content_type: None,
            headers: HeaderMap::new(),
            meta: HashMap::new(),
            body: Vec::new(),
            scheme: "http".to_string(),
        }
    }
}

impl HttpRequestBuilder {
    /// Sets the HTTP method.
    #[must_use]
    pub fn method(mut self, method: Method) -> Self {
        self.method = method;
        self
    }

    /// Sets the request path.
    #[must_use]
    pub fn path(mut self, path: &str) -> Self {
        self.path = path.to_string();
        self
    }

    /// Sets the query string (without leading `?`).
    #[must_use]
    pub fn query_string(mut self, qs: &str) -> Self {
        self.query_string = qs.to_string();
        self
    }

    /// Sets the content type.
    #[must_use]
    pub fn content_type(mut self, ct: &str) -> Self {
        self.content_type = Some(ct.to_string());
        self
    }

    /// Adds a header.
    #[must_use]
    pub fn header(mut self, name: &str, value: &str) -> Self {
        if let (Ok(name), Ok(value)) = (
            http::header::HeaderName::from_bytes(name.as_bytes()),
            http::header::HeaderValue::from_str(value),
        ) {
            self.headers.insert(name, value);
        }
        self
    }

    /// Adds a META entry.
    #[must_use]
    pub fn meta(mut self, key: &str, value: &str) -> Self {
        self.meta.insert(key.to_string(), value.to_string());
        self
    }

    /// Sets the request body.
    #[must_use]
    pub fn body(mut self, body: Vec<u8>) -> Self {
        self.body = body;
        self
    }

    /// Sets the scheme (http or https).
    #[must_use]
    pub fn scheme(mut self, scheme: &str) -> Self {
        self.scheme = scheme.to_string();
        self
    }

    /// Builds the [`HttpRequest`].
    pub fn build(self) -> HttpRequest {
        let get = QueryDict::parse(&self.query_string);

        let post = if self
            .content_type
            .as_deref()
            .is_some_and(|ct| ct.starts_with("application/x-www-form-urlencoded"))
        {
            let body_str = String::from_utf8_lossy(&self.body);
            QueryDict::parse(&body_str)
        } else {
            QueryDict::new()
        };

        let path_info = self.path.clone();

        let mut meta = self.meta;
        meta.entry("REQUEST_METHOD".to_string())
            .or_insert_with(|| self.method.to_string());
        meta.entry("PATH_INFO".to_string())
            .or_insert_with(|| path_info.clone());
        meta.entry("QUERY_STRING".to_string())
            .or_insert_with(|| self.query_string.clone());

        // Parse multipart data if content type is multipart/form-data
        let (post, files) = if self
            .content_type
            .as_deref()
            .is_some_and(|ct| ct.starts_with("multipart/form-data"))
        {
            if let Some(boundary) = self.content_type.as_deref().and_then(crate::upload::extract_boundary) {
                match crate::upload::parse_multipart(&self.body, boundary) {
                    Ok(multipart) => {
                        let mut post_dict = QueryDict::new_mutable();
                        for (name, values) in &multipart.fields {
                            for value in values {
                                let _ = post_dict.append(name, value);
                            }
                        }
                        (post_dict, multipart.files)
                    }
                    Err(_) => (post, HashMap::new()),
                }
            } else {
                (post, HashMap::new())
            }
        } else {
            (post, HashMap::new())
        };

        HttpRequest {
            method: self.method,
            path: self.path,
            path_info,
            query_string: self.query_string,
            content_type: self.content_type,
            get,
            post,
            headers: self.headers,
            meta,
            body: self.body,
            resolver_match: None,
            scheme: self.scheme,
            cached_cookies: std::sync::OnceLock::new(),
            files,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_defaults() {
        let req = HttpRequest::builder().build();
        assert_eq!(req.method(), &Method::GET);
        assert_eq!(req.path(), "/");
        assert_eq!(req.query_string(), "");
        assert!(req.content_type().is_none());
        assert!(req.body().is_empty());
        assert!(!req.is_secure());
    }

    #[test]
    fn test_builder_method() {
        let req = HttpRequest::builder()
            .method(Method::POST)
            .build();
        assert_eq!(req.method(), &Method::POST);
    }

    #[test]
    fn test_builder_path_and_query() {
        let req = HttpRequest::builder()
            .path("/articles/")
            .query_string("page=2&sort=date")
            .build();
        assert_eq!(req.path(), "/articles/");
        assert_eq!(req.query_string(), "page=2&sort=date");
        assert_eq!(req.get().get("page"), Some("2"));
        assert_eq!(req.get().get("sort"), Some("date"));
    }

    #[test]
    fn test_get_full_path_no_query() {
        let req = HttpRequest::builder().path("/articles/").build();
        assert_eq!(req.get_full_path(), "/articles/");
    }

    #[test]
    fn test_get_full_path_with_query() {
        let req = HttpRequest::builder()
            .path("/articles/")
            .query_string("page=2")
            .build();
        assert_eq!(req.get_full_path(), "/articles/?page=2");
    }

    #[test]
    fn test_is_secure_false() {
        let req = HttpRequest::builder().build();
        assert!(!req.is_secure());
    }

    #[test]
    fn test_is_secure_true() {
        let req = HttpRequest::builder().scheme("https").build();
        assert!(req.is_secure());
    }

    #[test]
    fn test_is_ajax_false() {
        let req = HttpRequest::builder().build();
        assert!(!req.is_ajax());
    }

    #[test]
    fn test_is_ajax_true() {
        let req = HttpRequest::builder()
            .header("x-requested-with", "XMLHttpRequest")
            .build();
        assert!(req.is_ajax());
    }

    #[test]
    fn test_get_host_default() {
        let req = HttpRequest::builder().build();
        assert_eq!(req.get_host(), "localhost");
    }

    #[test]
    fn test_get_host_from_meta() {
        let req = HttpRequest::builder()
            .meta("HTTP_HOST", "example.com")
            .build();
        assert_eq!(req.get_host(), "example.com");
    }

    #[test]
    fn test_build_absolute_uri_none() {
        let req = HttpRequest::builder()
            .path("/articles/")
            .query_string("page=1")
            .meta("HTTP_HOST", "example.com")
            .build();
        assert_eq!(
            req.build_absolute_uri(None),
            "http://example.com/articles/?page=1"
        );
    }

    #[test]
    fn test_build_absolute_uri_relative() {
        let req = HttpRequest::builder()
            .meta("HTTP_HOST", "example.com")
            .build();
        assert_eq!(
            req.build_absolute_uri(Some("/other/")),
            "http://example.com/other/"
        );
    }

    #[test]
    fn test_build_absolute_uri_absolute() {
        let req = HttpRequest::builder().build();
        assert_eq!(
            req.build_absolute_uri(Some("https://other.com/path")),
            "https://other.com/path"
        );
    }

    #[test]
    fn test_build_absolute_uri_secure() {
        let req = HttpRequest::builder()
            .scheme("https")
            .meta("HTTP_HOST", "example.com")
            .build();
        assert_eq!(
            req.build_absolute_uri(Some("/secure/")),
            "https://example.com/secure/"
        );
    }

    #[test]
    fn test_post_form_data() {
        let body = b"username=alice&password=secret".to_vec();
        let req = HttpRequest::builder()
            .method(Method::POST)
            .content_type("application/x-www-form-urlencoded")
            .body(body)
            .build();
        assert_eq!(req.post().get("username"), Some("alice"));
        assert_eq!(req.post().get("password"), Some("secret"));
    }

    #[test]
    fn test_post_non_form_data() {
        let body = b"{\"key\": \"value\"}".to_vec();
        let req = HttpRequest::builder()
            .method(Method::POST)
            .content_type("application/json")
            .body(body)
            .build();
        assert!(req.post().is_empty());
        assert_eq!(req.body(), b"{\"key\": \"value\"}");
    }

    #[test]
    fn test_content_type() {
        let req = HttpRequest::builder()
            .content_type("text/html")
            .build();
        assert_eq!(req.content_type(), Some("text/html"));
    }

    #[test]
    fn test_path_info() {
        let req = HttpRequest::builder()
            .path("/articles/2024/")
            .build();
        assert_eq!(req.path_info(), "/articles/2024/");
    }

    #[test]
    fn test_scheme() {
        let req = HttpRequest::builder().build();
        assert_eq!(req.scheme(), "http");
    }

    #[test]
    fn test_resolver_match() {
        let mut req = HttpRequest::builder().build();
        assert!(req.resolver_match().is_none());

        let resolver_match = ResolverMatch {
            func: std::sync::Arc::new(|_| Box::pin(async { crate::HttpResponse::ok("ok") })),
            args: Vec::new(),
            kwargs: HashMap::new(),
            url_name: Some("test".to_string()),
            app_names: Vec::new(),
            namespaces: Vec::new(),
            route: "test/".to_string(),
        };
        req.set_resolver_match(resolver_match);
        assert!(req.resolver_match().is_some());
        assert_eq!(req.resolver_match().unwrap().url_name.as_deref(), Some("test"));
    }

    #[test]
    fn test_headers() {
        let req = HttpRequest::builder()
            .header("accept", "text/html")
            .build();
        assert_eq!(
            req.headers().get("accept").unwrap().to_str().unwrap(),
            "text/html"
        );
    }

    #[test]
    fn test_meta() {
        let req = HttpRequest::builder()
            .meta("REMOTE_ADDR", "127.0.0.1")
            .build();
        assert_eq!(req.meta().get("REMOTE_ADDR").unwrap(), "127.0.0.1");
    }

    #[test]
    fn test_from_axum() {
        let request = http::Request::builder()
            .method(Method::GET)
            .uri("http://example.com/articles/?page=1")
            .header("host", "example.com")
            .header("accept", "text/html")
            .body(())
            .unwrap();

        let (parts, ()) = request.into_parts();
        let req = HttpRequest::from_axum(parts, Vec::new());

        assert_eq!(req.method(), &Method::GET);
        assert_eq!(req.path(), "/articles/");
        assert_eq!(req.query_string(), "page=1");
        assert_eq!(req.get().get("page"), Some("1"));
        assert_eq!(req.get_host(), "example.com");
    }

    #[test]
    fn test_from_axum_post() {
        let body = b"name=test&value=123".to_vec();
        let request = http::Request::builder()
            .method(Method::POST)
            .uri("http://example.com/submit/")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(())
            .unwrap();

        let (parts, ()) = request.into_parts();
        let req = HttpRequest::from_axum(parts, body);

        assert_eq!(req.post().get("name"), Some("test"));
        assert_eq!(req.post().get("value"), Some("123"));
    }

    #[test]
    fn test_build_absolute_uri_relative_no_leading_slash() {
        let req = HttpRequest::builder()
            .meta("HTTP_HOST", "example.com")
            .build();
        assert_eq!(
            req.build_absolute_uri(Some("path/")),
            "http://example.com/path/"
        );
    }

    // ── Cookie integration tests ────────────────────────────────────

    #[test]
    fn test_cookies_from_header() {
        let req = HttpRequest::builder()
            .header("cookie", "session=abc123; theme=dark")
            .build();
        let cookies = req.cookies();
        assert_eq!(cookies.get("session"), Some(&"abc123".to_string()));
        assert_eq!(cookies.get("theme"), Some(&"dark".to_string()));
    }

    #[test]
    fn test_cookie_specific() {
        let req = HttpRequest::builder()
            .header("cookie", "token=xyz789")
            .build();
        assert_eq!(req.cookie("token"), Some("xyz789"));
        assert_eq!(req.cookie("missing"), None);
    }

    #[test]
    fn test_cookies_empty_when_no_header() {
        let req = HttpRequest::builder().build();
        assert!(req.cookies().is_empty());
        assert_eq!(req.cookie("anything"), None);
    }

    #[test]
    fn test_signed_cookie_round_trip() {
        use crate::cookies;
        let signed = cookies::sign_cookie_value("my-data", "secret", "salt");
        let req = HttpRequest::builder()
            .header("cookie", &format!("signed={signed}"))
            .build();
        let result = req.get_signed_cookie("signed", "salt", "secret", None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "my-data");
    }

    #[test]
    fn test_signed_cookie_not_found() {
        let req = HttpRequest::builder().build();
        let result = req.get_signed_cookie("missing", "salt", "secret", None);
        assert!(matches!(result, Err(CookieError::NotFound)));
    }

    // ── File upload integration tests ───────────────────────────────

    #[test]
    fn test_files_empty_for_non_multipart() {
        let req = HttpRequest::builder()
            .method(Method::POST)
            .content_type("application/x-www-form-urlencoded")
            .body(b"key=value".to_vec())
            .build();
        assert!(req.files().is_empty());
    }

    #[test]
    fn test_files_from_multipart() {
        let boundary = "boundary123";
        let body = format!(
            "--{boundary}\r\n\
             Content-Disposition: form-data; name=\"field1\"\r\n\
             \r\n\
             value1\r\n\
             --{boundary}\r\n\
             Content-Disposition: form-data; name=\"myfile\"; filename=\"test.txt\"\r\n\
             Content-Type: text/plain\r\n\
             \r\n\
             file content here\r\n\
             --{boundary}--\r\n"
        );
        let req = HttpRequest::builder()
            .method(Method::POST)
            .content_type(&format!("multipart/form-data; boundary={boundary}"))
            .body(body.into_bytes())
            .build();

        // Files should be parsed
        assert!(!req.files().is_empty());
        let files = req.files().get("myfile").unwrap();
        assert_eq!(files[0].name, "test.txt");
        assert_eq!(files[0].content_type, "text/plain");

        // Form fields should be in POST data
        assert_eq!(req.post().get("field1"), Some("value1"));
    }

    #[test]
    fn test_files_multiple() {
        let boundary = "boundary456";
        let body = format!(
            "--{boundary}\r\n\
             Content-Disposition: form-data; name=\"files\"; filename=\"a.txt\"\r\n\
             Content-Type: text/plain\r\n\
             \r\n\
             A content\r\n\
             --{boundary}\r\n\
             Content-Disposition: form-data; name=\"files\"; filename=\"b.txt\"\r\n\
             Content-Type: text/plain\r\n\
             \r\n\
             B content\r\n\
             --{boundary}--\r\n"
        );
        let req = HttpRequest::builder()
            .method(Method::POST)
            .content_type(&format!("multipart/form-data; boundary={boundary}"))
            .body(body.into_bytes())
            .build();

        let files = req.files().get("files").unwrap();
        assert_eq!(files.len(), 2);
    }
}
