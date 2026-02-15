//! Pagination utilities for django-rs.
//!
//! Provides [`Paginator`] and [`Page`] types that mirror Django's
//! `django.core.paginator` module. These are used by generic views
//! like `ListView` to split large querysets into pages.
//!
//! # Examples
//!
//! ```
//! use django_rs_views::pagination::Paginator;
//!
//! let items: Vec<i32> = (1..=100).collect();
//! let paginator = Paginator::new(items, 10);
//! assert_eq!(paginator.num_pages(), 10);
//! assert_eq!(paginator.count(), 100);
//!
//! let page = paginator.page(1).unwrap();
//! assert_eq!(page.object_list().len(), 10);
//! assert!(page.has_next());
//! assert!(!page.has_previous());
//! ```

use std::fmt;
use std::ops::RangeInclusive;

/// Errors that can occur during pagination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaginationError {
    /// The requested page is empty (has no items).
    EmptyPage,
    /// The page number was not a valid integer.
    PageNotAnInteger,
    /// The page number is invalid (e.g., zero or negative).
    InvalidPage(String),
}

impl fmt::Display for PaginationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyPage => write!(f, "That page contains no results"),
            Self::PageNotAnInteger => write!(f, "That page number is not an integer"),
            Self::InvalidPage(msg) => write!(f, "Invalid page: {msg}"),
        }
    }
}

impl std::error::Error for PaginationError {}

/// An item in an elided page range: either a page number or an ellipsis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageRangeItem {
    /// A page number.
    Page(usize),
    /// An ellipsis (gap in the page range).
    Ellipsis,
}

/// Splits a collection of objects into pages.
///
/// Mirrors Django's `django.core.paginator.Paginator`.
pub struct Paginator<T> {
    object_list: Vec<T>,
    per_page: usize,
    orphans: usize,
    allow_empty_first_page: bool,
}

impl<T: Clone> Paginator<T> {
    /// Creates a new `Paginator` with the given objects and page size.
    ///
    /// By default, orphans is 0 and empty first pages are allowed.
    pub fn new(object_list: Vec<T>, per_page: usize) -> Self {
        Self {
            object_list,
            per_page: per_page.max(1),
            orphans: 0,
            allow_empty_first_page: true,
        }
    }

    /// Sets the number of orphans.
    ///
    /// When the last page has fewer than `orphans` items, those items
    /// are added to the previous page instead of creating a tiny last page.
    #[must_use]
    pub fn orphans(mut self, orphans: usize) -> Self {
        self.orphans = orphans;
        self
    }

    /// Sets whether the first page is allowed to be empty.
    #[must_use]
    pub fn allow_empty_first_page(mut self, allow: bool) -> Self {
        self.allow_empty_first_page = allow;
        self
    }

    /// Returns the total number of objects across all pages.
    pub fn count(&self) -> usize {
        self.object_list.len()
    }

    /// Returns the total number of pages.
    pub fn num_pages(&self) -> usize {
        let count = self.count();
        if count == 0 {
            return usize::from(self.allow_empty_first_page);
        }

        // hits = max(1, count - orphans)
        let hits = if count > self.orphans {
            count - self.orphans
        } else {
            1
        };

        hits.div_ceil(self.per_page)
    }

    /// Returns the range of valid page numbers (1-indexed, inclusive).
    pub fn page_range(&self) -> RangeInclusive<usize> {
        1..=self.num_pages()
    }

    /// Returns the requested page (1-indexed).
    ///
    /// # Errors
    ///
    /// Returns `PaginationError::InvalidPage` if the page number is 0,
    /// and `PaginationError::EmptyPage` if the page is beyond the last page.
    pub fn page(&self, number: usize) -> Result<Page<T>, PaginationError> {
        if number == 0 {
            return Err(PaginationError::InvalidPage(
                "Page number must be >= 1".to_string(),
            ));
        }

        let num_pages = self.num_pages();

        if number > num_pages {
            if num_pages == 0 && !self.allow_empty_first_page {
                return Err(PaginationError::EmptyPage);
            }
            return Err(PaginationError::EmptyPage);
        }

        let start = (number - 1) * self.per_page;
        let end = if number == num_pages {
            // Last page gets all remaining items (including orphans)
            self.count()
        } else {
            (start + self.per_page).min(self.count())
        };

        let object_list = self.object_list[start..end].to_vec();

        Ok(Page {
            object_list,
            number,
            num_pages,
            per_page: self.per_page,
        })
    }

    /// Returns the requested page, handling invalid page numbers gracefully.
    ///
    /// If `number` is 0 or below, returns the first page.
    /// If `number` is above the last page, returns the last page.
    pub fn get_page(&self, number: usize) -> Page<T> {
        let num_pages = self.num_pages();

        if number == 0 {
            return self.page(1).unwrap_or_else(|_| Page {
                object_list: Vec::new(),
                number: 1,
                num_pages,

                per_page: self.per_page,
            });
        }

        if number > num_pages {
            let target = num_pages.max(1);
            return self.page(target).unwrap_or_else(|_| Page {
                object_list: Vec::new(),
                number: target,
                num_pages,

                per_page: self.per_page,
            });
        }

        self.page(number).unwrap_or_else(|_| Page {
            object_list: Vec::new(),
            number,
            num_pages,
            per_page: self.per_page,
        })
    }

    /// Returns an elided page range around the given page number.
    ///
    /// Shows `on_each_side` pages on each side of the current page,
    /// and `on_ends` pages at the start and end of the range, with
    /// ellipsis markers for gaps.
    ///
    /// Mirrors Django's `Paginator.get_elided_page_range`.
    pub fn get_elided_page_range(
        &self,
        number: usize,
        on_each_side: usize,
        on_ends: usize,
    ) -> Vec<PageRangeItem> {
        let num_pages = self.num_pages();
        if num_pages == 0 {
            return vec![PageRangeItem::Page(1)];
        }

        let number = number.clamp(1, num_pages);
        let mut result = Vec::new();

        // Calculate the window around the current page
        let window_start = if number > on_each_side + 1 {
            number - on_each_side
        } else {
            1
        };
        let window_end = (number + on_each_side).min(num_pages);

        // If the total range is small enough, show all pages
        if num_pages <= (on_each_side * 2) + (on_ends * 2) + 2 {
            for i in 1..=num_pages {
                result.push(PageRangeItem::Page(i));
            }
            return result;
        }

        // Pages at the start
        let start_end = on_ends.min(num_pages);
        for i in 1..=start_end {
            result.push(PageRangeItem::Page(i));
        }

        // Ellipsis before the window (if there's a gap)
        if window_start > start_end + 1 {
            result.push(PageRangeItem::Ellipsis);
        } else if window_start == start_end + 1 {
            // No gap, but we might have already added this page
            // Only add if not already present
        }

        // Window around current page
        for i in window_start..=window_end {
            if i > start_end {
                result.push(PageRangeItem::Page(i));
            }
        }

        // Ellipsis after the window (if there's a gap)
        let end_start = if num_pages > on_ends {
            num_pages - on_ends + 1
        } else {
            1
        };

        if window_end < end_start - 1 {
            result.push(PageRangeItem::Ellipsis);
        }

        // Pages at the end
        for i in end_start..=num_pages {
            if i > window_end {
                result.push(PageRangeItem::Page(i));
            }
        }

        result
    }
}

/// A single page of results from a [`Paginator`].
#[derive(Debug, Clone)]
#[allow(clippy::struct_field_names)]
pub struct Page<T> {
    object_list: Vec<T>,
    number: usize,
    num_pages: usize,
    per_page: usize,
}

impl<T: Clone> Page<T> {
    /// Returns the items on this page.
    pub fn object_list(&self) -> &[T] {
        &self.object_list
    }

    /// Returns the 1-based page number.
    pub fn number(&self) -> usize {
        self.number
    }

    /// Returns `true` if there is a next page.
    pub fn has_next(&self) -> bool {
        self.number < self.num_pages
    }

    /// Returns `true` if there is a previous page.
    pub fn has_previous(&self) -> bool {
        self.number > 1
    }

    /// Returns `true` if there are other pages (either next or previous).
    pub fn has_other_pages(&self) -> bool {
        self.has_next() || self.has_previous()
    }

    /// Returns the next page number.
    ///
    /// # Panics
    ///
    /// Panics if there is no next page. Use `has_next()` to check first.
    pub fn next_page_number(&self) -> usize {
        assert!(self.has_next(), "No next page");
        self.number + 1
    }

    /// Returns the previous page number.
    ///
    /// # Panics
    ///
    /// Panics if there is no previous page. Use `has_previous()` to check first.
    pub fn previous_page_number(&self) -> usize {
        assert!(self.has_previous(), "No previous page");
        self.number - 1
    }

    /// Returns the 1-based index of the first item on this page.
    ///
    /// Returns 0 if the page is empty.
    pub fn start_index(&self) -> usize {
        if self.object_list.is_empty() {
            return 0;
        }
        (self.number - 1) * self.per_page + 1
    }

    /// Returns the 1-based index of the last item on this page.
    ///
    /// Returns 0 if the page is empty.
    pub fn end_index(&self) -> usize {
        if self.object_list.is_empty() {
            return 0;
        }
        self.start_index() + self.object_list.len() - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_items(n: usize) -> Vec<i32> {
        (1..=n as i32).collect()
    }

    // ── Basic pagination tests ──────────────────────────────────────

    #[test]
    fn test_paginator_count() {
        let paginator = Paginator::new(make_items(100), 10);
        assert_eq!(paginator.count(), 100);
    }

    #[test]
    fn test_paginator_num_pages() {
        let paginator = Paginator::new(make_items(100), 10);
        assert_eq!(paginator.num_pages(), 10);
    }

    #[test]
    fn test_paginator_num_pages_uneven() {
        let paginator = Paginator::new(make_items(23), 10);
        assert_eq!(paginator.num_pages(), 3);
    }

    #[test]
    fn test_paginator_page_range() {
        let paginator = Paginator::new(make_items(30), 10);
        assert_eq!(paginator.page_range(), 1..=3);
    }

    #[test]
    fn test_paginator_first_page() {
        let paginator = Paginator::new(make_items(25), 10);
        let page = paginator.page(1).unwrap();
        assert_eq!(page.object_list().len(), 10);
        assert_eq!(page.object_list()[0], 1);
        assert_eq!(page.object_list()[9], 10);
        assert_eq!(page.number(), 1);
    }

    #[test]
    fn test_paginator_last_page() {
        let paginator = Paginator::new(make_items(25), 10);
        let page = paginator.page(3).unwrap();
        assert_eq!(page.object_list().len(), 5);
        assert_eq!(page.object_list()[0], 21);
        assert_eq!(page.object_list()[4], 25);
    }

    #[test]
    fn test_paginator_middle_page() {
        let paginator = Paginator::new(make_items(30), 10);
        let page = paginator.page(2).unwrap();
        assert_eq!(page.object_list().len(), 10);
        assert_eq!(page.object_list()[0], 11);
    }

    // ── Edge case tests ─────────────────────────────────────────────

    #[test]
    fn test_paginator_empty_list() {
        let paginator: Paginator<i32> = Paginator::new(vec![], 10);
        assert_eq!(paginator.count(), 0);
        assert_eq!(paginator.num_pages(), 1); // allow_empty_first_page
        let page = paginator.page(1).unwrap();
        assert!(page.object_list().is_empty());
    }

    #[test]
    fn test_paginator_empty_no_first_page() {
        let paginator: Paginator<i32> = Paginator::new(vec![], 10).allow_empty_first_page(false);
        assert_eq!(paginator.num_pages(), 0);
        assert!(paginator.page(1).is_err());
    }

    #[test]
    fn test_paginator_page_zero() {
        let paginator = Paginator::new(make_items(10), 5);
        assert!(matches!(
            paginator.page(0),
            Err(PaginationError::InvalidPage(_))
        ));
    }

    #[test]
    fn test_paginator_page_too_large() {
        let paginator = Paginator::new(make_items(10), 5);
        assert!(matches!(paginator.page(99), Err(PaginationError::EmptyPage)));
    }

    #[test]
    fn test_paginator_single_item() {
        let paginator = Paginator::new(vec![42], 10);
        assert_eq!(paginator.num_pages(), 1);
        let page = paginator.page(1).unwrap();
        assert_eq!(page.object_list(), &[42]);
    }

    #[test]
    fn test_paginator_exact_fit() {
        let paginator = Paginator::new(make_items(10), 5);
        assert_eq!(paginator.num_pages(), 2);
        let page = paginator.page(2).unwrap();
        assert_eq!(page.object_list().len(), 5);
    }

    // ── Orphans tests ───────────────────────────────────────────────

    #[test]
    fn test_paginator_orphans_merges_last_page() {
        // 23 items, 10 per page, 3 orphans
        // Without orphans: 3 pages (10, 10, 3)
        // With orphans: last page has 3 items < 3 orphans threshold
        // Wait, orphans=3 means if last page has <= 3, merge
        // Actually: hits = max(1, 23-3) = 20, pages = ceil(20/10) = 2
        let paginator = Paginator::new(make_items(23), 10).orphans(3);
        assert_eq!(paginator.num_pages(), 2);
        let page = paginator.page(2).unwrap();
        assert_eq!(page.object_list().len(), 13); // 10 + 3 orphans
    }

    #[test]
    fn test_paginator_orphans_no_effect() {
        // 20 items, 10 per page, 3 orphans
        // hits = max(1, 20-3) = 17, pages = ceil(17/10) = 2
        let paginator = Paginator::new(make_items(20), 10).orphans(3);
        assert_eq!(paginator.num_pages(), 2);
        let page2 = paginator.page(2).unwrap();
        assert_eq!(page2.object_list().len(), 10);
    }

    #[test]
    fn test_paginator_orphans_all_on_one_page() {
        // 5 items, 10 per page, 5 orphans
        let paginator = Paginator::new(make_items(5), 10).orphans(5);
        assert_eq!(paginator.num_pages(), 1);
        let page = paginator.page(1).unwrap();
        assert_eq!(page.object_list().len(), 5);
    }

    // ── Page navigation tests ───────────────────────────────────────

    #[test]
    fn test_page_has_next() {
        let paginator = Paginator::new(make_items(20), 10);
        let page1 = paginator.page(1).unwrap();
        let page2 = paginator.page(2).unwrap();
        assert!(page1.has_next());
        assert!(!page2.has_next());
    }

    #[test]
    fn test_page_has_previous() {
        let paginator = Paginator::new(make_items(20), 10);
        let page1 = paginator.page(1).unwrap();
        let page2 = paginator.page(2).unwrap();
        assert!(!page1.has_previous());
        assert!(page2.has_previous());
    }

    #[test]
    fn test_page_has_other_pages() {
        let paginator = Paginator::new(make_items(5), 10);
        let page = paginator.page(1).unwrap();
        assert!(!page.has_other_pages());

        let paginator = Paginator::new(make_items(20), 10);
        let page = paginator.page(1).unwrap();
        assert!(page.has_other_pages());
    }

    #[test]
    fn test_page_next_page_number() {
        let paginator = Paginator::new(make_items(30), 10);
        let page = paginator.page(1).unwrap();
        assert_eq!(page.next_page_number(), 2);
    }

    #[test]
    fn test_page_previous_page_number() {
        let paginator = Paginator::new(make_items(30), 10);
        let page = paginator.page(2).unwrap();
        assert_eq!(page.previous_page_number(), 1);
    }

    #[test]
    #[should_panic(expected = "No next page")]
    fn test_page_next_panics_on_last() {
        let paginator = Paginator::new(make_items(10), 10);
        let page = paginator.page(1).unwrap();
        page.next_page_number();
    }

    #[test]
    #[should_panic(expected = "No previous page")]
    fn test_page_previous_panics_on_first() {
        let paginator = Paginator::new(make_items(10), 10);
        let page = paginator.page(1).unwrap();
        page.previous_page_number();
    }

    // ── Index tests ─────────────────────────────────────────────────

    #[test]
    fn test_page_start_index() {
        let paginator = Paginator::new(make_items(25), 10);
        assert_eq!(paginator.page(1).unwrap().start_index(), 1);
        assert_eq!(paginator.page(2).unwrap().start_index(), 11);
        assert_eq!(paginator.page(3).unwrap().start_index(), 21);
    }

    #[test]
    fn test_page_end_index() {
        let paginator = Paginator::new(make_items(25), 10);
        assert_eq!(paginator.page(1).unwrap().end_index(), 10);
        assert_eq!(paginator.page(2).unwrap().end_index(), 20);
        assert_eq!(paginator.page(3).unwrap().end_index(), 25);
    }

    #[test]
    fn test_page_start_index_empty() {
        let paginator: Paginator<i32> = Paginator::new(vec![], 10);
        let page = paginator.page(1).unwrap();
        assert_eq!(page.start_index(), 0);
        assert_eq!(page.end_index(), 0);
    }

    // ── get_page tests (graceful) ───────────────────────────────────

    #[test]
    fn test_get_page_valid() {
        let paginator = Paginator::new(make_items(20), 10);
        let page = paginator.get_page(1);
        assert_eq!(page.number(), 1);
        assert_eq!(page.object_list().len(), 10);
    }

    #[test]
    fn test_get_page_zero_returns_first() {
        let paginator = Paginator::new(make_items(20), 10);
        let page = paginator.get_page(0);
        assert_eq!(page.number(), 1);
    }

    #[test]
    fn test_get_page_too_large_returns_last() {
        let paginator = Paginator::new(make_items(25), 10);
        let page = paginator.get_page(999);
        assert_eq!(page.number(), 3);
    }

    // ── Elided page range tests ─────────────────────────────────────

    #[test]
    fn test_elided_range_small() {
        let paginator = Paginator::new(make_items(50), 10);
        let range = paginator.get_elided_page_range(1, 2, 1);
        // Only 5 pages, should show all without ellipsis
        let page_numbers: Vec<usize> = range
            .iter()
            .filter_map(|item| match item {
                PageRangeItem::Page(n) => Some(*n),
                _ => None,
            })
            .collect();
        assert_eq!(page_numbers, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_elided_range_large_start() {
        let paginator = Paginator::new(make_items(200), 10);
        let range = paginator.get_elided_page_range(1, 2, 2);
        // Should show pages at start, window, ellipsis, and end pages
        assert!(range.contains(&PageRangeItem::Page(1)));
        assert!(range.contains(&PageRangeItem::Page(2)));
        assert!(range.contains(&PageRangeItem::Page(3)));
    }

    #[test]
    fn test_elided_range_large_middle() {
        let paginator = Paginator::new(make_items(200), 10);
        let range = paginator.get_elided_page_range(10, 2, 2);
        // Should have ellipsis
        assert!(range.contains(&PageRangeItem::Ellipsis));
        assert!(range.contains(&PageRangeItem::Page(10)));
    }

    #[test]
    fn test_elided_range_large_end() {
        let paginator = Paginator::new(make_items(200), 10);
        let range = paginator.get_elided_page_range(20, 2, 2);
        assert!(range.contains(&PageRangeItem::Page(20)));
        assert!(range.contains(&PageRangeItem::Page(19)));
    }

    #[test]
    fn test_elided_range_empty_paginator() {
        let paginator: Paginator<i32> = Paginator::new(vec![], 10);
        let range = paginator.get_elided_page_range(1, 2, 2);
        assert_eq!(range, vec![PageRangeItem::Page(1)]);
    }

    // ── Error display tests ─────────────────────────────────────────

    #[test]
    fn test_pagination_error_display() {
        assert_eq!(
            format!("{}", PaginationError::EmptyPage),
            "That page contains no results"
        );
        assert_eq!(
            format!("{}", PaginationError::PageNotAnInteger),
            "That page number is not an integer"
        );
        assert_eq!(
            format!("{}", PaginationError::InvalidPage("bad".into())),
            "Invalid page: bad"
        );
    }

    // ── Per page edge cases ─────────────────────────────────────────

    #[test]
    fn test_paginator_per_page_one() {
        let paginator = Paginator::new(make_items(5), 1);
        assert_eq!(paginator.num_pages(), 5);
        let page = paginator.page(3).unwrap();
        assert_eq!(page.object_list(), &[3]);
    }

    #[test]
    fn test_paginator_per_page_larger_than_total() {
        let paginator = Paginator::new(make_items(5), 100);
        assert_eq!(paginator.num_pages(), 1);
        let page = paginator.page(1).unwrap();
        assert_eq!(page.object_list().len(), 5);
    }

    #[test]
    fn test_paginator_per_page_zero_clamped() {
        // per_page=0 is clamped to 1
        let paginator = Paginator::new(make_items(5), 0);
        assert_eq!(paginator.per_page, 1);
        assert_eq!(paginator.num_pages(), 5);
    }
}
