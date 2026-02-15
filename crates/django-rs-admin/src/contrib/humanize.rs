//! Humanize utilities for formatting values in a human-friendly way.
//!
//! Mirrors Django's `django.contrib.humanize` template filters. These functions
//! convert numbers, dates, and file sizes into human-readable strings.

use chrono::{DateTime, Datelike, NaiveDate, Utc};

/// Formats an integer with commas as thousand separators.
///
/// # Examples
///
/// ```
/// use django_rs_admin::contrib::humanize::intcomma;
///
/// assert_eq!(intcomma(1000), "1,000");
/// assert_eq!(intcomma(1000000), "1,000,000");
/// assert_eq!(intcomma(-1234567), "-1,234,567");
/// ```
pub fn intcomma(value: i64) -> String {
    let negative = value < 0;
    let abs_str = value.unsigned_abs().to_string();
    let chars: Vec<char> = abs_str.chars().collect();
    let mut result = String::with_capacity(abs_str.len() + abs_str.len() / 3);

    for (i, ch) in chars.iter().enumerate() {
        if i > 0 && (chars.len() - i) % 3 == 0 {
            result.push(',');
        }
        result.push(*ch);
    }

    if negative {
        format!("-{result}")
    } else {
        result
    }
}

/// Converts a large integer to a human-readable word form.
///
/// # Examples
///
/// ```
/// use django_rs_admin::contrib::humanize::intword;
///
/// assert_eq!(intword(1_000_000), "1.0 million");
/// assert_eq!(intword(2_500_000_000), "2.5 billion");
/// assert_eq!(intword(999), "999");
/// ```
pub fn intword(value: i64) -> String {
    let abs = value.unsigned_abs();

    let (divisor, label) = if abs >= 1_000_000_000_000_000 {
        (1_000_000_000_000_000_u64, "quadrillion")
    } else if abs >= 1_000_000_000_000 {
        (1_000_000_000_000_u64, "trillion")
    } else if abs >= 1_000_000_000 {
        (1_000_000_000_u64, "billion")
    } else if abs >= 1_000_000 {
        (1_000_000_u64, "million")
    } else {
        return intcomma(value);
    };

    let sign = if value < 0 { "-" } else { "" };
    #[allow(clippy::cast_precision_loss)]
    let quot = abs as f64 / divisor as f64;

    format!("{sign}{quot:.1} {label}")
}

/// Converts a `DateTime<Utc>` to a human-readable relative time string.
///
/// # Examples
///
/// ```
/// use django_rs_admin::contrib::humanize::naturaltime;
/// use chrono::Utc;
///
/// let now = Utc::now();
/// assert_eq!(naturaltime(now), "just now");
/// ```
pub fn naturaltime(dt: DateTime<Utc>) -> String {
    let now = Utc::now();
    let diff = now.signed_duration_since(dt);

    if diff.num_seconds().abs() < 10 {
        return "just now".to_string();
    }

    let (amount, in_past) = if diff.num_seconds() >= 0 {
        (diff, true)
    } else {
        (-diff, false)
    };

    let text = if amount.num_days() >= 365 {
        let years = amount.num_days() / 365;
        if years == 1 {
            "1 year".to_string()
        } else {
            format!("{years} years")
        }
    } else if amount.num_days() >= 30 {
        let months = amount.num_days() / 30;
        if months == 1 {
            "1 month".to_string()
        } else {
            format!("{months} months")
        }
    } else if amount.num_days() >= 7 {
        let weeks = amount.num_days() / 7;
        if weeks == 1 {
            "1 week".to_string()
        } else {
            format!("{weeks} weeks")
        }
    } else if amount.num_days() >= 1 {
        let days = amount.num_days();
        if days == 1 {
            "1 day".to_string()
        } else {
            format!("{days} days")
        }
    } else if amount.num_hours() >= 1 {
        let hours = amount.num_hours();
        if hours == 1 {
            "1 hour".to_string()
        } else {
            format!("{hours} hours")
        }
    } else if amount.num_minutes() >= 1 {
        let minutes = amount.num_minutes();
        if minutes == 1 {
            "1 minute".to_string()
        } else {
            format!("{minutes} minutes")
        }
    } else {
        let seconds = amount.num_seconds();
        if seconds == 1 {
            "1 second".to_string()
        } else {
            format!("{seconds} seconds")
        }
    };

    if in_past {
        format!("{text} ago")
    } else {
        format!("{text} from now")
    }
}

/// Converts a date to a human-readable string relative to today.
///
/// Returns "yesterday", "today", or "tomorrow" for dates within that range,
/// otherwise returns the date formatted as "Month Day, Year".
///
/// # Examples
///
/// ```
/// use django_rs_admin::contrib::humanize::naturalday;
/// use chrono::{NaiveDate, Utc, Datelike};
///
/// let today = Utc::now().date_naive();
/// assert_eq!(naturalday(today), "today");
/// ```
pub fn naturalday(date: NaiveDate) -> String {
    let today = Utc::now().date_naive();
    let diff = date.signed_duration_since(today).num_days();

    match diff {
        -1 => "yesterday".to_string(),
        0 => "today".to_string(),
        1 => "tomorrow".to_string(),
        _ => format_date(date),
    }
}

/// Formats a date as "Month Day, Year".
fn format_date(date: NaiveDate) -> String {
    let month = match date.month() {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "Unknown",
    };
    format!("{} {}, {}", month, date.day(), date.year())
}

/// Converts an integer to its ordinal form.
///
/// # Examples
///
/// ```
/// use django_rs_admin::contrib::humanize::ordinal;
///
/// assert_eq!(ordinal(1), "1st");
/// assert_eq!(ordinal(2), "2nd");
/// assert_eq!(ordinal(3), "3rd");
/// assert_eq!(ordinal(4), "4th");
/// assert_eq!(ordinal(11), "11th");
/// assert_eq!(ordinal(12), "12th");
/// assert_eq!(ordinal(13), "13th");
/// assert_eq!(ordinal(21), "21st");
/// assert_eq!(ordinal(22), "22nd");
/// assert_eq!(ordinal(23), "23rd");
/// assert_eq!(ordinal(111), "111th");
/// ```
pub fn ordinal(value: i64) -> String {
    let abs = value.unsigned_abs();
    let suffix = match (abs % 10, abs % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{value}{suffix}")
}

/// Formats a byte count into a human-readable file size string.
///
/// # Examples
///
/// ```
/// use django_rs_admin::contrib::humanize::filesizeformat;
///
/// assert_eq!(filesizeformat(0), "0 bytes");
/// assert_eq!(filesizeformat(1), "1 byte");
/// assert_eq!(filesizeformat(1024), "1.0 KB");
/// assert_eq!(filesizeformat(1048576), "1.0 MB");
/// ```
pub fn filesizeformat(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    const TB: f64 = GB * 1024.0;
    const PB: f64 = TB * 1024.0;

    #[allow(clippy::cast_precision_loss)]
    let size = bytes as f64;

    if bytes == 0 {
        "0 bytes".to_string()
    } else if bytes == 1 {
        "1 byte".to_string()
    } else if size < KB {
        format!("{bytes} bytes")
    } else if size < MB {
        format!("{:.1} KB", size / KB)
    } else if size < GB {
        format!("{:.1} MB", size / MB)
    } else if size < TB {
        format!("{:.1} GB", size / GB)
    } else if size < PB {
        format!("{:.1} TB", size / TB)
    } else {
        format!("{:.1} PB", size / PB)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeDelta;

    #[test]
    fn test_intcomma_zero() {
        assert_eq!(intcomma(0), "0");
    }

    #[test]
    fn test_intcomma_small() {
        assert_eq!(intcomma(100), "100");
        assert_eq!(intcomma(999), "999");
    }

    #[test]
    fn test_intcomma_thousands() {
        assert_eq!(intcomma(1000), "1,000");
        assert_eq!(intcomma(10000), "10,000");
        assert_eq!(intcomma(100_000), "100,000");
    }

    #[test]
    fn test_intcomma_millions() {
        assert_eq!(intcomma(1_000_000), "1,000,000");
        assert_eq!(intcomma(1_234_567), "1,234,567");
    }

    #[test]
    fn test_intcomma_negative() {
        assert_eq!(intcomma(-1234), "-1,234");
        assert_eq!(intcomma(-1_234_567), "-1,234,567");
    }

    #[test]
    fn test_intword_small() {
        assert_eq!(intword(999), "999");
        assert_eq!(intword(0), "0");
    }

    #[test]
    fn test_intword_million() {
        assert_eq!(intword(1_000_000), "1.0 million");
        assert_eq!(intword(2_500_000), "2.5 million");
    }

    #[test]
    fn test_intword_billion() {
        assert_eq!(intword(1_000_000_000), "1.0 billion");
        assert_eq!(intword(2_500_000_000), "2.5 billion");
    }

    #[test]
    fn test_intword_trillion() {
        assert_eq!(intword(1_000_000_000_000), "1.0 trillion");
    }

    #[test]
    fn test_intword_negative() {
        assert_eq!(intword(-1_000_000), "-1.0 million");
    }

    #[test]
    fn test_naturaltime_just_now() {
        let now = Utc::now();
        assert_eq!(naturaltime(now), "just now");
    }

    #[test]
    fn test_naturaltime_seconds_ago() {
        let dt = Utc::now() - TimeDelta::seconds(30);
        let result = naturaltime(dt);
        assert!(result.contains("seconds ago"));
    }

    #[test]
    fn test_naturaltime_minutes_ago() {
        let dt = Utc::now() - TimeDelta::minutes(5);
        let result = naturaltime(dt);
        assert!(result.contains("minutes ago"));
    }

    #[test]
    fn test_naturaltime_hours_ago() {
        let dt = Utc::now() - TimeDelta::hours(3);
        let result = naturaltime(dt);
        assert!(result.contains("hours ago"));
    }

    #[test]
    fn test_naturaltime_days_ago() {
        let dt = Utc::now() - TimeDelta::days(2);
        let result = naturaltime(dt);
        assert!(result.contains("days ago"));
    }

    #[test]
    fn test_naturaltime_weeks_ago() {
        let dt = Utc::now() - TimeDelta::weeks(2);
        let result = naturaltime(dt);
        assert!(result.contains("weeks ago"));
    }

    #[test]
    fn test_naturaltime_months_ago() {
        let dt = Utc::now() - TimeDelta::days(60);
        let result = naturaltime(dt);
        assert!(result.contains("months ago"));
    }

    #[test]
    fn test_naturaltime_years_ago() {
        let dt = Utc::now() - TimeDelta::days(400);
        let result = naturaltime(dt);
        assert!(result.contains("year"));
    }

    #[test]
    fn test_naturaltime_future() {
        let dt = Utc::now() + TimeDelta::hours(3);
        let result = naturaltime(dt);
        assert!(result.contains("from now"));
    }

    #[test]
    fn test_naturalday_today() {
        let today = Utc::now().date_naive();
        assert_eq!(naturalday(today), "today");
    }

    #[test]
    fn test_naturalday_yesterday() {
        let yesterday = Utc::now().date_naive() - TimeDelta::days(1);
        assert_eq!(naturalday(yesterday), "yesterday");
    }

    #[test]
    fn test_naturalday_tomorrow() {
        let tomorrow = Utc::now().date_naive() + TimeDelta::days(1);
        assert_eq!(naturalday(tomorrow), "tomorrow");
    }

    #[test]
    fn test_naturalday_other() {
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        assert_eq!(naturalday(date), "January 15, 2024");
    }

    #[test]
    fn test_ordinal_first() {
        assert_eq!(ordinal(1), "1st");
    }

    #[test]
    fn test_ordinal_second() {
        assert_eq!(ordinal(2), "2nd");
    }

    #[test]
    fn test_ordinal_third() {
        assert_eq!(ordinal(3), "3rd");
    }

    #[test]
    fn test_ordinal_fourth() {
        assert_eq!(ordinal(4), "4th");
    }

    #[test]
    fn test_ordinal_eleventh() {
        assert_eq!(ordinal(11), "11th");
    }

    #[test]
    fn test_ordinal_twelfth() {
        assert_eq!(ordinal(12), "12th");
    }

    #[test]
    fn test_ordinal_thirteenth() {
        assert_eq!(ordinal(13), "13th");
    }

    #[test]
    fn test_ordinal_twenty_first() {
        assert_eq!(ordinal(21), "21st");
    }

    #[test]
    fn test_ordinal_twenty_second() {
        assert_eq!(ordinal(22), "22nd");
    }

    #[test]
    fn test_ordinal_twenty_third() {
        assert_eq!(ordinal(23), "23rd");
    }

    #[test]
    fn test_ordinal_hundredth() {
        assert_eq!(ordinal(100), "100th");
    }

    #[test]
    fn test_ordinal_hundred_eleventh() {
        assert_eq!(ordinal(111), "111th");
    }

    #[test]
    fn test_ordinal_negative() {
        assert_eq!(ordinal(-1), "-1st");
        assert_eq!(ordinal(-2), "-2nd");
    }

    #[test]
    fn test_ordinal_zero() {
        assert_eq!(ordinal(0), "0th");
    }

    #[test]
    fn test_filesizeformat_zero() {
        assert_eq!(filesizeformat(0), "0 bytes");
    }

    #[test]
    fn test_filesizeformat_one_byte() {
        assert_eq!(filesizeformat(1), "1 byte");
    }

    #[test]
    fn test_filesizeformat_bytes() {
        assert_eq!(filesizeformat(100), "100 bytes");
        assert_eq!(filesizeformat(1023), "1023 bytes");
    }

    #[test]
    fn test_filesizeformat_kb() {
        assert_eq!(filesizeformat(1024), "1.0 KB");
        assert_eq!(filesizeformat(1536), "1.5 KB");
    }

    #[test]
    fn test_filesizeformat_mb() {
        assert_eq!(filesizeformat(1_048_576), "1.0 MB");
    }

    #[test]
    fn test_filesizeformat_gb() {
        assert_eq!(filesizeformat(1_073_741_824), "1.0 GB");
    }

    #[test]
    fn test_filesizeformat_tb() {
        assert_eq!(filesizeformat(1_099_511_627_776), "1.0 TB");
    }

    #[test]
    fn test_filesizeformat_pb() {
        assert_eq!(filesizeformat(1_125_899_906_842_624), "1.0 PB");
    }
}
