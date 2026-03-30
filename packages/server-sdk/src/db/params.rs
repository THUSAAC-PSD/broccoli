use serde_json::Value as JsonValue;

/// Manages `$N` parameter numbering and argument collection for parameterized SQL.
///
/// Use `bind()` to get the next `$N` placeholder and push the corresponding arg.
/// The returned placeholder string embeds directly into `format!` SQL strings.
///
/// For nullable values (`Option<T>`), wrap in `json!()` so `None` serializes as
/// JSON null: `p.bind(json!(maybe_value))`. Non-optional types (`i32`, `&str`,
/// `f64`) can be passed directly.
///
/// ```ignore
/// let mut p = Params::new();
/// let sql = format!(
///     "UPDATE foo SET bar = {} WHERE id = {}",
///     p.bind("hello"), p.bind(42)
/// );
/// // sql = "UPDATE foo SET bar = $1 WHERE id = $2"
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
