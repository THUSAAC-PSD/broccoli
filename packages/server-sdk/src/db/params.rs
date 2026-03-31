use serde_json::Value as JsonValue;

/// Manages `$N` parameter numbering and argument collection for parameterized SQL.
///
/// Use `bind()` to get the next `$N` placeholder and push the corresponding arg.
/// The returned placeholder string embeds directly into `format!` SQL strings.
///
/// For nullable values, use `json!()` to convert `Option<T>` -> JSON null, and
/// add `::int` / `::text` casts in the SQL for nullable columns where Postgres
/// needs a type hint.
///
/// ```ignore
/// let mut p = Params::new();
/// let sql = format!(
///     "INSERT INTO foo (name, score) VALUES ({}, {})",
///     p.bind("hello"), p.bind(42)
/// );
/// db_execute_with_args(&sql, &p.into_args())?;
/// ```
pub struct Params {
    args: Vec<JsonValue>,
    idx: usize,
}

impl Params {
    pub fn new() -> Self {
        Self {
            args: Vec::new(),
            idx: 1,
        }
    }

    /// Push an arg and return its `$N` placeholder string.
    pub fn bind(&mut self, value: impl Into<JsonValue>) -> String {
        let placeholder = format!("${}", self.idx);
        self.args.push(value.into());
        self.idx += 1;
        placeholder
    }

    pub fn into_args(self) -> Vec<JsonValue> {
        self.args
    }
}

impl Default for Params {
    fn default() -> Self {
        Self::new()
    }
}
