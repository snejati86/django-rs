//! Context processors.
//!
//! Context processors add variables to the template context automatically
//! based on the current request. They mirror Django's context processors
//! such as `django.template.context_processors.debug`.

use std::collections::HashMap;

use django_rs_http::HttpRequest;

use crate::context::ContextValue;

/// A context processor that adds variables to every template context.
///
/// Implementations inspect the request and return a map of variable names
/// to values that should be available in all templates.
pub trait ContextProcessor: Send + Sync {
    /// Processes the request and returns context variables.
    fn process(&self, request: &HttpRequest) -> HashMap<String, ContextValue>;
}

/// Adds `debug` (bool) and `sql_queries` (list) to the context.
///
/// Only adds the debug variable when `DEBUG=True` in settings.
pub struct DebugContextProcessor;

impl ContextProcessor for DebugContextProcessor {
    fn process(&self, _request: &HttpRequest) -> HashMap<String, ContextValue> {
        let mut ctx = HashMap::new();
        ctx.insert("debug".to_string(), ContextValue::Bool(true));
        ctx.insert("sql_queries".to_string(), ContextValue::List(vec![]));
        ctx
    }
}

/// Adds `STATIC_URL` to the context.
pub struct StaticContextProcessor {
    /// The static URL prefix.
    pub static_url: String,
}

impl StaticContextProcessor {
    /// Creates a new `StaticContextProcessor` with the given URL prefix.
    pub fn new(static_url: impl Into<String>) -> Self {
        Self {
            static_url: static_url.into(),
        }
    }
}

impl Default for StaticContextProcessor {
    fn default() -> Self {
        Self::new("/static/")
    }
}

impl ContextProcessor for StaticContextProcessor {
    fn process(&self, _request: &HttpRequest) -> HashMap<String, ContextValue> {
        let mut ctx = HashMap::new();
        ctx.insert(
            "STATIC_URL".to_string(),
            ContextValue::String(self.static_url.clone()),
        );
        ctx
    }
}

/// Adds `MEDIA_URL` to the context.
pub struct MediaContextProcessor {
    /// The media URL prefix.
    pub media_url: String,
}

impl MediaContextProcessor {
    /// Creates a new `MediaContextProcessor` with the given URL prefix.
    pub fn new(media_url: impl Into<String>) -> Self {
        Self {
            media_url: media_url.into(),
        }
    }
}

impl Default for MediaContextProcessor {
    fn default() -> Self {
        Self::new("/media/")
    }
}

impl ContextProcessor for MediaContextProcessor {
    fn process(&self, _request: &HttpRequest) -> HashMap<String, ContextValue> {
        let mut ctx = HashMap::new();
        ctx.insert(
            "MEDIA_URL".to_string(),
            ContextValue::String(self.media_url.clone()),
        );
        ctx
    }
}

/// Adds `csrf_token` to the context.
///
/// In a real application this would generate a cryptographically secure token.
/// This implementation uses a placeholder for demonstration.
pub struct CsrfContextProcessor;

impl ContextProcessor for CsrfContextProcessor {
    fn process(&self, _request: &HttpRequest) -> HashMap<String, ContextValue> {
        use rand::Rng;
        let token: String = rand::thread_rng()
            .sample_iter(&rand::distributions::Alphanumeric)
            .take(64)
            .map(char::from)
            .collect();

        let mut ctx = HashMap::new();
        ctx.insert("csrf_token".to_string(), ContextValue::String(token));
        ctx
    }
}

/// Adds request-related variables to the context.
///
/// Adds `request` as a dict with `path`, `method`, and `is_secure` fields.
pub struct RequestContextProcessor;

impl ContextProcessor for RequestContextProcessor {
    fn process(&self, request: &HttpRequest) -> HashMap<String, ContextValue> {
        let mut req_dict = HashMap::new();
        req_dict.insert(
            "path".to_string(),
            ContextValue::String(request.path().to_string()),
        );
        req_dict.insert(
            "method".to_string(),
            ContextValue::String(request.method().to_string()),
        );
        req_dict.insert(
            "is_secure".to_string(),
            ContextValue::Bool(request.is_secure()),
        );

        let mut ctx = HashMap::new();
        ctx.insert("request".to_string(), ContextValue::Dict(req_dict));
        ctx
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_request() -> HttpRequest {
        HttpRequest::builder().path("/test/").build()
    }

    #[test]
    fn test_debug_context_processor() {
        let cp = DebugContextProcessor;
        let ctx = cp.process(&make_request());
        assert!(matches!(ctx.get("debug"), Some(ContextValue::Bool(true))));
        assert!(matches!(
            ctx.get("sql_queries"),
            Some(ContextValue::List(_))
        ));
    }

    #[test]
    fn test_static_context_processor() {
        let cp = StaticContextProcessor::new("/static/");
        let ctx = cp.process(&make_request());
        assert_eq!(
            ctx.get("STATIC_URL").unwrap().to_display_string(),
            "/static/"
        );
    }

    #[test]
    fn test_static_context_processor_custom() {
        let cp = StaticContextProcessor::new("/assets/");
        let ctx = cp.process(&make_request());
        assert_eq!(
            ctx.get("STATIC_URL").unwrap().to_display_string(),
            "/assets/"
        );
    }

    #[test]
    fn test_media_context_processor() {
        let cp = MediaContextProcessor::new("/media/");
        let ctx = cp.process(&make_request());
        assert_eq!(ctx.get("MEDIA_URL").unwrap().to_display_string(), "/media/");
    }

    #[test]
    fn test_csrf_context_processor() {
        let cp = CsrfContextProcessor;
        let ctx = cp.process(&make_request());
        let token = ctx.get("csrf_token").unwrap().to_display_string();
        assert_eq!(token.len(), 64);
    }

    #[test]
    fn test_request_context_processor() {
        let cp = RequestContextProcessor;
        let request = HttpRequest::builder().path("/articles/").build();
        let ctx = cp.process(&request);

        if let Some(ContextValue::Dict(req)) = ctx.get("request") {
            assert_eq!(req.get("path").unwrap().to_display_string(), "/articles/");
            assert_eq!(req.get("method").unwrap().to_display_string(), "GET");
        } else {
            panic!("Expected request dict in context");
        }
    }

    #[test]
    fn test_static_context_processor_default() {
        let cp = StaticContextProcessor::default();
        let ctx = cp.process(&make_request());
        assert_eq!(
            ctx.get("STATIC_URL").unwrap().to_display_string(),
            "/static/"
        );
    }

    #[test]
    fn test_media_context_processor_default() {
        let cp = MediaContextProcessor::default();
        let ctx = cp.process(&make_request());
        assert_eq!(ctx.get("MEDIA_URL").unwrap().to_display_string(), "/media/");
    }
}
