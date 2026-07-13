use serde::{Deserialize, Serialize};
use std::path::Path;

const CONTEXT_FILE: &str = ".broccoli";

#[derive(Serialize, Deserialize, Default)]
pub struct ProjectContext {
    pub contest: Option<String>,
    pub problem: Option<String>,
    pub language: Option<String>,
}

/// Walk up from cwd looking for a .broccoli file
pub fn discover_context() -> Option<ProjectContext> {
    let cwd = std::env::current_dir().ok()?;
    for dir in cwd.ancestors() {
        let path = dir.join(CONTEXT_FILE);
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(ctx) = toml::from_str(&content) {
                    return Some(ctx);
                }
            }
        }
    }
    None
}

/// Write .broccoli file in cwd
pub fn save_context(ctx: &ProjectContext) -> anyhow::Result<()> {
    let content = toml::to_string_pretty(ctx)?;
    std::fs::write(CONTEXT_FILE, content)?;
    Ok(())
}

/// Maps to server-side language identifiers expected by the `standard-languages` plugin.
pub fn detect_language(filename: &str) -> Option<&'static str> {
    let ext = Path::new(filename).extension()?.to_str()?;
    Some(match ext {
        "py" => "python3",
        "cpp" | "cc" | "cxx" => "cpp",
        "c" => "c",
        "java" => "java",
        "rs" => "rust",
        "go" => "go",
        "js" => "javascript",
        "ts" => "typescript",
        "kt" => "kotlin",
        "swift" => "swift",
        "rb" => "ruby",
        "hs" => "haskell",
        "cs" => "csharp",
        _ => return None,
    })
}
