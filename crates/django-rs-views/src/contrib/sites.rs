//! Sites framework.
//!
//! Provides multi-site support through an in-memory registry of sites.
//! Each site has an `id`, `domain`, and `name`. The current site is determined
//! by matching the request's `Host` header against registered domains,
//! or by falling back to the configured `SITE_ID`.
//!
//! This mirrors Django's `django.contrib.sites` framework.
//!
//! ## Quick Start
//!
//! ```
//! use django_rs_views::contrib::sites::{Site, SiteRegistry};
//!
//! let mut registry = SiteRegistry::new();
//! registry.register(Site::new(1, "example.com", "Example Site"));
//! registry.register(Site::new(2, "staging.example.com", "Staging"));
//!
//! let site = registry.get_by_domain("example.com");
//! assert!(site.is_some());
//! assert_eq!(site.unwrap().name, "Example Site");
//! ```

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

use django_rs_http::HttpRequest;

/// Represents a single site in the sites framework.
///
/// Each site has a unique numeric ID, a domain name, and a human-readable name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Site {
    /// The unique site identifier.
    pub id: u64,
    /// The domain name for this site (e.g., "example.com").
    pub domain: String,
    /// The human-readable name for this site.
    pub name: String,
}

impl Site {
    /// Creates a new `Site`.
    pub fn new(id: u64, domain: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id,
            domain: domain.into(),
            name: name.into(),
        }
    }
}

/// An in-memory registry of sites.
///
/// Sites can be looked up by ID or domain name. A default site ID can be
/// configured as a fallback.
#[derive(Debug, Clone)]
pub struct SiteRegistry {
    sites: HashMap<u64, Site>,
    domain_index: HashMap<String, u64>,
    default_site_id: u64,
}

impl Default for SiteRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SiteRegistry {
    /// Creates a new empty site registry with a default site ID of 1.
    pub fn new() -> Self {
        Self {
            sites: HashMap::new(),
            domain_index: HashMap::new(),
            default_site_id: 1,
        }
    }

    /// Sets the default site ID (equivalent to Django's `SITE_ID` setting).
    pub fn set_default_site_id(&mut self, id: u64) {
        self.default_site_id = id;
    }

    /// Returns the default site ID.
    pub fn default_site_id(&self) -> u64 {
        self.default_site_id
    }

    /// Registers a site. Overwrites any existing site with the same ID.
    pub fn register(&mut self, site: Site) {
        self.domain_index.insert(site.domain.clone(), site.id);
        self.sites.insert(site.id, site);
    }

    /// Removes a site by ID.
    pub fn unregister(&mut self, id: u64) -> Option<Site> {
        if let Some(site) = self.sites.remove(&id) {
            self.domain_index.remove(&site.domain);
            Some(site)
        } else {
            None
        }
    }

    /// Looks up a site by its ID.
    pub fn get_by_id(&self, id: u64) -> Option<&Site> {
        self.sites.get(&id)
    }

    /// Looks up a site by its domain name.
    pub fn get_by_domain(&self, domain: &str) -> Option<&Site> {
        self.domain_index
            .get(domain)
            .and_then(|id| self.sites.get(id))
    }

    /// Returns the current site based on the request host.
    ///
    /// Checks the request's `Host` header against registered domains.
    /// If no match is found, falls back to the default site ID.
    /// Returns `None` only if neither the domain nor the default ID matches.
    pub fn get_current_site(&self, request: &HttpRequest) -> Option<&Site> {
        let host = request.get_host();
        // Strip port number if present
        let domain = host.split(':').next().unwrap_or(host);

        self.get_by_domain(domain)
            .or_else(|| self.get_by_id(self.default_site_id))
    }

    /// Returns the number of registered sites.
    pub fn len(&self) -> usize {
        self.sites.len()
    }

    /// Returns `true` if no sites are registered.
    pub fn is_empty(&self) -> bool {
        self.sites.is_empty()
    }

    /// Returns all registered sites.
    pub fn all(&self) -> Vec<&Site> {
        self.sites.values().collect()
    }

    /// Clears all registered sites.
    pub fn clear(&mut self) {
        self.sites.clear();
        self.domain_index.clear();
    }
}

// ── Global registry ─────────────────────────────────────────────────────

/// Returns the global site registry singleton.
pub fn global_site_registry() -> &'static RwLock<SiteRegistry> {
    static REGISTRY: OnceLock<RwLock<SiteRegistry>> = OnceLock::new();
    REGISTRY.get_or_init(|| RwLock::new(SiteRegistry::new()))
}

/// Convenience: registers a site in the global registry.
pub fn register_site(site: Site) {
    let mut registry = global_site_registry()
        .write()
        .expect("site registry lock poisoned");
    registry.register(site);
}

/// Convenience: returns the current site from the global registry.
pub fn get_current_site(request: &HttpRequest) -> Option<Site> {
    let registry = global_site_registry()
        .read()
        .expect("site registry lock poisoned");
    registry.get_current_site(request).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_request(host: &str) -> HttpRequest {
        HttpRequest::builder()
            .meta("HTTP_HOST", host)
            .build()
    }

    #[test]
    fn test_site_new() {
        let site = Site::new(1, "example.com", "Example");
        assert_eq!(site.id, 1);
        assert_eq!(site.domain, "example.com");
        assert_eq!(site.name, "Example");
    }

    #[test]
    fn test_registry_new_is_empty() {
        let registry = SiteRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
        assert_eq!(registry.default_site_id(), 1);
    }

    #[test]
    fn test_registry_register_and_get() {
        let mut registry = SiteRegistry::new();
        registry.register(Site::new(1, "example.com", "Example"));

        assert_eq!(registry.len(), 1);
        assert!(!registry.is_empty());

        let site = registry.get_by_id(1).unwrap();
        assert_eq!(site.domain, "example.com");

        let site = registry.get_by_domain("example.com").unwrap();
        assert_eq!(site.id, 1);
    }

    #[test]
    fn test_registry_get_missing() {
        let registry = SiteRegistry::new();
        assert!(registry.get_by_id(999).is_none());
        assert!(registry.get_by_domain("missing.com").is_none());
    }

    #[test]
    fn test_registry_overwrite() {
        let mut registry = SiteRegistry::new();
        registry.register(Site::new(1, "old.com", "Old"));
        registry.register(Site::new(1, "new.com", "New"));

        assert_eq!(registry.len(), 1);
        let site = registry.get_by_id(1).unwrap();
        assert_eq!(site.domain, "new.com");
        assert_eq!(site.name, "New");
    }

    #[test]
    fn test_registry_unregister() {
        let mut registry = SiteRegistry::new();
        registry.register(Site::new(1, "example.com", "Example"));
        let removed = registry.unregister(1);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().domain, "example.com");
        assert!(registry.is_empty());
        assert!(registry.get_by_domain("example.com").is_none());
    }

    #[test]
    fn test_registry_unregister_missing() {
        let mut registry = SiteRegistry::new();
        assert!(registry.unregister(999).is_none());
    }

    #[test]
    fn test_get_current_site_by_host() {
        let mut registry = SiteRegistry::new();
        registry.register(Site::new(1, "example.com", "Example"));
        registry.register(Site::new(2, "other.com", "Other"));

        let request = make_request("other.com");
        let site = registry.get_current_site(&request).unwrap();
        assert_eq!(site.id, 2);
        assert_eq!(site.domain, "other.com");
    }

    #[test]
    fn test_get_current_site_strips_port() {
        let mut registry = SiteRegistry::new();
        registry.register(Site::new(1, "example.com", "Example"));

        let request = make_request("example.com:8080");
        let site = registry.get_current_site(&request).unwrap();
        assert_eq!(site.domain, "example.com");
    }

    #[test]
    fn test_get_current_site_falls_back_to_default() {
        let mut registry = SiteRegistry::new();
        registry.register(Site::new(1, "example.com", "Example"));

        let request = make_request("unknown.com");
        let site = registry.get_current_site(&request).unwrap();
        assert_eq!(site.id, 1); // default site ID is 1
    }

    #[test]
    fn test_get_current_site_custom_default() {
        let mut registry = SiteRegistry::new();
        registry.set_default_site_id(5);
        registry.register(Site::new(5, "default.com", "Default"));

        let request = make_request("unknown.com");
        let site = registry.get_current_site(&request).unwrap();
        assert_eq!(site.id, 5);
    }

    #[test]
    fn test_get_current_site_no_match() {
        let registry = SiteRegistry::new();
        let request = make_request("unknown.com");
        assert!(registry.get_current_site(&request).is_none());
    }

    #[test]
    fn test_registry_all() {
        let mut registry = SiteRegistry::new();
        registry.register(Site::new(1, "a.com", "A"));
        registry.register(Site::new(2, "b.com", "B"));

        let all = registry.all();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_registry_clear() {
        let mut registry = SiteRegistry::new();
        registry.register(Site::new(1, "a.com", "A"));
        registry.register(Site::new(2, "b.com", "B"));
        registry.clear();
        assert!(registry.is_empty());
    }

    #[test]
    fn test_multiple_sites() {
        let mut registry = SiteRegistry::new();
        registry.register(Site::new(1, "example.com", "Example"));
        registry.register(Site::new(2, "staging.example.com", "Staging"));
        registry.register(Site::new(3, "dev.example.com", "Dev"));

        assert_eq!(registry.len(), 3);
        assert_eq!(
            registry.get_by_domain("staging.example.com").unwrap().name,
            "Staging"
        );
    }

    #[test]
    fn test_site_equality() {
        let a = Site::new(1, "example.com", "Example");
        let b = Site::new(1, "example.com", "Example");
        let c = Site::new(2, "example.com", "Example");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn test_site_clone() {
        let site = Site::new(1, "example.com", "Example");
        let cloned = site.clone();
        assert_eq!(site, cloned);
    }

    #[test]
    fn test_global_registry_access() {
        let registry = global_site_registry();
        let _guard = registry.read().unwrap();
        // Just verify we can access without panic
    }

    #[test]
    fn test_default_impl() {
        let registry = SiteRegistry::default();
        assert!(registry.is_empty());
        assert_eq!(registry.default_site_id(), 1);
    }
}
