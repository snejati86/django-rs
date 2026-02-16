//! PostgreSQL full-text search expressions.
//!
//! This module provides types for PostgreSQL full-text search operations:
//!
//! - [`SearchVector`] — wraps one or more columns as `tsvector` for full-text indexing
//! - [`SearchQuery`] — wraps a search query string as `tsquery`
//! - [`SearchRank`] — computes relevance of a `tsvector` against a `tsquery`
//! - [`TrigramSimilarity`] — computes trigram similarity between two strings
//!
//! These map to Django's `SearchVector`, `SearchQuery`, `SearchRank`, and
//! `TrigramSimilarity` from `django.contrib.postgres.search`.

use super::core::Expression;
use crate::value::Value;

/// A PostgreSQL `tsvector` expression built from one or more model columns.
///
/// This corresponds to Django's `SearchVector`. Each column can optionally
/// be assigned a weight (A, B, C, D) for ranking purposes.
///
/// # Example
///
/// ```
/// use django_rs_db::query::expressions::search::SearchVector;
///
/// let sv = SearchVector::new(vec!["title", "body"])
///     .config("english")
///     .weight("A");
/// let expr = sv.to_expression();
/// ```
#[derive(Debug, Clone)]
pub struct SearchVector {
    /// The columns to include in the tsvector.
    columns: Vec<String>,
    /// Optional text search configuration (e.g., "english", "simple").
    config: Option<String>,
    /// Optional weight label (A, B, C, or D).
    weight: Option<String>,
}

impl SearchVector {
    /// Creates a new `SearchVector` from the given columns.
    pub fn new(columns: Vec<&str>) -> Self {
        Self {
            columns: columns.into_iter().map(String::from).collect(),
            config: None,
            weight: None,
        }
    }

    /// Sets the text search configuration.
    #[must_use]
    pub fn config(mut self, config: &str) -> Self {
        self.config = Some(config.to_string());
        self
    }

    /// Sets the weight for ranking (A, B, C, or D).
    #[must_use]
    pub fn weight(mut self, weight: &str) -> Self {
        self.weight = Some(weight.to_string());
        self
    }

    /// Converts to an Expression for use in queries.
    pub fn to_expression(&self) -> Expression {
        let config_arg = self
            .config
            .as_ref()
            .map_or(String::new(), |c| format!("'{c}', "));

        let col_parts: Vec<String> = self
            .columns
            .iter()
            .map(|col| format!("to_tsvector({config_arg}\"{col}\")"))
            .collect();

        let mut raw_sql = col_parts.join(" || ");

        if let Some(ref w) = self.weight {
            raw_sql = format!("setweight({raw_sql}, '{w}')");
        }

        Expression::RawSQL(raw_sql, vec![])
    }
}

/// A PostgreSQL `tsquery` expression built from a search string.
///
/// This corresponds to Django's `SearchQuery`. It supports different
/// search types and configurations.
///
/// # Example
///
/// ```
/// use django_rs_db::query::expressions::search::{SearchQuery, SearchQueryType};
///
/// let sq = SearchQuery::new("rust web framework")
///     .config("english")
///     .search_type(SearchQueryType::Websearch);
/// let expr = sq.to_expression();
/// ```
#[derive(Debug, Clone)]
pub struct SearchQuery {
    /// The search query string.
    query: String,
    /// Optional text search configuration.
    config: Option<String>,
    /// The type of query parsing to use.
    search_type: SearchQueryType,
}

/// The type of tsquery parsing function to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchQueryType {
    /// `plainto_tsquery` — splits on spaces, ANDs terms.
    Plain,
    /// `phraseto_tsquery` — proximity search (terms must be adjacent).
    Phrase,
    /// `to_tsquery` — expects pre-formatted tsquery syntax.
    Raw,
    /// `websearch_to_tsquery` — web-style search syntax (PostgreSQL 11+).
    Websearch,
}

impl SearchQuery {
    /// Creates a new `SearchQuery` with the given query string.
    pub fn new(query: &str) -> Self {
        Self {
            query: query.to_string(),
            config: None,
            search_type: SearchQueryType::Plain,
        }
    }

    /// Sets the text search configuration.
    #[must_use]
    pub fn config(mut self, config: &str) -> Self {
        self.config = Some(config.to_string());
        self
    }

    /// Sets the search type.
    #[must_use]
    pub fn search_type(mut self, search_type: SearchQueryType) -> Self {
        self.search_type = search_type;
        self
    }

    /// Converts to an Expression for use in queries.
    pub fn to_expression(&self) -> Expression {
        let func = match self.search_type {
            SearchQueryType::Plain => "plainto_tsquery",
            SearchQueryType::Phrase => "phraseto_tsquery",
            SearchQueryType::Raw => "to_tsquery",
            SearchQueryType::Websearch => "websearch_to_tsquery",
        };

        let config_arg = self
            .config
            .as_ref()
            .map_or(String::new(), |c| format!("'{c}', "));

        // Use a parameter placeholder for the query string to prevent SQL injection.
        Expression::RawSQL(
            format!("{func}({config_arg}${{PARAM}})"),
            vec![Value::String(self.query.clone())],
        )
    }
}

/// Computes the relevance ranking of a `tsvector` against a `tsquery`.
///
/// This corresponds to Django's `SearchRank`. It produces a `ts_rank` or
/// `ts_rank_cd` function call.
///
/// # Example
///
/// ```
/// use django_rs_db::query::expressions::search::{SearchVector, SearchQuery, SearchRank};
///
/// let vector = SearchVector::new(vec!["title", "body"]);
/// let query = SearchQuery::new("rust");
/// let rank = SearchRank::new(vector, query);
/// let expr = rank.to_expression();
/// ```
#[derive(Debug, Clone)]
pub struct SearchRank {
    /// The search vector expression.
    vector: SearchVector,
    /// The search query expression.
    query: SearchQuery,
    /// Optional weights array for the four weight classes (D, C, B, A).
    weights: Option<[f32; 4]>,
    /// Whether to use cover density ranking (`ts_rank_cd`).
    cover_density: bool,
}

impl SearchRank {
    /// Creates a new `SearchRank`.
    pub fn new(vector: SearchVector, query: SearchQuery) -> Self {
        Self {
            vector,
            query,
            weights: None,
            cover_density: false,
        }
    }

    /// Sets custom weights for the four weight classes (D, C, B, A).
    #[must_use]
    pub fn weights(mut self, weights: [f32; 4]) -> Self {
        self.weights = Some(weights);
        self
    }

    /// Uses cover density ranking (`ts_rank_cd` instead of `ts_rank`).
    #[must_use]
    pub fn cover_density(mut self) -> Self {
        self.cover_density = true;
        self
    }

    /// Converts to an Expression for use in queries.
    pub fn to_expression(&self) -> Expression {
        let func = if self.cover_density {
            "ts_rank_cd"
        } else {
            "ts_rank"
        };

        let vector_expr = self.vector.to_expression();
        let query_expr = self.query.to_expression();

        let vector_sql = match &vector_expr {
            Expression::RawSQL(s, _) => s.clone(),
            _ => String::new(),
        };
        let (query_sql, query_params) = match &query_expr {
            Expression::RawSQL(s, p) => (s.clone(), p.clone()),
            _ => (String::new(), vec![]),
        };

        if let Some(w) = &self.weights {
            Expression::RawSQL(
                format!(
                    "{func}('{{{}, {}, {}, {}}}', {vector_sql}, {query_sql})",
                    w[0], w[1], w[2], w[3]
                ),
                query_params,
            )
        } else {
            Expression::RawSQL(format!("{func}({vector_sql}, {query_sql})"), query_params)
        }
    }
}

/// Computes the trigram similarity between a column and a string.
///
/// This corresponds to Django's `TrigramSimilarity` from
/// `django.contrib.postgres.search`. It requires the `pg_trgm` extension.
///
/// # Example
///
/// ```
/// use django_rs_db::query::expressions::search::TrigramSimilarity;
///
/// let sim = TrigramSimilarity::new("name", "Rust");
/// let expr = sim.to_expression();
/// ```
#[derive(Debug, Clone)]
pub struct TrigramSimilarity {
    /// The column to compare.
    column: String,
    /// The string to compare against.
    string: String,
}

impl TrigramSimilarity {
    /// Creates a new `TrigramSimilarity`.
    pub fn new(column: &str, string: &str) -> Self {
        Self {
            column: column.to_string(),
            string: string.to_string(),
        }
    }

    /// Converts to an Expression for use in queries.
    pub fn to_expression(&self) -> Expression {
        Expression::RawSQL(
            format!("similarity(\"{}\", ${{PARAM}})", self.column),
            vec![Value::String(self.string.clone())],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_vector_single_column() {
        let sv = SearchVector::new(vec!["title"]);
        let expr = sv.to_expression();
        match &expr {
            Expression::RawSQL(sql, _) => {
                assert_eq!(sql, "to_tsvector(\"title\")");
            }
            _ => panic!("Expected RawSQL"),
        }
    }

    #[test]
    fn test_search_vector_multiple_columns() {
        let sv = SearchVector::new(vec!["title", "body"]);
        let expr = sv.to_expression();
        match &expr {
            Expression::RawSQL(sql, _) => {
                assert_eq!(sql, "to_tsvector(\"title\") || to_tsvector(\"body\")");
            }
            _ => panic!("Expected RawSQL"),
        }
    }

    #[test]
    fn test_search_vector_with_config() {
        let sv = SearchVector::new(vec!["title"]).config("english");
        let expr = sv.to_expression();
        match &expr {
            Expression::RawSQL(sql, _) => {
                assert_eq!(sql, "to_tsvector('english', \"title\")");
            }
            _ => panic!("Expected RawSQL"),
        }
    }

    #[test]
    fn test_search_vector_with_weight() {
        let sv = SearchVector::new(vec!["title"]).weight("A");
        let expr = sv.to_expression();
        match &expr {
            Expression::RawSQL(sql, _) => {
                assert_eq!(sql, "setweight(to_tsvector(\"title\"), 'A')");
            }
            _ => panic!("Expected RawSQL"),
        }
    }

    #[test]
    fn test_search_query_plain() {
        let sq = SearchQuery::new("rust web");
        let expr = sq.to_expression();
        match &expr {
            Expression::RawSQL(sql, params) => {
                assert_eq!(sql, "plainto_tsquery(${PARAM})");
                assert_eq!(params.len(), 1);
            }
            _ => panic!("Expected RawSQL"),
        }
    }

    #[test]
    fn test_search_query_websearch() {
        let sq = SearchQuery::new("rust OR python").search_type(SearchQueryType::Websearch);
        let expr = sq.to_expression();
        match &expr {
            Expression::RawSQL(sql, _) => {
                assert_eq!(sql, "websearch_to_tsquery(${PARAM})");
            }
            _ => panic!("Expected RawSQL"),
        }
    }

    #[test]
    fn test_search_query_with_config() {
        let sq = SearchQuery::new("test").config("simple");
        let expr = sq.to_expression();
        match &expr {
            Expression::RawSQL(sql, _) => {
                assert_eq!(sql, "plainto_tsquery('simple', ${PARAM})");
            }
            _ => panic!("Expected RawSQL"),
        }
    }

    #[test]
    fn test_search_query_raw() {
        let sq = SearchQuery::new("rust & web").search_type(SearchQueryType::Raw);
        let expr = sq.to_expression();
        match &expr {
            Expression::RawSQL(sql, _) => {
                assert_eq!(sql, "to_tsquery(${PARAM})");
            }
            _ => panic!("Expected RawSQL"),
        }
    }

    #[test]
    fn test_search_query_phrase() {
        let sq = SearchQuery::new("rust web").search_type(SearchQueryType::Phrase);
        let expr = sq.to_expression();
        match &expr {
            Expression::RawSQL(sql, _) => {
                assert_eq!(sql, "phraseto_tsquery(${PARAM})");
            }
            _ => panic!("Expected RawSQL"),
        }
    }

    #[test]
    fn test_search_rank_basic() {
        let sv = SearchVector::new(vec!["title"]);
        let sq = SearchQuery::new("rust");
        let rank = SearchRank::new(sv, sq);
        let expr = rank.to_expression();
        match &expr {
            Expression::RawSQL(sql, params) => {
                assert!(sql.starts_with("ts_rank("));
                assert!(sql.contains("to_tsvector(\"title\")"));
                assert!(sql.contains("plainto_tsquery("));
                assert_eq!(params.len(), 1);
            }
            _ => panic!("Expected RawSQL"),
        }
    }

    #[test]
    fn test_search_rank_cover_density() {
        let sv = SearchVector::new(vec!["body"]);
        let sq = SearchQuery::new("test");
        let rank = SearchRank::new(sv, sq).cover_density();
        let expr = rank.to_expression();
        match &expr {
            Expression::RawSQL(sql, _) => {
                assert!(sql.starts_with("ts_rank_cd("));
            }
            _ => panic!("Expected RawSQL"),
        }
    }

    #[test]
    fn test_search_rank_with_weights() {
        let sv = SearchVector::new(vec!["title"]);
        let sq = SearchQuery::new("test");
        let rank = SearchRank::new(sv, sq).weights([0.1, 0.2, 0.4, 1.0]);
        let expr = rank.to_expression();
        match &expr {
            Expression::RawSQL(sql, _) => {
                assert!(sql.contains("{0.1, 0.2, 0.4, 1}"));
            }
            _ => panic!("Expected RawSQL"),
        }
    }

    #[test]
    fn test_trigram_similarity() {
        let sim = TrigramSimilarity::new("name", "Django");
        let expr = sim.to_expression();
        match &expr {
            Expression::RawSQL(sql, params) => {
                assert_eq!(sql, "similarity(\"name\", ${PARAM})");
                assert_eq!(params.len(), 1);
                assert_eq!(params[0], Value::String("Django".to_string()));
            }
            _ => panic!("Expected RawSQL"),
        }
    }
}
