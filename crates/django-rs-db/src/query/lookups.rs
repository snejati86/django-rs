//! Query lookups and Q objects for building complex filters.
//!
//! This module provides the [`Lookup`] enum for field-level comparisons and
//! the [`Q`] enum for combining filters with AND, OR, and NOT operators.
//! Together they mirror Django's `Q` objects and lookup expressions.
//!
//! # Examples
//!
//! ```
//! use django_rs_db::query::lookups::{Q, Lookup};
//! use django_rs_db::value::Value;
//!
//! // Simple filter: name = "Alice"
//! let q = Q::filter("name", Lookup::Exact(Value::from("Alice")));
//!
//! // Combining with AND: name = "Alice" AND age > 25
//! let combined = q & Q::filter("age", Lookup::Gt(Value::from(25)));
//!
//! // OR: name = "Alice" OR name = "Bob"
//! let either = Q::filter("name", Lookup::Exact(Value::from("Alice")))
//!     | Q::filter("name", Lookup::Exact(Value::from("Bob")));
//!
//! // NOT: NOT(active = false)
//! let negated = !Q::filter("active", Lookup::Exact(Value::from(false)));
//! ```

use crate::value::Value;
use std::ops;

/// A field-level lookup operation.
///
/// Each variant corresponds to a Django lookup type (e.g., `exact`, `contains`,
/// `gt`, etc.) and produces the appropriate SQL WHERE clause fragment.
#[derive(Debug, Clone, PartialEq)]
pub enum Lookup {
    /// Exact match (`field = value`).
    Exact(Value),
    /// Case-insensitive exact match (`LOWER(field) = LOWER(value)`).
    IExact(Value),
    /// Substring match (`field LIKE '%value%'`).
    Contains(String),
    /// Case-insensitive substring match.
    IContains(String),
    /// Membership test (`field IN (values...)`).
    In(Vec<Value>),
    /// Greater than (`field > value`).
    Gt(Value),
    /// Greater than or equal (`field >= value`).
    Gte(Value),
    /// Less than (`field < value`).
    Lt(Value),
    /// Less than or equal (`field <= value`).
    Lte(Value),
    /// Starts with (`field LIKE 'value%'`).
    StartsWith(String),
    /// Case-insensitive starts with.
    IStartsWith(String),
    /// Ends with (`field LIKE '%value'`).
    EndsWith(String),
    /// Case-insensitive ends with.
    IEndsWith(String),
    /// Range test (`field BETWEEN low AND high`).
    Range(Value, Value),
    /// NULL test (`field IS NULL` or `field IS NOT NULL`).
    IsNull(bool),
    /// Regular expression match.
    Regex(String),
    /// Case-insensitive regular expression match.
    IRegex(String),

    // ── PostgreSQL array lookups ─────────────────────────────────────
    /// Array contains all of the given values (`@>` operator).
    ArrayContains(Vec<Value>),
    /// Array is contained by the given values (`<@` operator).
    ArrayContainedBy(Vec<Value>),
    /// Array overlaps with the given values (`&&` operator).
    ArrayOverlap(Vec<Value>),
    /// Array has the given length (`array_length(col, 1) = n`).
    ArrayLen(usize),

    // ── PostgreSQL hstore lookups ────────────────────────────────────
    /// Hstore has a specific key (`?` operator).
    HasKey(String),
    /// Hstore has all of the given keys (`?&` operator).
    HasKeys(Vec<String>),
    /// Hstore has any of the given keys (`?|` operator).
    HasAnyKeys(Vec<String>),

    // ── PostgreSQL range lookups ─────────────────────────────────────
    /// Range contains the given value or range (`@>` operator).
    RangeContains(Value),
    /// Range is contained by the given range (`<@` operator).
    RangeContainedBy(Value),
    /// Range overlaps with the given range (`&&` operator).
    RangeOverlap(Value),
    /// Range is fully less than the given range (`<<` operator).
    FullyLt(Value),
    /// Range is fully greater than the given range (`>>` operator).
    FullyGt(Value),

    // ── PostgreSQL full-text search lookup ───────────────────────────
    /// Full-text search: matches the column against a tsquery string.
    Search(String),
}

/// A composable query filter, equivalent to Django's `Q` object.
///
/// `Q` objects can be combined using `&` (AND), `|` (OR), and `!` (NOT)
/// operators to build arbitrarily complex WHERE clauses.
#[derive(Debug, Clone, PartialEq)]
pub enum Q {
    /// A single field lookup.
    Filter {
        /// The field name (may use `__` notation for related fields).
        field: String,
        /// The lookup operation.
        lookup: Lookup,
    },
    /// Logical AND of multiple conditions.
    And(Vec<Q>),
    /// Logical OR of multiple conditions.
    Or(Vec<Q>),
    /// Logical negation of a condition.
    Not(Box<Q>),
}

impl Q {
    /// Creates a new filter Q object.
    pub fn filter(field: impl Into<String>, lookup: Lookup) -> Self {
        Self::Filter {
            field: field.into(),
            lookup,
        }
    }

    /// Returns `true` if this is an empty AND (always true).
    pub fn is_empty(&self) -> bool {
        match self {
            Self::And(children) | Self::Or(children) => children.is_empty(),
            _ => false,
        }
    }
}

impl ops::BitAnd for Q {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            // Flatten nested ANDs
            (Self::And(mut left), Self::And(right)) => {
                left.extend(right);
                Self::And(left)
            }
            (Self::And(mut left), other) => {
                left.push(other);
                Self::And(left)
            }
            (other, Self::And(mut right)) => {
                right.insert(0, other);
                Self::And(right)
            }
            (left, right) => Self::And(vec![left, right]),
        }
    }
}

impl ops::BitOr for Q {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            // Flatten nested ORs
            (Self::Or(mut left), Self::Or(right)) => {
                left.extend(right);
                Self::Or(left)
            }
            (Self::Or(mut left), other) => {
                left.push(other);
                Self::Or(left)
            }
            (other, Self::Or(mut right)) => {
                right.insert(0, other);
                Self::Or(right)
            }
            (left, right) => Self::Or(vec![left, right]),
        }
    }
}

impl ops::Not for Q {
    type Output = Self;

    fn not(self) -> Self::Output {
        // Double negation cancellation
        match self {
            Self::Not(inner) => *inner,
            other => Self::Not(Box::new(other)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_filter() {
        let q = Q::filter("name", Lookup::Exact(Value::from("Alice")));
        match &q {
            Q::Filter { field, lookup } => {
                assert_eq!(field, "name");
                assert_eq!(*lookup, Lookup::Exact(Value::String("Alice".to_string())));
            }
            _ => panic!("Expected Filter"),
        }
    }

    #[test]
    fn test_and_operator() {
        let q1 = Q::filter("name", Lookup::Exact(Value::from("Alice")));
        let q2 = Q::filter("age", Lookup::Gt(Value::from(25)));
        let combined = q1 & q2;
        match &combined {
            Q::And(children) => assert_eq!(children.len(), 2),
            _ => panic!("Expected And"),
        }
    }

    #[test]
    fn test_or_operator() {
        let q1 = Q::filter("name", Lookup::Exact(Value::from("Alice")));
        let q2 = Q::filter("name", Lookup::Exact(Value::from("Bob")));
        let combined = q1 | q2;
        match &combined {
            Q::Or(children) => assert_eq!(children.len(), 2),
            _ => panic!("Expected Or"),
        }
    }

    #[test]
    fn test_not_operator() {
        let q = Q::filter("active", Lookup::Exact(Value::from(false)));
        let negated = !q;
        match &negated {
            Q::Not(inner) => match inner.as_ref() {
                Q::Filter { field, .. } => assert_eq!(field, "active"),
                _ => panic!("Expected Filter inside Not"),
            },
            _ => panic!("Expected Not"),
        }
    }

    #[test]
    fn test_double_negation() {
        let q = Q::filter("active", Lookup::Exact(Value::from(true)));
        let double_neg = !!q.clone();
        assert_eq!(double_neg, q);
    }

    #[test]
    fn test_and_flattening() {
        let q1 = Q::filter("a", Lookup::Exact(Value::from(1)));
        let q2 = Q::filter("b", Lookup::Exact(Value::from(2)));
        let q3 = Q::filter("c", Lookup::Exact(Value::from(3)));
        let combined = (q1 & q2) & q3;
        match &combined {
            Q::And(children) => assert_eq!(children.len(), 3),
            _ => panic!("Expected And with 3 children"),
        }
    }

    #[test]
    fn test_or_flattening() {
        let q1 = Q::filter("a", Lookup::Exact(Value::from(1)));
        let q2 = Q::filter("b", Lookup::Exact(Value::from(2)));
        let q3 = Q::filter("c", Lookup::Exact(Value::from(3)));
        let combined = (q1 | q2) | q3;
        match &combined {
            Q::Or(children) => assert_eq!(children.len(), 3),
            _ => panic!("Expected Or with 3 children"),
        }
    }

    #[test]
    fn test_complex_combination() {
        // (name = "Alice" AND age > 25) OR (name = "Bob")
        let q1 = Q::filter("name", Lookup::Exact(Value::from("Alice")));
        let q2 = Q::filter("age", Lookup::Gt(Value::from(25)));
        let q3 = Q::filter("name", Lookup::Exact(Value::from("Bob")));
        let combined = (q1 & q2) | q3;
        match &combined {
            Q::Or(children) => {
                assert_eq!(children.len(), 2);
                assert!(matches!(&children[0], Q::And(_)));
                assert!(matches!(&children[1], Q::Filter { .. }));
            }
            _ => panic!("Expected Or"),
        }
    }

    #[test]
    fn test_lookup_variants() {
        let _ = Lookup::Exact(Value::from(1));
        let _ = Lookup::IExact(Value::from("test"));
        let _ = Lookup::Contains("sub".to_string());
        let _ = Lookup::IContains("sub".to_string());
        let _ = Lookup::In(vec![Value::from(1), Value::from(2)]);
        let _ = Lookup::Gt(Value::from(10));
        let _ = Lookup::Gte(Value::from(10));
        let _ = Lookup::Lt(Value::from(10));
        let _ = Lookup::Lte(Value::from(10));
        let _ = Lookup::StartsWith("pre".to_string());
        let _ = Lookup::IStartsWith("pre".to_string());
        let _ = Lookup::EndsWith("suf".to_string());
        let _ = Lookup::IEndsWith("suf".to_string());
        let _ = Lookup::Range(Value::from(1), Value::from(10));
        let _ = Lookup::IsNull(true);
        let _ = Lookup::Regex("^test".to_string());
        let _ = Lookup::IRegex("^test".to_string());
    }

    #[test]
    fn test_q_is_empty() {
        assert!(Q::And(vec![]).is_empty());
        assert!(Q::Or(vec![]).is_empty());
        assert!(!Q::filter("x", Lookup::Exact(Value::from(1))).is_empty());
    }

    #[test]
    fn test_and_with_or_right() {
        let q1 = Q::filter("a", Lookup::Exact(Value::from(1)));
        let q_and = Q::And(vec![Q::filter("b", Lookup::Exact(Value::from(2)))]);
        let combined = q1 & q_and;
        match &combined {
            Q::And(children) => assert_eq!(children.len(), 2),
            _ => panic!("Expected And"),
        }
    }

    // ── PostgreSQL lookup variant tests ──────────────────────────────

    #[test]
    fn test_array_contains_lookup() {
        let q = Q::filter(
            "tags",
            Lookup::ArrayContains(vec![Value::from("rust"), Value::from("python")]),
        );
        match &q {
            Q::Filter { field, lookup } => {
                assert_eq!(field, "tags");
                assert!(matches!(lookup, Lookup::ArrayContains(_)));
            }
            _ => panic!("Expected Filter"),
        }
    }

    #[test]
    fn test_array_contained_by_lookup() {
        let q = Q::filter(
            "tags",
            Lookup::ArrayContainedBy(vec![Value::from("a"), Value::from("b"), Value::from("c")]),
        );
        assert!(matches!(
            q,
            Q::Filter {
                lookup: Lookup::ArrayContainedBy(_),
                ..
            }
        ));
    }

    #[test]
    fn test_array_overlap_lookup() {
        let q = Q::filter("tags", Lookup::ArrayOverlap(vec![Value::from("x")]));
        assert!(matches!(
            q,
            Q::Filter {
                lookup: Lookup::ArrayOverlap(_),
                ..
            }
        ));
    }

    #[test]
    fn test_array_len_lookup() {
        let q = Q::filter("items", Lookup::ArrayLen(5));
        assert!(matches!(
            q,
            Q::Filter {
                lookup: Lookup::ArrayLen(5),
                ..
            }
        ));
    }

    #[test]
    fn test_has_key_lookup() {
        let q = Q::filter("metadata", Lookup::HasKey("color".to_string()));
        assert!(matches!(
            q,
            Q::Filter {
                lookup: Lookup::HasKey(_),
                ..
            }
        ));
    }

    #[test]
    fn test_has_keys_lookup() {
        let q = Q::filter(
            "metadata",
            Lookup::HasKeys(vec!["color".to_string(), "size".to_string()]),
        );
        assert!(matches!(
            q,
            Q::Filter {
                lookup: Lookup::HasKeys(_),
                ..
            }
        ));
    }

    #[test]
    fn test_has_any_keys_lookup() {
        let q = Q::filter(
            "metadata",
            Lookup::HasAnyKeys(vec!["color".to_string(), "weight".to_string()]),
        );
        assert!(matches!(
            q,
            Q::Filter {
                lookup: Lookup::HasAnyKeys(_),
                ..
            }
        ));
    }

    #[test]
    fn test_range_contains_lookup() {
        let q = Q::filter("age_range", Lookup::RangeContains(Value::from(25)));
        assert!(matches!(
            q,
            Q::Filter {
                lookup: Lookup::RangeContains(_),
                ..
            }
        ));
    }

    #[test]
    fn test_range_overlap_lookup() {
        let q = Q::filter("period", Lookup::RangeOverlap(Value::range(1, 5)));
        assert!(matches!(
            q,
            Q::Filter {
                lookup: Lookup::RangeOverlap(_),
                ..
            }
        ));
    }

    #[test]
    fn test_fully_lt_lookup() {
        let q = Q::filter("period", Lookup::FullyLt(Value::range(10, 20)));
        assert!(matches!(
            q,
            Q::Filter {
                lookup: Lookup::FullyLt(_),
                ..
            }
        ));
    }

    #[test]
    fn test_fully_gt_lookup() {
        let q = Q::filter("period", Lookup::FullyGt(Value::range(1, 5)));
        assert!(matches!(
            q,
            Q::Filter {
                lookup: Lookup::FullyGt(_),
                ..
            }
        ));
    }

    #[test]
    fn test_search_lookup() {
        let q = Q::filter("body", Lookup::Search("django & rust".to_string()));
        assert!(matches!(
            q,
            Q::Filter {
                lookup: Lookup::Search(_),
                ..
            }
        ));
    }
}
