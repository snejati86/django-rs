//! Integration tests for the middleware pipeline and built-in middleware.
//!
//! Tests cover: SecurityMiddleware headers, GZipMiddleware compression,
//! CommonMiddleware trailing slash, AuthenticationMiddleware, LoginRequiredMiddleware,
//! MessageMiddleware, LocaleMiddleware, middleware execution order, short-circuit
//! behavior, and full pipeline composition.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use django_rs_core::DjangoError;
use django_rs_http::{HttpRequest, HttpResponse};
use django_rs_views::middleware::builtin::{
    AuthenticationMiddleware, CommonMiddleware, GZipMiddleware, LocaleMiddleware,
    LoginRequiredMiddleware, MessageMiddleware, SecurityMiddleware,
};
use django_rs_views::middleware::{Middleware, MiddlewarePipeline, ViewHandler};

// ── Helper: default view handler ────────────────────────────────────

fn ok_handler() -> ViewHandler {
    Box::new(|_req| Box::pin(async { HttpResponse::ok("OK") }))
}

fn large_body_handler() -> ViewHandler {
    Box::new(|_req| {
        Box::pin(async {
            let body = "x".repeat(500);
            HttpResponse::ok(&body)
        })
    })
}

// ═════════════════════════════════════════════════════════════════════
// 1. Security middleware adds HSTS header
// ═════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_security_middleware_adds_hsts() {
    let mw = SecurityMiddleware {
        hsts_seconds: 31_536_000,
        hsts_include_subdomains: true,
        hsts_preload: true,
        ..Default::default()
    };

    let mut pipeline = MiddlewarePipeline::new();
    pipeline.add(mw);

    let request = HttpRequest::builder().build();
    let response = pipeline.process(request, &ok_handler()).await;

    let hsts = response
        .headers()
        .get("strict-transport-security")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(hsts.contains("max-age=31536000"));
    assert!(hsts.contains("includeSubDomains"));
    assert!(hsts.contains("preload"));
}

// ═════════════════════════════════════════════════════════════════════
// 2. Security middleware adds X-Content-Type-Options
// ═════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_security_middleware_adds_content_type_options() {
    let mut pipeline = MiddlewarePipeline::new();
    pipeline.add(SecurityMiddleware::default());

    let request = HttpRequest::builder().build();
    let response = pipeline.process(request, &ok_handler()).await;

    assert_eq!(
        response
            .headers()
            .get("x-content-type-options")
            .unwrap()
            .to_str()
            .unwrap(),
        "nosniff"
    );
}

// ═════════════════════════════════════════════════════════════════════
// 3. Security middleware adds X-Frame-Options
// ═════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_security_middleware_adds_x_frame_options() {
    let mut pipeline = MiddlewarePipeline::new();
    pipeline.add(SecurityMiddleware::default());

    let request = HttpRequest::builder().build();
    let response = pipeline.process(request, &ok_handler()).await;

    assert_eq!(
        response
            .headers()
            .get("x-frame-options")
            .unwrap()
            .to_str()
            .unwrap(),
        "DENY"
    );
}

// ═════════════════════════════════════════════════════════════════════
// 4. Security middleware adds Referrer-Policy (via X-XSS-Protection)
// ═════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_security_middleware_adds_xss_protection() {
    let mut pipeline = MiddlewarePipeline::new();
    pipeline.add(SecurityMiddleware::default());

    let request = HttpRequest::builder().build();
    let response = pipeline.process(request, &ok_handler()).await;

    assert_eq!(
        response
            .headers()
            .get("x-xss-protection")
            .unwrap()
            .to_str()
            .unwrap(),
        "1; mode=block"
    );
}

// ═════════════════════════════════════════════════════════════════════
// 5. GZip middleware compresses large responses
// ═════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_gzip_compresses_large_responses() {
    let mut pipeline = MiddlewarePipeline::new();
    pipeline.add(GZipMiddleware::default());

    let request = HttpRequest::builder()
        .header("accept-encoding", "gzip, deflate")
        .build();
    let response = pipeline.process(request, &large_body_handler()).await;

    assert_eq!(
        response
            .headers()
            .get(http::header::CONTENT_ENCODING)
            .unwrap()
            .to_str()
            .unwrap(),
        "gzip"
    );
    // Compressed body should be smaller than 500 bytes
    let bytes = response.content_bytes().unwrap();
    assert!(bytes.len() < 500);
}

// ═════════════════════════════════════════════════════════════════════
// 6. GZip middleware skips small responses
// ═════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_gzip_skips_small_responses() {
    let mut pipeline = MiddlewarePipeline::new();
    pipeline.add(GZipMiddleware::default());

    let request = HttpRequest::builder()
        .header("accept-encoding", "gzip")
        .build();
    let response = pipeline.process(request, &ok_handler()).await;

    // "OK" is small; no Content-Encoding header
    assert!(response
        .headers()
        .get(http::header::CONTENT_ENCODING)
        .is_none());
}

// ═════════════════════════════════════════════════════════════════════
// 7. Common middleware appends trailing slash
// ═════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_common_middleware_appends_trailing_slash() {
    let mut pipeline = MiddlewarePipeline::new();
    pipeline.add(CommonMiddleware::default());

    let request = HttpRequest::builder().path("/articles").build();
    let response = pipeline.process(request, &ok_handler()).await;

    assert_eq!(response.status(), http::StatusCode::MOVED_PERMANENTLY);
    let location = response
        .headers()
        .get(http::header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(location, "/articles/");
}

// ═════════════════════════════════════════════════════════════════════
// 8. Authentication middleware sets user on request
// ═════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_auth_middleware_sets_user_on_request() {
    let session_data = serde_json::json!({"_auth_user_id": "42"});
    let mut pipeline = MiddlewarePipeline::new();
    pipeline.add(AuthenticationMiddleware);

    let handler: ViewHandler = Box::new(|req| {
        Box::pin(async move {
            let user_id = req.meta().get("USER_ID").cloned().unwrap_or_default();
            let authed = req
                .meta()
                .get("USER_AUTHENTICATED")
                .cloned()
                .unwrap_or_default();
            HttpResponse::ok(&format!("user={user_id},auth={authed}"))
        })
    });

    let request = HttpRequest::builder()
        .meta("SESSION_DATA", &session_data.to_string())
        .build();
    let response = pipeline.process(request, &handler).await;
    let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
    assert!(body.contains("user=42"));
    assert!(body.contains("auth=true"));
}

// ═════════════════════════════════════════════════════════════════════
// 9. LoginRequired middleware blocks anonymous
// ═════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_login_required_blocks_anonymous() {
    let mut pipeline = MiddlewarePipeline::new();
    pipeline.add(AuthenticationMiddleware);
    pipeline.add(LoginRequiredMiddleware::default());

    let request = HttpRequest::builder().path("/dashboard/").build();
    let response = pipeline.process(request, &ok_handler()).await;

    assert_eq!(response.status(), http::StatusCode::FOUND);
    let location = response
        .headers()
        .get(http::header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.contains("/accounts/login/"));
    assert!(location.contains("next="));
}

// ═════════════════════════════════════════════════════════════════════
// 10. LoginRequired middleware allows authenticated
// ═════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_login_required_allows_authenticated() {
    let session_data = serde_json::json!({"_auth_user_id": "1"});
    let mut pipeline = MiddlewarePipeline::new();
    pipeline.add(AuthenticationMiddleware);
    pipeline.add(LoginRequiredMiddleware::default());

    let request = HttpRequest::builder()
        .path("/dashboard/")
        .meta("SESSION_DATA", &session_data.to_string())
        .build();
    let response = pipeline.process(request, &ok_handler()).await;

    assert_eq!(response.status(), http::StatusCode::OK);
}

// ═════════════════════════════════════════════════════════════════════
// 11. Message middleware adds messages to context
// ═════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_message_middleware_initializes_message_store() {
    let mut pipeline = MiddlewarePipeline::new();
    pipeline.add(MessageMiddleware);

    let handler: ViewHandler = Box::new(|req| {
        Box::pin(async move {
            let has_store = req.meta().contains_key("_messages_store");
            let has_added = req.meta().contains_key("_messages_added");
            HttpResponse::ok(&format!("store={has_store},added={has_added}"))
        })
    });

    let request = HttpRequest::builder().meta("SESSION_DATA", "{}").build();
    let response = pipeline.process(request, &handler).await;
    let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
    assert!(body.contains("store=true"));
    assert!(body.contains("added=true"));
}

// ═════════════════════════════════════════════════════════════════════
// 12. Locale middleware sets language from Accept-Language
// ═════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_locale_middleware_sets_language() {
    let mut pipeline = MiddlewarePipeline::new();
    pipeline.add(LocaleMiddleware {
        default_language: "en".to_string(),
        supported_languages: vec!["en".to_string(), "fr".to_string(), "de".to_string()],
    });

    let handler: ViewHandler = Box::new(|req| {
        Box::pin(async move {
            let lang = req.meta().get("LANGUAGE_CODE").cloned().unwrap_or_default();
            HttpResponse::ok(&format!("lang={lang}"))
        })
    });

    let request = HttpRequest::builder()
        .meta("SESSION_DATA", "{}")
        .header("accept-language", "de-DE,de;q=0.9,en;q=0.8")
        .build();
    let response = pipeline.process(request, &handler).await;

    let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
    assert!(body.contains("lang=de"));

    // Check Content-Language response header
    assert_eq!(
        response
            .headers()
            .get("content-language")
            .unwrap()
            .to_str()
            .unwrap(),
        "de"
    );
    // Check Vary header
    assert_eq!(
        response
            .headers()
            .get(http::header::VARY)
            .unwrap()
            .to_str()
            .unwrap(),
        "Accept-Language"
    );
}

// ═════════════════════════════════════════════════════════════════════
// 13. Middleware execution order (first-in, last-out)
// ═════════════════════════════════════════════════════════════════════

struct OrderTracker {
    name: String,
    request_order: Arc<AtomicUsize>,
    response_order: Arc<AtomicUsize>,
    request_log: Arc<Mutex<Vec<String>>>,
    response_log: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl Middleware for OrderTracker {
    async fn process_request(&self, _request: &mut HttpRequest) -> Option<HttpResponse> {
        let order = self.request_order.fetch_add(1, Ordering::SeqCst);
        self.request_log
            .lock()
            .unwrap()
            .push(format!("{}:{order}", self.name));
        None
    }

    async fn process_response(
        &self,
        _request: &HttpRequest,
        response: HttpResponse,
    ) -> HttpResponse {
        let order = self.response_order.fetch_add(1, Ordering::SeqCst);
        self.response_log
            .lock()
            .unwrap()
            .push(format!("{}:{order}", self.name));
        response
    }

    async fn process_exception(
        &self,
        _request: &HttpRequest,
        _error: &DjangoError,
    ) -> Option<HttpResponse> {
        None
    }
}

#[tokio::test]
async fn test_middleware_execution_order_first_in_last_out() {
    let request_order = Arc::new(AtomicUsize::new(0));
    let response_order = Arc::new(AtomicUsize::new(0));
    let request_log = Arc::new(Mutex::new(Vec::new()));
    let response_log = Arc::new(Mutex::new(Vec::new()));

    let mut pipeline = MiddlewarePipeline::new();
    for name in &["A", "B", "C"] {
        pipeline.add(OrderTracker {
            name: name.to_string(),
            request_order: request_order.clone(),
            response_order: response_order.clone(),
            request_log: request_log.clone(),
            response_log: response_log.clone(),
        });
    }

    let request = HttpRequest::builder().build();
    pipeline.process(request, &ok_handler()).await;

    let req_log = request_log.lock().unwrap();
    assert_eq!(req_log[0], "A:0");
    assert_eq!(req_log[1], "B:1");
    assert_eq!(req_log[2], "C:2");

    let resp_log = response_log.lock().unwrap();
    // Response is processed in reverse order
    assert_eq!(resp_log[0], "C:0");
    assert_eq!(resp_log[1], "B:1");
    assert_eq!(resp_log[2], "A:2");
}

// ═════════════════════════════════════════════════════════════════════
// 14. Middleware short-circuit (early response)
// ═════════════════════════════════════════════════════════════════════

struct ShortCircuitMiddleware;

#[async_trait]
impl Middleware for ShortCircuitMiddleware {
    async fn process_request(&self, _request: &mut HttpRequest) -> Option<HttpResponse> {
        Some(HttpResponse::forbidden("Blocked by middleware"))
    }

    async fn process_response(
        &self,
        _request: &HttpRequest,
        response: HttpResponse,
    ) -> HttpResponse {
        response
    }

    async fn process_exception(
        &self,
        _request: &HttpRequest,
        _error: &DjangoError,
    ) -> Option<HttpResponse> {
        None
    }
}

#[tokio::test]
async fn test_middleware_short_circuit_early_response() {
    let response_log = Arc::new(Mutex::new(Vec::new()));

    let mut pipeline = MiddlewarePipeline::new();
    // A runs, ShortCircuit returns early, C should never run
    pipeline.add(OrderTracker {
        name: "A".to_string(),
        request_order: Arc::new(AtomicUsize::new(0)),
        response_order: Arc::new(AtomicUsize::new(0)),
        request_log: Arc::new(Mutex::new(Vec::new())),
        response_log: response_log.clone(),
    });
    pipeline.add(ShortCircuitMiddleware);
    pipeline.add(OrderTracker {
        name: "C".to_string(),
        request_order: Arc::new(AtomicUsize::new(0)),
        response_order: Arc::new(AtomicUsize::new(0)),
        request_log: Arc::new(Mutex::new(Vec::new())),
        response_log: response_log.clone(),
    });

    let request = HttpRequest::builder().build();
    let response = pipeline.process(request, &ok_handler()).await;

    // Should be blocked
    assert_eq!(response.status(), http::StatusCode::FORBIDDEN);

    // C should NOT have processed the response
    let resp_log = response_log.lock().unwrap();
    assert_eq!(resp_log.len(), 1);
    assert!(resp_log[0].starts_with("A:"));
}

// ═════════════════════════════════════════════════════════════════════
// 15. Full pipeline: Security + GZip + Auth + LoginRequired + Common
// ═════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_full_pipeline_security_auth_common() {
    let session_data = serde_json::json!({"_auth_user_id": "1"});

    let mut pipeline = MiddlewarePipeline::new();
    pipeline.add(SecurityMiddleware {
        hsts_seconds: 31_536_000,
        ..Default::default()
    });
    pipeline.add(AuthenticationMiddleware);
    pipeline.add(LoginRequiredMiddleware::default());
    pipeline.add(CommonMiddleware::default());

    let handler: ViewHandler = Box::new(|req| {
        Box::pin(async move {
            let user_id = req.meta().get("USER_ID").cloned().unwrap_or_default();
            HttpResponse::ok(&format!("Hello user {user_id}"))
        })
    });

    let request = HttpRequest::builder()
        .path("/dashboard/")
        .meta("SESSION_DATA", &session_data.to_string())
        .build();
    let response = pipeline.process(request, &handler).await;

    // Should pass through (authenticated, path has trailing slash)
    assert_eq!(response.status(), http::StatusCode::OK);
    let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
    assert!(body.contains("Hello user 1"));

    // Security headers should be present
    assert!(response.headers().get("x-content-type-options").is_some());
    assert!(response.headers().get("x-frame-options").is_some());
    assert!(response
        .headers()
        .get("strict-transport-security")
        .is_some());
    assert!(response.headers().get("x-xss-protection").is_some());
}
