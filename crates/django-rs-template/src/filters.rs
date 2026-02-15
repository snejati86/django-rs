//! Built-in template filters.
//!
//! Implements Django's most commonly used template filters. Each filter is a
//! function registered in a [`FilterRegistry`].

use std::collections::HashMap;
use std::sync::OnceLock;

use django_rs_core::error::DjangoError;

use crate::context::{escape_html, ContextValue};

/// A template filter function.
///
/// Takes a value and optional arguments, and returns a transformed value.
pub trait Filter: Send + Sync {
    /// Returns the filter name.
    fn name(&self) -> &'static str;

    /// Applies the filter to a value with the given arguments.
    fn apply(
        &self,
        value: &ContextValue,
        args: &[ContextValue],
    ) -> Result<ContextValue, DjangoError>;
}

/// A registry of available template filters.
pub struct FilterRegistry {
    filters: HashMap<String, Box<dyn Filter>>,
}

impl FilterRegistry {
    /// Creates a new empty filter registry.
    pub fn new() -> Self {
        Self {
            filters: HashMap::new(),
        }
    }

    /// Registers a filter.
    pub fn register(&mut self, filter: Box<dyn Filter>) {
        self.filters.insert(filter.name().to_string(), filter);
    }

    /// Applies a named filter to a value.
    pub fn apply(
        &self,
        name: &str,
        value: &ContextValue,
        args: &[ContextValue],
    ) -> Result<ContextValue, DjangoError> {
        let filter = self.filters.get(name).ok_or_else(|| {
            DjangoError::TemplateSyntaxError(format!("Unknown filter: '{name}'"))
        })?;
        filter.apply(value, args)
    }
}

impl Default for FilterRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Returns the default filter registry with all built-in filters.
pub fn default_registry() -> &'static FilterRegistry {
    static REGISTRY: OnceLock<FilterRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut r = FilterRegistry::new();
        register_all(&mut r);
        r
    })
}

/// Registers all built-in filters.
fn register_all(r: &mut FilterRegistry) {
    // String filters
    r.register(Box::new(LowerFilter));
    r.register(Box::new(UpperFilter));
    r.register(Box::new(TitleFilter));
    r.register(Box::new(CapfirstFilter));
    r.register(Box::new(CutFilter));
    r.register(Box::new(TruncatecharsFilter));
    r.register(Box::new(TruncatewordsFilter));
    r.register(Box::new(StriptagsFilter));
    r.register(Box::new(EscapeFilter));
    r.register(Box::new(EscapejsFilter));
    r.register(Box::new(SafeFilter));
    r.register(Box::new(LinebreaksFilter));
    r.register(Box::new(LinebreaksbrFilter));
    r.register(Box::new(UrlizeFilter));
    r.register(Box::new(SlugifyFilter));
    r.register(Box::new(CenterFilter));
    r.register(Box::new(LjustFilter));
    r.register(Box::new(RjustFilter));
    r.register(Box::new(WordwrapFilter));
    r.register(Box::new(AddslashesFilter));

    // List/iteration filters
    r.register(Box::new(LengthFilter));
    r.register(Box::new(FirstFilter));
    r.register(Box::new(LastFilter));
    r.register(Box::new(JoinFilter));
    r.register(Box::new(SliceFilter));
    r.register(Box::new(DictsortFilter));
    r.register(Box::new(DictsortreversedFilter));
    r.register(Box::new(RandomFilter));
    r.register(Box::new(UnorderedListFilter));

    // Number filters
    r.register(Box::new(AddFilter));
    r.register(Box::new(FloatformatFilter));
    r.register(Box::new(FilesizeformatFilter));
    r.register(Box::new(DivisiblebyFilter));

    // Date filters
    r.register(Box::new(DateFilter));
    r.register(Box::new(TimeFilter));
    r.register(Box::new(TimesinceFilter));
    r.register(Box::new(TimeuntilFilter));

    // Logic filters
    r.register(Box::new(DefaultFilter));
    r.register(Box::new(DefaultIfNoneFilter));
    r.register(Box::new(YesnoFilter));
    r.register(Box::new(PluralizeFilter));

    // Additional useful filters
    r.register(Box::new(LengthIsFilter));
}

// ============================================================
// String filters
// ============================================================

struct LowerFilter;
impl Filter for LowerFilter {
    fn name(&self) -> &'static str { "lower" }
    fn apply(&self, value: &ContextValue, _args: &[ContextValue]) -> Result<ContextValue, DjangoError> {
        Ok(ContextValue::String(value.to_display_string().to_lowercase()))
    }
}

struct UpperFilter;
impl Filter for UpperFilter {
    fn name(&self) -> &'static str { "upper" }
    fn apply(&self, value: &ContextValue, _args: &[ContextValue]) -> Result<ContextValue, DjangoError> {
        Ok(ContextValue::String(value.to_display_string().to_uppercase()))
    }
}

struct TitleFilter;
impl Filter for TitleFilter {
    fn name(&self) -> &'static str { "title" }
    fn apply(&self, value: &ContextValue, _args: &[ContextValue]) -> Result<ContextValue, DjangoError> {
        let s = value.to_display_string();
        let titled = s.split_whitespace()
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    Some(c) => format!("{}{}", c.to_uppercase(), chars.as_str().to_lowercase()),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        Ok(ContextValue::String(titled))
    }
}

struct CapfirstFilter;
impl Filter for CapfirstFilter {
    fn name(&self) -> &'static str { "capfirst" }
    fn apply(&self, value: &ContextValue, _args: &[ContextValue]) -> Result<ContextValue, DjangoError> {
        let s = value.to_display_string();
        let mut chars = s.chars();
        let result = match chars.next() {
            Some(c) => format!("{}{}", c.to_uppercase(), chars.as_str()),
            None => String::new(),
        };
        Ok(ContextValue::String(result))
    }
}

struct CutFilter;
impl Filter for CutFilter {
    fn name(&self) -> &'static str { "cut" }
    fn apply(&self, value: &ContextValue, args: &[ContextValue]) -> Result<ContextValue, DjangoError> {
        let s = value.to_display_string();
        let to_remove = args.first().map(|a| a.to_display_string()).unwrap_or_default();
        Ok(ContextValue::String(s.replace(&to_remove, "")))
    }
}

struct TruncatecharsFilter;
impl Filter for TruncatecharsFilter {
    fn name(&self) -> &'static str { "truncatechars" }
    fn apply(&self, value: &ContextValue, args: &[ContextValue]) -> Result<ContextValue, DjangoError> {
        let s = value.to_display_string();
        let max_len = args.first().and_then(|a| a.as_integer()).unwrap_or(0) as usize;
        if max_len == 0 || s.len() <= max_len {
            return Ok(ContextValue::String(s));
        }
        if max_len <= 3 {
            return Ok(ContextValue::String("...".to_string()));
        }
        let truncated = &s[..max_len - 3];
        Ok(ContextValue::String(format!("{truncated}...")))
    }
}

struct TruncatewordsFilter;
impl Filter for TruncatewordsFilter {
    fn name(&self) -> &'static str { "truncatewords" }
    fn apply(&self, value: &ContextValue, args: &[ContextValue]) -> Result<ContextValue, DjangoError> {
        let s = value.to_display_string();
        let max_words = args.first().and_then(|a| a.as_integer()).unwrap_or(0) as usize;
        let words: Vec<&str> = s.split_whitespace().collect();
        if max_words == 0 || words.len() <= max_words {
            return Ok(ContextValue::String(s));
        }
        let truncated = words[..max_words].join(" ");
        Ok(ContextValue::String(format!("{truncated} ...")))
    }
}

struct StriptagsFilter;
impl Filter for StriptagsFilter {
    fn name(&self) -> &'static str { "striptags" }
    fn apply(&self, value: &ContextValue, _args: &[ContextValue]) -> Result<ContextValue, DjangoError> {
        let s = value.to_display_string();
        let re = regex::Regex::new(r"<[^>]*>").unwrap();
        Ok(ContextValue::String(re.replace_all(&s, "").to_string()))
    }
}

struct EscapeFilter;
impl Filter for EscapeFilter {
    fn name(&self) -> &'static str { "escape" }
    fn apply(&self, value: &ContextValue, _args: &[ContextValue]) -> Result<ContextValue, DjangoError> {
        let s = value.to_display_string();
        Ok(ContextValue::String(escape_html(&s)))
    }
}

struct EscapejsFilter;
impl Filter for EscapejsFilter {
    fn name(&self) -> &'static str { "escapejs" }
    fn apply(&self, value: &ContextValue, _args: &[ContextValue]) -> Result<ContextValue, DjangoError> {
        let s = value.to_display_string();
        let result = s
            .replace('\\', "\\\\")
            .replace('\'', "\\'")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('<', "\\u003C")
            .replace('>', "\\u003E")
            .replace('&', "\\u0026");
        Ok(ContextValue::SafeString(result))
    }
}

struct SafeFilter;
impl Filter for SafeFilter {
    fn name(&self) -> &'static str { "safe" }
    fn apply(&self, value: &ContextValue, _args: &[ContextValue]) -> Result<ContextValue, DjangoError> {
        Ok(ContextValue::SafeString(value.to_display_string()))
    }
}

struct LinebreaksFilter;
impl Filter for LinebreaksFilter {
    fn name(&self) -> &'static str { "linebreaks" }
    fn apply(&self, value: &ContextValue, _args: &[ContextValue]) -> Result<ContextValue, DjangoError> {
        let s = value.to_display_string();
        let paragraphs: Vec<&str> = s.split("\n\n").collect();
        let result = paragraphs
            .iter()
            .map(|p| {
                let lines = p.replace('\n', "<br>");
                format!("<p>{lines}</p>")
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        Ok(ContextValue::SafeString(result))
    }
}

struct LinebreaksbrFilter;
impl Filter for LinebreaksbrFilter {
    fn name(&self) -> &'static str { "linebreaksbr" }
    fn apply(&self, value: &ContextValue, _args: &[ContextValue]) -> Result<ContextValue, DjangoError> {
        let s = value.to_display_string();
        Ok(ContextValue::SafeString(s.replace('\n', "<br>")))
    }
}

struct UrlizeFilter;
impl Filter for UrlizeFilter {
    fn name(&self) -> &'static str { "urlize" }
    fn apply(&self, value: &ContextValue, _args: &[ContextValue]) -> Result<ContextValue, DjangoError> {
        let s = value.to_display_string();
        let re = regex::Regex::new(r"(https?://[^\s<]+)").unwrap();
        let result = re.replace_all(&s, r#"<a href="$1">$1</a>"#).to_string();
        Ok(ContextValue::SafeString(result))
    }
}

struct SlugifyFilter;
impl Filter for SlugifyFilter {
    fn name(&self) -> &'static str { "slugify" }
    fn apply(&self, value: &ContextValue, _args: &[ContextValue]) -> Result<ContextValue, DjangoError> {
        let s = value.to_display_string().to_lowercase();
        let re = regex::Regex::new(r"[^a-z0-9\s-]").unwrap();
        let cleaned = re.replace_all(&s, "");
        let re2 = regex::Regex::new(r"[\s]+").unwrap();
        let slugified = re2.replace_all(&cleaned, "-").trim_matches('-').to_string();
        Ok(ContextValue::String(slugified))
    }
}

struct CenterFilter;
impl Filter for CenterFilter {
    fn name(&self) -> &'static str { "center" }
    fn apply(&self, value: &ContextValue, args: &[ContextValue]) -> Result<ContextValue, DjangoError> {
        let s = value.to_display_string();
        let width = args.first().and_then(|a| a.as_integer()).unwrap_or(0) as usize;
        if s.len() >= width {
            return Ok(ContextValue::String(s));
        }
        let padding = width - s.len();
        let left = padding / 2;
        let right = padding - left;
        Ok(ContextValue::String(format!(
            "{}{}{}",
            " ".repeat(left),
            s,
            " ".repeat(right)
        )))
    }
}

struct LjustFilter;
impl Filter for LjustFilter {
    fn name(&self) -> &'static str { "ljust" }
    fn apply(&self, value: &ContextValue, args: &[ContextValue]) -> Result<ContextValue, DjangoError> {
        let s = value.to_display_string();
        let width = args.first().and_then(|a| a.as_integer()).unwrap_or(0) as usize;
        Ok(ContextValue::String(format!("{s:<width$}")))
    }
}

struct RjustFilter;
impl Filter for RjustFilter {
    fn name(&self) -> &'static str { "rjust" }
    fn apply(&self, value: &ContextValue, args: &[ContextValue]) -> Result<ContextValue, DjangoError> {
        let s = value.to_display_string();
        let width = args.first().and_then(|a| a.as_integer()).unwrap_or(0) as usize;
        Ok(ContextValue::String(format!("{s:>width$}")))
    }
}

struct WordwrapFilter;
impl Filter for WordwrapFilter {
    fn name(&self) -> &'static str { "wordwrap" }
    fn apply(&self, value: &ContextValue, args: &[ContextValue]) -> Result<ContextValue, DjangoError> {
        let s = value.to_display_string();
        let width = args.first().and_then(|a| a.as_integer()).unwrap_or(79) as usize;
        let mut result = String::new();
        let mut line_len = 0;

        for word in s.split_whitespace() {
            if line_len > 0 && line_len + word.len() + 1 > width {
                result.push('\n');
                line_len = 0;
            }
            if line_len > 0 {
                result.push(' ');
                line_len += 1;
            }
            result.push_str(word);
            line_len += word.len();
        }

        Ok(ContextValue::String(result))
    }
}

struct AddslashesFilter;
impl Filter for AddslashesFilter {
    fn name(&self) -> &'static str { "addslashes" }
    fn apply(&self, value: &ContextValue, _args: &[ContextValue]) -> Result<ContextValue, DjangoError> {
        let s = value.to_display_string();
        let result = s.replace('\\', "\\\\").replace('\'', "\\'").replace('"', "\\\"");
        Ok(ContextValue::String(result))
    }
}

// ============================================================
// List/iteration filters
// ============================================================

struct LengthFilter;
impl Filter for LengthFilter {
    fn name(&self) -> &'static str { "length" }
    fn apply(&self, value: &ContextValue, _args: &[ContextValue]) -> Result<ContextValue, DjangoError> {
        let len = value.len().unwrap_or(0);
        Ok(ContextValue::Integer(len as i64))
    }
}

struct LengthIsFilter;
impl Filter for LengthIsFilter {
    fn name(&self) -> &'static str { "length_is" }
    fn apply(&self, value: &ContextValue, args: &[ContextValue]) -> Result<ContextValue, DjangoError> {
        let len = value.len().unwrap_or(0) as i64;
        let expected = args.first().and_then(|a| a.as_integer()).unwrap_or(0);
        Ok(ContextValue::Bool(len == expected))
    }
}

struct FirstFilter;
impl Filter for FirstFilter {
    fn name(&self) -> &'static str { "first" }
    fn apply(&self, value: &ContextValue, _args: &[ContextValue]) -> Result<ContextValue, DjangoError> {
        match value {
            ContextValue::List(list) => Ok(list.first().cloned().unwrap_or(ContextValue::None)),
            ContextValue::String(s) | ContextValue::SafeString(s) => {
                Ok(s.chars().next().map_or(ContextValue::None, |c| {
                    ContextValue::String(c.to_string())
                }))
            }
            _ => Ok(ContextValue::None),
        }
    }
}

struct LastFilter;
impl Filter for LastFilter {
    fn name(&self) -> &'static str { "last" }
    fn apply(&self, value: &ContextValue, _args: &[ContextValue]) -> Result<ContextValue, DjangoError> {
        match value {
            ContextValue::List(list) => Ok(list.last().cloned().unwrap_or(ContextValue::None)),
            ContextValue::String(s) | ContextValue::SafeString(s) => {
                Ok(s.chars().last().map_or(ContextValue::None, |c| {
                    ContextValue::String(c.to_string())
                }))
            }
            _ => Ok(ContextValue::None),
        }
    }
}

struct JoinFilter;
impl Filter for JoinFilter {
    fn name(&self) -> &'static str { "join" }
    fn apply(&self, value: &ContextValue, args: &[ContextValue]) -> Result<ContextValue, DjangoError> {
        let separator = args.first().map(|a| a.to_display_string()).unwrap_or_default();
        match value {
            ContextValue::List(list) => {
                let joined = list
                    .iter()
                    .map(|v| v.to_display_string())
                    .collect::<Vec<_>>()
                    .join(&separator);
                Ok(ContextValue::String(joined))
            }
            _ => Ok(value.clone()),
        }
    }
}

struct SliceFilter;
impl Filter for SliceFilter {
    fn name(&self) -> &'static str { "slice" }
    fn apply(&self, value: &ContextValue, args: &[ContextValue]) -> Result<ContextValue, DjangoError> {
        let slice_str = args.first().map(|a| a.to_display_string()).unwrap_or_default();
        let parts: Vec<&str> = slice_str.split(':').collect();

        match value {
            ContextValue::List(list) => {
                let len = list.len() as i64;
                let start = parts.first().and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
                let end = parts.get(1).and_then(|s| s.parse::<i64>().ok()).unwrap_or(len);

                let start = normalize_index(start, len);
                let end = normalize_index(end, len);

                if start >= end {
                    return Ok(ContextValue::List(Vec::new()));
                }

                Ok(ContextValue::List(list[start..end].to_vec()))
            }
            ContextValue::String(s) | ContextValue::SafeString(s) => {
                let len = s.len() as i64;
                let start = parts.first().and_then(|p| p.parse::<i64>().ok()).unwrap_or(0);
                let end = parts.get(1).and_then(|p| p.parse::<i64>().ok()).unwrap_or(len);

                let start = normalize_index(start, len);
                let end = normalize_index(end, len);

                if start >= end || start >= s.len() {
                    return Ok(ContextValue::String(String::new()));
                }
                let end = end.min(s.len());

                Ok(ContextValue::String(s[start..end].to_string()))
            }
            _ => Ok(value.clone()),
        }
    }
}

fn normalize_index(idx: i64, len: i64) -> usize {
    if idx < 0 {
        (len + idx).max(0) as usize
    } else {
        idx.min(len) as usize
    }
}

struct DictsortFilter;
impl Filter for DictsortFilter {
    fn name(&self) -> &'static str { "dictsort" }
    fn apply(&self, value: &ContextValue, args: &[ContextValue]) -> Result<ContextValue, DjangoError> {
        let key = args.first().map(|a| a.to_display_string()).unwrap_or_default();
        match value {
            ContextValue::List(list) => {
                let mut sorted = list.clone();
                sorted.sort_by(|a, b| {
                    let a_val = a.resolve_path(&key).map(|v| v.to_display_string()).unwrap_or_default();
                    let b_val = b.resolve_path(&key).map(|v| v.to_display_string()).unwrap_or_default();
                    a_val.cmp(&b_val)
                });
                Ok(ContextValue::List(sorted))
            }
            _ => Ok(value.clone()),
        }
    }
}

struct DictsortreversedFilter;
impl Filter for DictsortreversedFilter {
    fn name(&self) -> &'static str { "dictsortreversed" }
    fn apply(&self, value: &ContextValue, args: &[ContextValue]) -> Result<ContextValue, DjangoError> {
        let key = args.first().map(|a| a.to_display_string()).unwrap_or_default();
        match value {
            ContextValue::List(list) => {
                let mut sorted = list.clone();
                sorted.sort_by(|a, b| {
                    let a_val = a.resolve_path(&key).map(|v| v.to_display_string()).unwrap_or_default();
                    let b_val = b.resolve_path(&key).map(|v| v.to_display_string()).unwrap_or_default();
                    b_val.cmp(&a_val)
                });
                Ok(ContextValue::List(sorted))
            }
            _ => Ok(value.clone()),
        }
    }
}

struct RandomFilter;
impl Filter for RandomFilter {
    fn name(&self) -> &'static str { "random" }
    fn apply(&self, value: &ContextValue, _args: &[ContextValue]) -> Result<ContextValue, DjangoError> {
        use rand::Rng;
        match value {
            ContextValue::List(list) if !list.is_empty() => {
                let idx = rand::thread_rng().gen_range(0..list.len());
                Ok(list[idx].clone())
            }
            _ => Ok(ContextValue::None),
        }
    }
}

struct UnorderedListFilter;
impl Filter for UnorderedListFilter {
    fn name(&self) -> &'static str { "unordered_list" }
    fn apply(&self, value: &ContextValue, _args: &[ContextValue]) -> Result<ContextValue, DjangoError> {
        match value {
            ContextValue::List(list) => {
                let items: Vec<String> = list
                    .iter()
                    .map(|v| format!("<li>{}</li>", v.to_display_string()))
                    .collect();
                Ok(ContextValue::SafeString(items.join("\n")))
            }
            _ => Ok(ContextValue::None),
        }
    }
}

// ============================================================
// Number filters
// ============================================================

struct AddFilter;
impl Filter for AddFilter {
    fn name(&self) -> &'static str { "add" }
    fn apply(&self, value: &ContextValue, args: &[ContextValue]) -> Result<ContextValue, DjangoError> {
        let arg = args.first().unwrap_or(&ContextValue::Integer(0));

        // Try numeric addition first
        if let (Some(a), Some(b)) = (value.as_integer(), arg.as_integer()) {
            return Ok(ContextValue::Integer(a + b));
        }
        if let (Some(a), Some(b)) = (value.as_float(), arg.as_float()) {
            return Ok(ContextValue::Float(a + b));
        }

        // Fall back to string concatenation
        let a = value.to_display_string();
        let b = arg.to_display_string();
        Ok(ContextValue::String(format!("{a}{b}")))
    }
}

struct FloatformatFilter;
impl Filter for FloatformatFilter {
    fn name(&self) -> &'static str { "floatformat" }
    fn apply(&self, value: &ContextValue, args: &[ContextValue]) -> Result<ContextValue, DjangoError> {
        let f = value.as_float().unwrap_or(0.0);
        let precision = args.first().and_then(|a| a.as_integer()).unwrap_or(-1);

        let result = if precision < 0 {
            // Default Django behavior: use 1 decimal if non-zero
            let abs_prec = precision.unsigned_abs() as usize;
            let formatted = format!("{f:.abs_prec$}");
            // Strip trailing zeros after decimal point (but keep at least one)
            if formatted.contains('.') {
                let trimmed = formatted.trim_end_matches('0');
                let trimmed = trimmed.trim_end_matches('.');
                if trimmed == formatted.split('.').next().unwrap_or("") {
                    // It was an integer
                    trimmed.to_string()
                } else {
                    trimmed.to_string()
                }
            } else {
                formatted
            }
        } else {
            format!("{f:.prec$}", prec = precision as usize)
        };

        Ok(ContextValue::String(result))
    }
}

struct FilesizeformatFilter;
impl Filter for FilesizeformatFilter {
    fn name(&self) -> &'static str { "filesizeformat" }
    fn apply(&self, value: &ContextValue, _args: &[ContextValue]) -> Result<ContextValue, DjangoError> {
        let bytes = value.as_float().unwrap_or(0.0);
        let result = if bytes < 1024.0 {
            format!("{} bytes", bytes as i64)
        } else if bytes < 1_048_576.0 {
            format!("{:.1} KB", bytes / 1024.0)
        } else if bytes < 1_073_741_824.0 {
            format!("{:.1} MB", bytes / 1_048_576.0)
        } else if bytes < 1_099_511_627_776.0 {
            format!("{:.1} GB", bytes / 1_073_741_824.0)
        } else {
            format!("{:.1} TB", bytes / 1_099_511_627_776.0)
        };
        Ok(ContextValue::String(result))
    }
}

struct DivisiblebyFilter;
impl Filter for DivisiblebyFilter {
    fn name(&self) -> &'static str { "divisibleby" }
    fn apply(&self, value: &ContextValue, args: &[ContextValue]) -> Result<ContextValue, DjangoError> {
        let n = value.as_integer().unwrap_or(0);
        let divisor = args.first().and_then(|a| a.as_integer()).unwrap_or(1);
        if divisor == 0 {
            return Ok(ContextValue::Bool(false));
        }
        Ok(ContextValue::Bool(n % divisor == 0))
    }
}

// ============================================================
// Date filters
// ============================================================

struct DateFilter;
impl Filter for DateFilter {
    fn name(&self) -> &'static str { "date" }
    fn apply(&self, value: &ContextValue, args: &[ContextValue]) -> Result<ContextValue, DjangoError> {
        let format = args.first().map(|a| a.to_display_string()).unwrap_or_else(|| "N j, Y".to_string());
        let s = value.to_display_string();
        // Try to parse the date string
        if let Ok(dt) = chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d") {
            let dt = dt.and_hms_opt(0, 0, 0).unwrap();
            let dt: chrono::DateTime<chrono::Local> = chrono::DateTime::from_naive_utc_and_offset(
                dt,
                *chrono::Local::now().offset(),
            );
            Ok(ContextValue::String(crate::parser::format_django_date_pub(&dt, &format)))
        } else {
            Ok(value.clone())
        }
    }
}

struct TimeFilter;
impl Filter for TimeFilter {
    fn name(&self) -> &'static str { "time" }
    fn apply(&self, value: &ContextValue, args: &[ContextValue]) -> Result<ContextValue, DjangoError> {
        let format = args.first().map(|a| a.to_display_string()).unwrap_or_else(|| "H:i".to_string());
        let s = value.to_display_string();
        if let Ok(t) = chrono::NaiveTime::parse_from_str(&s, "%H:%M:%S") {
            let dt = chrono::NaiveDate::from_ymd_opt(2000, 1, 1).unwrap().and_time(t);
            let dt: chrono::DateTime<chrono::Local> = chrono::DateTime::from_naive_utc_and_offset(
                dt,
                *chrono::Local::now().offset(),
            );
            Ok(ContextValue::String(crate::parser::format_django_date_pub(&dt, &format)))
        } else {
            Ok(value.clone())
        }
    }
}

struct TimesinceFilter;
impl Filter for TimesinceFilter {
    fn name(&self) -> &'static str { "timesince" }
    fn apply(&self, value: &ContextValue, _args: &[ContextValue]) -> Result<ContextValue, DjangoError> {
        let s = value.to_display_string();
        if let Ok(dt) = chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d") {
            let now = chrono::Local::now().date_naive();
            let diff = now.signed_duration_since(dt);
            Ok(ContextValue::String(format_duration(diff)))
        } else {
            Ok(ContextValue::String("0 minutes".to_string()))
        }
    }
}

struct TimeuntilFilter;
impl Filter for TimeuntilFilter {
    fn name(&self) -> &'static str { "timeuntil" }
    fn apply(&self, value: &ContextValue, _args: &[ContextValue]) -> Result<ContextValue, DjangoError> {
        let s = value.to_display_string();
        if let Ok(dt) = chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d") {
            let now = chrono::Local::now().date_naive();
            let diff = dt.signed_duration_since(now);
            Ok(ContextValue::String(format_duration(diff)))
        } else {
            Ok(ContextValue::String("0 minutes".to_string()))
        }
    }
}

fn format_duration(duration: chrono::Duration) -> String {
    let total_seconds = duration.num_seconds().unsigned_abs();
    let days = total_seconds / 86400;
    let hours = (total_seconds % 86400) / 3600;
    let minutes = (total_seconds % 3600) / 60;

    if days > 365 {
        let years = days / 365;
        let remaining_days = days % 365;
        let months = remaining_days / 30;
        if months > 0 {
            format!("{years} year{}, {months} month{}", plural(years), plural(months))
        } else {
            format!("{years} year{}", plural(years))
        }
    } else if days > 30 {
        let months = days / 30;
        let remaining_days = days % 30;
        if remaining_days > 0 {
            format!("{months} month{}, {remaining_days} day{}", plural(months), plural(remaining_days))
        } else {
            format!("{months} month{}", plural(months))
        }
    } else if days > 0 {
        if hours > 0 {
            format!("{days} day{}, {hours} hour{}", plural(days), plural(hours))
        } else {
            format!("{days} day{}", plural(days))
        }
    } else if hours > 0 {
        if minutes > 0 {
            format!("{hours} hour{}, {minutes} minute{}", plural(hours), plural(minutes))
        } else {
            format!("{hours} hour{}", plural(hours))
        }
    } else {
        format!("{minutes} minute{}", plural(minutes))
    }
}

fn plural(n: u64) -> &'static str {
    if n == 1 { "" } else { "s" }
}

// ============================================================
// Logic filters
// ============================================================

struct DefaultFilter;
impl Filter for DefaultFilter {
    fn name(&self) -> &'static str { "default" }
    fn apply(&self, value: &ContextValue, args: &[ContextValue]) -> Result<ContextValue, DjangoError> {
        if value.is_truthy() {
            Ok(value.clone())
        } else {
            Ok(args.first().cloned().unwrap_or(ContextValue::None))
        }
    }
}

struct DefaultIfNoneFilter;
impl Filter for DefaultIfNoneFilter {
    fn name(&self) -> &'static str { "default_if_none" }
    fn apply(&self, value: &ContextValue, args: &[ContextValue]) -> Result<ContextValue, DjangoError> {
        if matches!(value, ContextValue::None) {
            Ok(args.first().cloned().unwrap_or(ContextValue::None))
        } else {
            Ok(value.clone())
        }
    }
}

struct YesnoFilter;
impl Filter for YesnoFilter {
    fn name(&self) -> &'static str { "yesno" }
    fn apply(&self, value: &ContextValue, args: &[ContextValue]) -> Result<ContextValue, DjangoError> {
        let mapping = args.first().map(|a| a.to_display_string()).unwrap_or_else(|| "yes,no,maybe".to_string());
        let parts: Vec<&str> = mapping.split(',').collect();
        let yes = parts.first().unwrap_or(&"yes");
        let no = parts.get(1).unwrap_or(&"no");
        let maybe = parts.get(2).unwrap_or(no);

        let result = match value {
            ContextValue::None => maybe,
            v if v.is_truthy() => yes,
            _ => no,
        };

        Ok(ContextValue::String((*result).to_string()))
    }
}

struct PluralizeFilter;
impl Filter for PluralizeFilter {
    fn name(&self) -> &'static str { "pluralize" }
    fn apply(&self, value: &ContextValue, args: &[ContextValue]) -> Result<ContextValue, DjangoError> {
        let suffix = args.first().map(|a| a.to_display_string()).unwrap_or_else(|| "s".to_string());
        let parts: Vec<&str> = suffix.split(',').collect();

        let (singular, plural_suffix) = if parts.len() >= 2 {
            (parts[0], parts[1])
        } else {
            ("", parts[0])
        };

        let n = value.as_integer().unwrap_or(0);
        if n == 1 {
            Ok(ContextValue::String(singular.to_string()))
        } else {
            Ok(ContextValue::String(plural_suffix.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply_filter(name: &str, value: ContextValue, args: Vec<ContextValue>) -> ContextValue {
        default_registry().apply(name, &value, &args).unwrap()
    }

    #[test]
    fn test_lower() {
        let result = apply_filter("lower", ContextValue::from("HELLO"), vec![]);
        assert_eq!(result.to_display_string(), "hello");
    }

    #[test]
    fn test_upper() {
        let result = apply_filter("upper", ContextValue::from("hello"), vec![]);
        assert_eq!(result.to_display_string(), "HELLO");
    }

    #[test]
    fn test_title() {
        let result = apply_filter("title", ContextValue::from("hello world"), vec![]);
        assert_eq!(result.to_display_string(), "Hello World");
    }

    #[test]
    fn test_capfirst() {
        let result = apply_filter("capfirst", ContextValue::from("hello"), vec![]);
        assert_eq!(result.to_display_string(), "Hello");
    }

    #[test]
    fn test_cut() {
        let result = apply_filter("cut", ContextValue::from("hello world"), vec![ContextValue::from(" ")]);
        assert_eq!(result.to_display_string(), "helloworld");
    }

    #[test]
    fn test_truncatechars() {
        let result = apply_filter(
            "truncatechars",
            ContextValue::from("Hello World"),
            vec![ContextValue::Integer(8)],
        );
        assert_eq!(result.to_display_string(), "Hello...");
    }

    #[test]
    fn test_truncatechars_short() {
        let result = apply_filter(
            "truncatechars",
            ContextValue::from("Hi"),
            vec![ContextValue::Integer(10)],
        );
        assert_eq!(result.to_display_string(), "Hi");
    }

    #[test]
    fn test_truncatewords() {
        let result = apply_filter(
            "truncatewords",
            ContextValue::from("one two three four five"),
            vec![ContextValue::Integer(3)],
        );
        assert_eq!(result.to_display_string(), "one two three ...");
    }

    #[test]
    fn test_striptags() {
        let result = apply_filter("striptags", ContextValue::from("<b>bold</b> text"), vec![]);
        assert_eq!(result.to_display_string(), "bold text");
    }

    #[test]
    fn test_escape() {
        let result = apply_filter("escape", ContextValue::from("<b>bold</b>"), vec![]);
        assert_eq!(result.to_display_string(), "&lt;b&gt;bold&lt;/b&gt;");
    }

    #[test]
    fn test_escapejs() {
        let result = apply_filter("escapejs", ContextValue::from("it's \"good\"\nnewline"), vec![]);
        assert!(result.to_display_string().contains("\\'"));
        assert!(result.to_display_string().contains("\\n"));
    }

    #[test]
    fn test_safe() {
        let result = apply_filter("safe", ContextValue::from("<b>bold</b>"), vec![]);
        assert!(result.is_safe());
    }

    #[test]
    fn test_linebreaks() {
        let result = apply_filter("linebreaks", ContextValue::from("hello\nworld"), vec![]);
        assert_eq!(result.to_display_string(), "<p>hello<br>world</p>");
    }

    #[test]
    fn test_linebreaksbr() {
        let result = apply_filter("linebreaksbr", ContextValue::from("hello\nworld"), vec![]);
        assert_eq!(result.to_display_string(), "hello<br>world");
    }

    #[test]
    fn test_urlize() {
        let result = apply_filter("urlize", ContextValue::from("Visit https://example.com"), vec![]);
        assert!(result.to_display_string().contains("<a href="));
    }

    #[test]
    fn test_slugify() {
        let result = apply_filter("slugify", ContextValue::from("Hello World!"), vec![]);
        assert_eq!(result.to_display_string(), "hello-world");
    }

    #[test]
    fn test_center() {
        let result = apply_filter("center", ContextValue::from("hi"), vec![ContextValue::Integer(10)]);
        assert_eq!(result.to_display_string().len(), 10);
        assert!(result.to_display_string().contains("hi"));
    }

    #[test]
    fn test_ljust() {
        let result = apply_filter("ljust", ContextValue::from("hi"), vec![ContextValue::Integer(10)]);
        assert_eq!(result.to_display_string(), "hi        ");
    }

    #[test]
    fn test_rjust() {
        let result = apply_filter("rjust", ContextValue::from("hi"), vec![ContextValue::Integer(10)]);
        assert_eq!(result.to_display_string(), "        hi");
    }

    #[test]
    fn test_wordwrap() {
        let result = apply_filter(
            "wordwrap",
            ContextValue::from("This is a long sentence that should be wrapped"),
            vec![ContextValue::Integer(15)],
        );
        assert!(result.to_display_string().contains('\n'));
    }

    #[test]
    fn test_addslashes() {
        let result = apply_filter("addslashes", ContextValue::from("it's a \"test\""), vec![]);
        assert_eq!(result.to_display_string(), "it\\'s a \\\"test\\\"");
    }

    #[test]
    fn test_length() {
        let result = apply_filter(
            "length",
            ContextValue::List(vec![ContextValue::Integer(1), ContextValue::Integer(2)]),
            vec![],
        );
        assert_eq!(result.to_display_string(), "2");
    }

    #[test]
    fn test_length_string() {
        let result = apply_filter("length", ContextValue::from("hello"), vec![]);
        assert_eq!(result.to_display_string(), "5");
    }

    #[test]
    fn test_first() {
        let result = apply_filter(
            "first",
            ContextValue::List(vec![ContextValue::from("a"), ContextValue::from("b")]),
            vec![],
        );
        assert_eq!(result.to_display_string(), "a");
    }

    #[test]
    fn test_last() {
        let result = apply_filter(
            "last",
            ContextValue::List(vec![ContextValue::from("a"), ContextValue::from("b")]),
            vec![],
        );
        assert_eq!(result.to_display_string(), "b");
    }

    #[test]
    fn test_join() {
        let result = apply_filter(
            "join",
            ContextValue::List(vec![ContextValue::from("a"), ContextValue::from("b"), ContextValue::from("c")]),
            vec![ContextValue::from(", ")],
        );
        assert_eq!(result.to_display_string(), "a, b, c");
    }

    #[test]
    fn test_slice() {
        let result = apply_filter(
            "slice",
            ContextValue::List(vec![
                ContextValue::Integer(1),
                ContextValue::Integer(2),
                ContextValue::Integer(3),
                ContextValue::Integer(4),
            ]),
            vec![ContextValue::from("1:3")],
        );
        if let ContextValue::List(items) = result {
            assert_eq!(items.len(), 2);
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_add_integers() {
        let result = apply_filter("add", ContextValue::Integer(5), vec![ContextValue::Integer(3)]);
        assert_eq!(result.to_display_string(), "8");
    }

    #[test]
    fn test_add_strings() {
        let result = apply_filter("add", ContextValue::from("hello"), vec![ContextValue::from(" world")]);
        assert_eq!(result.to_display_string(), "hello world");
    }

    #[test]
    fn test_floatformat() {
        let result = apply_filter("floatformat", ContextValue::Float(3.14159), vec![ContextValue::Integer(2)]);
        assert_eq!(result.to_display_string(), "3.14");
    }

    #[test]
    fn test_filesizeformat() {
        let result = apply_filter("filesizeformat", ContextValue::Integer(1024), vec![]);
        assert_eq!(result.to_display_string(), "1.0 KB");
    }

    #[test]
    fn test_filesizeformat_bytes() {
        let result = apply_filter("filesizeformat", ContextValue::Integer(500), vec![]);
        assert_eq!(result.to_display_string(), "500 bytes");
    }

    #[test]
    fn test_filesizeformat_mb() {
        let result = apply_filter("filesizeformat", ContextValue::Integer(1_048_576), vec![]);
        assert_eq!(result.to_display_string(), "1.0 MB");
    }

    #[test]
    fn test_divisibleby() {
        let result = apply_filter("divisibleby", ContextValue::Integer(10), vec![ContextValue::Integer(5)]);
        assert_eq!(result, ContextValue::Bool(true));
    }

    #[test]
    fn test_divisibleby_false() {
        let result = apply_filter("divisibleby", ContextValue::Integer(10), vec![ContextValue::Integer(3)]);
        assert_eq!(result, ContextValue::Bool(false));
    }

    #[test]
    fn test_default() {
        let result = apply_filter("default", ContextValue::None, vec![ContextValue::from("N/A")]);
        assert_eq!(result.to_display_string(), "N/A");
    }

    #[test]
    fn test_default_with_value() {
        let result = apply_filter("default", ContextValue::from("hello"), vec![ContextValue::from("N/A")]);
        assert_eq!(result.to_display_string(), "hello");
    }

    #[test]
    fn test_default_if_none() {
        let result = apply_filter("default_if_none", ContextValue::None, vec![ContextValue::from("fallback")]);
        assert_eq!(result.to_display_string(), "fallback");
    }

    #[test]
    fn test_default_if_none_with_empty_string() {
        let result = apply_filter(
            "default_if_none",
            ContextValue::from(""),
            vec![ContextValue::from("fallback")],
        );
        assert_eq!(result.to_display_string(), "");
    }

    #[test]
    fn test_yesno() {
        assert_eq!(
            apply_filter("yesno", ContextValue::Bool(true), vec![ContextValue::from("yeah,nah")]).to_display_string(),
            "yeah"
        );
        assert_eq!(
            apply_filter("yesno", ContextValue::Bool(false), vec![ContextValue::from("yeah,nah")]).to_display_string(),
            "nah"
        );
        assert_eq!(
            apply_filter("yesno", ContextValue::None, vec![ContextValue::from("yeah,nah,dunno")]).to_display_string(),
            "dunno"
        );
    }

    #[test]
    fn test_pluralize() {
        assert_eq!(apply_filter("pluralize", ContextValue::Integer(1), vec![]).to_display_string(), "");
        assert_eq!(apply_filter("pluralize", ContextValue::Integer(2), vec![]).to_display_string(), "s");
    }

    #[test]
    fn test_pluralize_custom() {
        assert_eq!(
            apply_filter("pluralize", ContextValue::Integer(1), vec![ContextValue::from("es")]).to_display_string(),
            ""
        );
        assert_eq!(
            apply_filter("pluralize", ContextValue::Integer(2), vec![ContextValue::from("es")]).to_display_string(),
            "es"
        );
    }

    #[test]
    fn test_pluralize_singular_plural() {
        assert_eq!(
            apply_filter("pluralize", ContextValue::Integer(1), vec![ContextValue::from("y,ies")]).to_display_string(),
            "y"
        );
        assert_eq!(
            apply_filter("pluralize", ContextValue::Integer(2), vec![ContextValue::from("y,ies")]).to_display_string(),
            "ies"
        );
    }

    #[test]
    fn test_length_is() {
        let result = apply_filter(
            "length_is",
            ContextValue::from("hello"),
            vec![ContextValue::Integer(5)],
        );
        assert_eq!(result, ContextValue::Bool(true));
    }

    #[test]
    fn test_random_from_list() {
        let result = apply_filter(
            "random",
            ContextValue::List(vec![ContextValue::from("only")]),
            vec![],
        );
        assert_eq!(result.to_display_string(), "only");
    }

    #[test]
    fn test_unordered_list() {
        let result = apply_filter(
            "unordered_list",
            ContextValue::List(vec![ContextValue::from("a"), ContextValue::from("b")]),
            vec![],
        );
        assert!(result.to_display_string().contains("<li>a</li>"));
        assert!(result.to_display_string().contains("<li>b</li>"));
    }

    #[test]
    fn test_unknown_filter() {
        let result = default_registry().apply("nonexistent", &ContextValue::None, &[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_slice_negative() {
        let result = apply_filter(
            "slice",
            ContextValue::List(vec![
                ContextValue::Integer(1),
                ContextValue::Integer(2),
                ContextValue::Integer(3),
            ]),
            vec![ContextValue::from(":-1")],
        );
        if let ContextValue::List(items) = result {
            assert_eq!(items.len(), 2);
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_first_string() {
        let result = apply_filter("first", ContextValue::from("hello"), vec![]);
        assert_eq!(result.to_display_string(), "h");
    }

    #[test]
    fn test_last_string() {
        let result = apply_filter("last", ContextValue::from("hello"), vec![]);
        assert_eq!(result.to_display_string(), "o");
    }
}
