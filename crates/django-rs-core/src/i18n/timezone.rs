//! Timezone support for i18n/l10n.
//!
//! Provides thread-local timezone activation, timezone-aware "now", and local
//! time conversion. This mirrors Django's `django.utils.timezone` module.
//!
//! ## Quick Start
//!
//! ```
//! use django_rs_core::i18n::timezone;
//!
//! // Get current UTC time
//! let utc_now = timezone::now();
//! assert!(utc_now.offset().local_minus_utc() == 0);
//!
//! // Activate a timezone
//! timezone::activate_timezone(5 * 3600); // UTC+5
//! let local = timezone::localtime(&utc_now);
//! assert_eq!(local.offset().local_minus_utc(), 5 * 3600);
//!
//! timezone::deactivate_timezone();
//! ```

use std::cell::RefCell;

use chrono::{DateTime, FixedOffset, Utc};

thread_local! {
    /// The current thread's timezone offset in seconds east of UTC.
    /// `None` means use UTC (the default).
    static CURRENT_TIMEZONE: RefCell<Option<i32>> = const { RefCell::new(None) };
}

/// Activates a timezone for the current thread.
///
/// The `offset_seconds` parameter is the number of seconds east of UTC.
/// For example, UTC+5:30 would be `5 * 3600 + 30 * 60 = 19800`.
///
/// # Examples
///
/// ```
/// use django_rs_core::i18n::timezone;
///
/// timezone::activate_timezone(3600); // UTC+1
/// assert_eq!(timezone::get_current_timezone_offset(), 3600);
/// timezone::deactivate_timezone();
/// ```
pub fn activate_timezone(offset_seconds: i32) {
    CURRENT_TIMEZONE.with(|cell| {
        *cell.borrow_mut() = Some(offset_seconds);
    });
}

/// Deactivates the current thread's timezone, reverting to UTC.
pub fn deactivate_timezone() {
    CURRENT_TIMEZONE.with(|cell| {
        *cell.borrow_mut() = None;
    });
}

/// Returns the current thread's timezone offset in seconds east of UTC.
///
/// Returns `0` (UTC) if no timezone has been activated.
pub fn get_current_timezone_offset() -> i32 {
    CURRENT_TIMEZONE.with(|cell| cell.borrow().unwrap_or(0))
}

/// Returns the current timezone as a `FixedOffset`.
pub fn get_current_timezone() -> FixedOffset {
    let offset = get_current_timezone_offset();
    FixedOffset::east_opt(offset).unwrap_or_else(|| FixedOffset::east_opt(0).expect("UTC offset"))
}

/// Returns the current date and time in UTC.
///
/// This is the timezone-aware equivalent of `Utc::now()`.
///
/// # Examples
///
/// ```
/// use django_rs_core::i18n::timezone;
///
/// let now = timezone::now();
/// // The returned DateTime is always in UTC
/// assert_eq!(now.offset().local_minus_utc(), 0);
/// ```
pub fn now() -> DateTime<FixedOffset> {
    Utc::now().with_timezone(&FixedOffset::east_opt(0).expect("UTC offset"))
}

/// Converts a `DateTime<FixedOffset>` to the current thread's active timezone.
///
/// If no timezone is active, the datetime is returned in UTC.
///
/// # Examples
///
/// ```
/// use django_rs_core::i18n::timezone;
/// use chrono::{FixedOffset, TimeZone, Timelike};
///
/// let utc = FixedOffset::east_opt(0).unwrap();
/// let dt = utc.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap();
///
/// timezone::activate_timezone(3600); // UTC+1
/// let local = timezone::localtime(&dt);
/// assert_eq!(local.hour(), 13);
/// assert_eq!(local.offset().local_minus_utc(), 3600);
///
/// timezone::deactivate_timezone();
/// ```
pub fn localtime(dt: &DateTime<FixedOffset>) -> DateTime<FixedOffset> {
    let tz = get_current_timezone();
    dt.with_timezone(&tz)
}

/// Converts a `DateTime<FixedOffset>` to a specific timezone offset.
///
/// # Examples
///
/// ```
/// use django_rs_core::i18n::timezone;
/// use chrono::{Datelike, FixedOffset, TimeZone, Timelike};
///
/// let utc = FixedOffset::east_opt(0).unwrap();
/// let dt = utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
///
/// let est = timezone::localtime_with_offset(&dt, -5 * 3600); // UTC-5
/// assert_eq!(est.day(), 31);
/// assert_eq!(est.month(), 12);
/// assert_eq!(est.hour(), 19);
/// ```
pub fn localtime_with_offset(
    dt: &DateTime<FixedOffset>,
    offset_seconds: i32,
) -> DateTime<FixedOffset> {
    let tz = FixedOffset::east_opt(offset_seconds)
        .unwrap_or_else(|| FixedOffset::east_opt(0).expect("UTC offset"));
    dt.with_timezone(&tz)
}

/// Returns `true` if a timezone has been activated for the current thread.
pub fn is_timezone_active() -> bool {
    CURRENT_TIMEZONE.with(|cell| cell.borrow().is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, TimeZone, Timelike};

    fn setup() {
        deactivate_timezone();
    }

    #[test]
    fn test_default_timezone_is_utc() {
        setup();
        assert_eq!(get_current_timezone_offset(), 0);
        assert!(!is_timezone_active());
    }

    #[test]
    fn test_activate_timezone() {
        setup();
        activate_timezone(3600);
        assert_eq!(get_current_timezone_offset(), 3600);
        assert!(is_timezone_active());
        deactivate_timezone();
        assert_eq!(get_current_timezone_offset(), 0);
        assert!(!is_timezone_active());
    }

    #[test]
    fn test_now_returns_utc() {
        setup();
        let n = now();
        assert_eq!(n.offset().local_minus_utc(), 0);
    }

    #[test]
    fn test_localtime_utc() {
        setup();
        let utc = FixedOffset::east_opt(0).unwrap();
        let dt = utc.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap();
        let local = localtime(&dt);
        assert_eq!(local.hour(), 12);
        assert_eq!(local.offset().local_minus_utc(), 0);
    }

    #[test]
    fn test_localtime_with_timezone() {
        setup();
        let utc = FixedOffset::east_opt(0).unwrap();
        let dt = utc.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap();

        activate_timezone(5 * 3600); // UTC+5
        let local = localtime(&dt);
        assert_eq!(local.hour(), 17);
        assert_eq!(local.offset().local_minus_utc(), 5 * 3600);
        deactivate_timezone();
    }

    #[test]
    fn test_localtime_negative_offset() {
        setup();
        let utc = FixedOffset::east_opt(0).unwrap();
        let dt = utc.with_ymd_and_hms(2024, 1, 1, 3, 0, 0).unwrap();

        activate_timezone(-5 * 3600); // UTC-5 (EST)
        let local = localtime(&dt);
        assert_eq!(local.hour(), 22);
        assert_eq!(local.day(), 31);
        assert_eq!(local.month(), 12);
        deactivate_timezone();
    }

    #[test]
    fn test_localtime_with_offset() {
        setup();
        let utc = FixedOffset::east_opt(0).unwrap();
        let dt = utc.with_ymd_and_hms(2024, 6, 15, 0, 0, 0).unwrap();

        let result = localtime_with_offset(&dt, 9 * 3600); // UTC+9 (JST)
        assert_eq!(result.hour(), 9);
        assert_eq!(result.offset().local_minus_utc(), 9 * 3600);
    }

    #[test]
    fn test_get_current_timezone() {
        setup();
        let tz = get_current_timezone();
        assert_eq!(tz.local_minus_utc(), 0);

        activate_timezone(-8 * 3600); // PST
        let tz = get_current_timezone();
        assert_eq!(tz.local_minus_utc(), -8 * 3600);
        deactivate_timezone();
    }

    #[test]
    fn test_activate_multiple_times() {
        setup();
        activate_timezone(3600);
        assert_eq!(get_current_timezone_offset(), 3600);
        activate_timezone(7200);
        assert_eq!(get_current_timezone_offset(), 7200);
        deactivate_timezone();
    }

    #[test]
    fn test_half_hour_offset() {
        setup();
        let utc = FixedOffset::east_opt(0).unwrap();
        let dt = utc.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap();

        // India Standard Time: UTC+5:30
        activate_timezone(5 * 3600 + 30 * 60);
        let local = localtime(&dt);
        assert_eq!(local.hour(), 17);
        assert_eq!(local.minute(), 30);
        deactivate_timezone();
    }
}
