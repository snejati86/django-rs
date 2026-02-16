# QuerySets and the ORM

This reference covers the django-rs ORM query system: building queries with `QuerySet`, filtering with `Q` objects and lookups, expressions, aggregations, and raw SQL.

---

## QuerySet

A `QuerySet` represents a lazy, chainable SQL query. Methods on `QuerySet` return a new `QuerySet` without executing SQL -- the query is only sent to the database when you evaluate the queryset (e.g., by calling `.to_sql()`).

```rust
use django_rs_db::query::queryset::QuerySet;
use django_rs_db::query::lookups::{Q, Lookup};
use django_rs_db::value::Value;

let qs = QuerySet::new("blog", "post")
    .filter(Q::new("published", "exact", Value::Bool(true)))
    .order_by(vec![OrderBy::desc("created_at")])
    .limit(10);
```

### QuerySet methods

| Method | Description |
|--------|-------------|
| `filter(Q)` | Add a WHERE condition (AND with existing filters) |
| `exclude(Q)` | Add a negated WHERE condition |
| `order_by(Vec<OrderBy>)` | Set the ORDER BY clause |
| `limit(usize)` | Set a LIMIT |
| `offset(usize)` | Set an OFFSET |
| `distinct()` | Add DISTINCT to the SELECT |
| `values(Vec<&str>)` | Select specific columns only |
| `annotate(name, Expression)` | Add a computed column |
| `aggregate(Vec<Aggregate>)` | Compute aggregates (COUNT, SUM, etc.) |
| `select_related(Vec<&str>)` | JOIN related tables |
| `using(&str)` | Route to a specific database |

### Generating SQL

```rust
use django_rs_db::query::compiler::DatabaseBackendType;

let backend = DatabaseBackendType::PostgreSQL;
let (sql, params) = qs.to_sql(&backend);
// sql: "SELECT ... FROM \"blog_post\" WHERE \"published\" = $1 ORDER BY \"created_at\" DESC LIMIT 10"
// params: [Value::Bool(true)]
```

---

## Q objects and lookups

`Q` objects represent query conditions. They can be composed with `&` (AND), `|` (OR), and `!` (NOT):

```rust
use django_rs_db::query::lookups::{Q, Lookup};
use django_rs_db::value::Value;

// Simple filter
let q = Q::filter("published", Lookup::Exact(Value::Bool(true)));

// AND
let q = Q::filter("published", Lookup::Exact(Value::Bool(true)))
    & Q::filter("author_id", Lookup::Exact(Value::Int(1)));

// OR
let q = Q::filter("status", Lookup::Exact(Value::String("draft".into())))
    | Q::filter("status", Lookup::Exact(Value::String("review".into())));

// NOT
let q = !Q::filter("deleted", Lookup::Exact(Value::Bool(true)));

// Complex composition
let q = (Q::filter("published", Lookup::Exact(Value::Bool(true)))
    & Q::filter("category", Lookup::Exact(Value::String("tech".into()))))
    | Q::filter("featured", Lookup::Exact(Value::Bool(true)));
```

### Lookup reference

| Lookup | SQL | Example |
|--------|-----|---------|
| `Exact(value)` | `= value` | `Lookup::Exact(Value::Int(42))` |
| `IExact(value)` | `ILIKE value` | Case-insensitive exact match |
| `Contains(value)` | `LIKE '%value%'` | Substring match |
| `IContains(value)` | `ILIKE '%value%'` | Case-insensitive substring |
| `StartsWith(value)` | `LIKE 'value%'` | Prefix match |
| `EndsWith(value)` | `LIKE '%value'` | Suffix match |
| `Gt(value)` | `> value` | Greater than |
| `Gte(value)` | `>= value` | Greater than or equal |
| `Lt(value)` | `< value` | Less than |
| `Lte(value)` | `<= value` | Less than or equal |
| `In(values)` | `IN (...)` | Membership test |
| `Range(low, high)` | `BETWEEN low AND high` | Range check |
| `IsNull(bool)` | `IS NULL` / `IS NOT NULL` | Null check |
| `Regex(pattern)` | `~ pattern` | Regex match (PostgreSQL) |
| `IRegex(pattern)` | `~* pattern` | Case-insensitive regex |

### Using Q with QuerySet

```rust
let qs = QuerySet::new("blog", "post")
    .filter(
        Q::filter("published", Lookup::Exact(Value::Bool(true)))
        & Q::filter("created_at", Lookup::Gte(Value::String("2025-01-01".into())))
    )
    .exclude(Q::filter("deleted", Lookup::Exact(Value::Bool(true))));
```

---

## Expressions

Expressions represent SQL expressions that can be used in annotations, aggregations, and ordering.

### Database functions

```rust
use django_rs_db::query::expressions::functions::*;

// LOWER("email")
Lower::new("email");

// UPPER("name")
Upper::new("name");

// COALESCE("nickname", 'Anonymous')
Coalesce::new(vec!["nickname"], "Anonymous");

// LENGTH("title")
Length::new("title");

// NOW()
Now;
```

### Aggregates

```rust
use django_rs_db::query::expressions::aggregates::*;

Count::new("id");            // COUNT("id")
Sum::new("amount");          // SUM("amount")
Avg::new("score");           // AVG("score")
Max::new("created_at");      // MAX("created_at")
Min::new("price");           // MIN("price")
Count::new("id").distinct(); // COUNT(DISTINCT "id")
```

### Window functions

```rust
use django_rs_db::query::expressions::window::*;

// ROW_NUMBER() OVER (ORDER BY created_at DESC)
RowNumber::new().order_by(vec![OrderBy::desc("created_at")]);

// SUM(amount) OVER (PARTITION BY user_id ORDER BY created_at)
WindowExpression::new(Sum::new("amount"))
    .partition_by(vec!["user_id"])
    .order_by(vec![OrderBy::asc("created_at")]);
```

### Full-text search

```rust
use django_rs_db::query::expressions::search::*;

// to_tsvector('english', "title" || ' ' || "content")
SearchVector::new(vec!["title", "content"]).config("english");

// plainto_tsquery('english', 'search terms')
SearchQuery::new("search terms").config("english");

// Trigram similarity
TrigramSimilarity::new("name", "search_term");
```

### Subqueries

```rust
use django_rs_db::query::expressions::subquery::*;

// Subquery
let subquery = Subquery::new(
    QuerySet::new("blog", "comment")
        .filter(Q::filter("post_id", Lookup::Exact(Value::Int(1))))
        .values(vec!["id"])
);

// EXISTS subquery
let exists = Exists::new(
    QuerySet::new("blog", "comment")
        .filter(Q::new("post_id", "exact", Value::Int(1)))
);
```

---

## Raw SQL

For queries that cannot be expressed through the ORM, use raw SQL:

```rust
use django_rs_db::query::raw::RawSQL;

let raw = RawSQL::new(
    "SELECT * FROM blog_post WHERE EXTRACT(YEAR FROM created_at) = $1",
    vec![Value::Int(2025)],
);
```

---

## Database routing

For multi-database setups, the database router determines which database a query is sent to:

```rust
use django_rs_db::router::DatabaseRouter;

let router = DatabaseRouter::new();
router.add_route("analytics", |app, model| {
    app == "analytics"
});

// Route a queryset to a specific database
let qs = QuerySet::new("analytics", "event").using("analytics_db");
```

---

## Comparison with Django

| Django (Python) | django-rs (Rust) |
|-----------------|------------------|
| `Post.objects.filter(published=True)` | `QuerySet::new("blog", "post").filter(Q::filter("published", Lookup::Exact(Value::Bool(true))))` |
| `Q(status='draft') \| Q(status='review')` | `Q::filter("status", Lookup::Exact(..)) \| Q::filter("status", Lookup::Exact(..))` |
| `.exclude(deleted=True)` | `.exclude(Q::filter("deleted", Lookup::Exact(Value::Bool(true))))` |
| `.order_by('-created_at')` | `.order_by(vec![OrderBy::desc("created_at")])` |
| `.annotate(count=Count('id'))` | `.annotate("count", Count::new("id"))` |
| `.aggregate(Sum('amount'))` | `.aggregate(vec![Sum::new("amount")])` |
| `from django.db.models import F` | Expressions are used directly |
| `RawSQL("SELECT ...")` | `RawSQL::new("SELECT ...", params)` |
