//! Development configuration for plugin builds and file watching.
//!
//! Reads optional `broccoli.dev.toml` from the plugin directory. When absent,
//! all values fall back to sensible defaults derived from `plugin.toml`.
//!
//! ## File format
//!
//! ```toml
//! [watch]
//! # Extra glob patterns to ignore (extends built-in ignores).
//! # Built-in: target/, .git/, node_modules/, and [web].root from plugin.toml
//! ignore = ["*.log", "tmp/", "coverage/"]
//!
//! [build]
//! # Directory containing the frontend source (has package.json).
//! # Default: auto-detected from [web].root parent, then web/, frontend/, or root.
//! frontend_dir = "client"
//!
//! # Command to install frontend dependencies. Default: "pnpm install --ignore-workspace"
//! frontend_install_cmd = "npm install"
//!
//! # Command to build the frontend (one-shot). Default: "pnpm build"
//! frontend_build_cmd = "npm run build"
//!
//! # Command to start the frontend dev server (long-running, used by `watch`).
//! # Default: "pnpm dev"
//! frontend_dev_cmd = "npm run dev"
//! ```

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Filename for the dev config.
const DEV_CONFIG_FILE: &str = "broccoli.dev.toml";

/// Directories always ignored by the watcher (non-configurable).
pub const BUILTIN_IGNORE_DIRS: &[&str] = &["target", ".git", "node_modules"];

/// The parsed dev config with defaults applied.
pub struct ResolvedDevConfig {
    /// Extra ignore patterns from the config file.
    pub extra_ignores: Vec<String>,
    /// Resolved frontend source directory (absolute path), if any.
    pub frontend_dir: Option<PathBuf>,
    /// Frontend install command + args (one-shot). Default: ["pnpm", "install",
    /// "--ignore-workspace"]
    pub frontend_install_cmd: Vec<String>,
    /// Frontend build command + args (one-shot). Default: ["pnpm", "build"]
    pub frontend_build_cmd: Vec<String>,
    /// Frontend dev command + args (long-running, watch mode). Default: ["pnpm", "dev"]
    pub frontend_dev_cmd: Vec<String>,
}

/// Raw config as deserialized from `broccoli.dev.toml`.
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawDevConfig {
    watch: RawWatchConfig,
    build: RawBuildConfig,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawWatchConfig {
    /// Extra glob patterns to ignore.
    ignore: Vec<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawBuildConfig {
    /// Explicit frontend source directory (relative to plugin root).
    frontend_dir: Option<String>,
    /// Frontend install command (one-shot). Default: "pnpm install --ignore-workspace"
    frontend_install_cmd: Option<String>,
    /// Frontend build command (one-shot). Default: "pnpm build"
    frontend_build_cmd: Option<String>,
    /// Frontend dev command (long-running, watch mode). Default: "pnpm dev"
    frontend_dev_cmd: Option<String>,
}

/// Resolve the dev config for a plugin directory.
///
/// `web_root` is the `[web].root` value from `plugin.toml` (e.g. `"frontend/dist"`),
/// used to derive the default frontend source directory.
pub fn resolve(plugin_dir: &Path, web_root: Option<&str>) -> ResolvedDevConfig {
    let raw = load_raw(plugin_dir);

    let frontend_dir =
        resolve_frontend_dir(plugin_dir, web_root, raw.build.frontend_dir.as_deref());

    let frontend_install_cmd = match raw.build.frontend_install_cmd {
        Some(cmd) => shell_words(cmd.trim()),
        None => vec!["pnpm".into(), "install".into(), "--ignore-workspace".into()],
    };

    let frontend_build_cmd = match raw.build.frontend_build_cmd {
        Some(cmd) => shell_words(cmd.trim()),
        None => vec!["pnpm".into(), "build".into()],
    };

    let frontend_dev_cmd = match raw.build.frontend_dev_cmd {
        Some(cmd) => shell_words(cmd.trim()),
        None => vec!["pnpm".into(), "dev".into()],
    };

    ResolvedDevConfig {
        extra_ignores: raw.watch.ignore,
        frontend_dir,
        frontend_install_cmd,
        frontend_build_cmd,
        frontend_dev_cmd,
    }
}

/// Load and parse the raw config file. Returns defaults if missing or invalid.
fn load_raw(plugin_dir: &Path) -> RawDevConfig {
    let path = plugin_dir.join(DEV_CONFIG_FILE);
    match std::fs::read_to_string(&path) {
        Ok(content) => match toml::from_str(&content) {
            Ok(config) => config,
            Err(e) => {
                eprintln!(
                    "Warning: failed to parse {}: {}. Using defaults.",
                    DEV_CONFIG_FILE, e
                );
                RawDevConfig::default()
            }
        },
        Err(_) => RawDevConfig::default(),
    }
}

/// Resolve the frontend source directory.
///
/// Priority:
/// 1. Explicit `build.frontend_dir` from config
/// 2. Derived from `[web].root` in plugin.toml (parent of the dist dir)
/// 3. Convention: `web/`, `frontend/`, or root (whichever has `package.json`)
fn resolve_frontend_dir(
    plugin_dir: &Path,
    web_root: Option<&str>,
    explicit: Option<&str>,
) -> Option<PathBuf> {
    if let Some(dir) = explicit {
        return Some(plugin_dir.join(dir));
    }

    if let Some(root) = web_root {
        let root_path = Path::new(root);
        if let Some(parent) = root_path.parent().filter(|p| !p.as_os_str().is_empty()) {
            let candidate = plugin_dir.join(parent);
            if candidate.join("package.json").exists() {
                return Some(candidate);
            }
        }
    }

    for subdir in &["web", "frontend"] {
        let candidate = plugin_dir.join(subdir);
        if candidate.join("package.json").exists() {
            return Some(candidate);
        }
    }

    if plugin_dir.join("package.json").exists() {
        return Some(plugin_dir.to_path_buf());
    }

    None
}

/// Split a command string into program + args, respecting shell quoting.
fn shell_words(cmd: &str) -> Vec<String> {
    shlex::split(cmd).unwrap_or_else(|| cmd.split_whitespace().map(String::from).collect())
}

/// Check whether a path component matches any of the built-in or extra ignore patterns.
///
/// This checks directory names in the path. Glob-style patterns (e.g. `*.log`)
/// are matched against the filename of the changed file.
pub fn should_ignore(
    relative: &Path,
    extra_ignores: &[String],
    web_root_relative: Option<&Path>,
) -> bool {
    let components: Vec<_> = relative.components().collect();

    // Check if any path component is a built-in ignore dir
    for comp in &components {
        let s = comp.as_os_str().to_string_lossy();
        if BUILTIN_IGNORE_DIRS.contains(&s.as_ref()) {
            return true;
        }
    }

    // Check if path is inside web_root (build output)
    if web_root_relative.is_some_and(|wr| relative.starts_with(wr)) {
        return true;
    }

    // Check extra ignore patterns against path components and filename
    let filename = relative.file_name().unwrap_or_default().to_string_lossy();

    for pattern in extra_ignores {
        let pat = pattern.trim_end_matches('/');

        // Directory pattern: check path components
        if pattern.ends_with('/') {
            for comp in &components {
                if comp.as_os_str().to_string_lossy() == pat {
                    return true;
                }
            }
            continue;
        }

        // Glob pattern with * — match against filename
        if pat.contains('*') {
            if glob_match(pat, &filename) {
                return true;
            }
            continue;
        }

        // Exact match against filename or directory component
        if filename == pat {
            return true;
        }
        for comp in &components {
            if comp.as_os_str().to_string_lossy() == pat {
                return true;
            }
        }
    }

    false
}

/// Minimal glob matching (supports `*` and `?` only).
fn glob_match(pattern: &str, text: &str) -> bool {
    let mut p = pattern.chars().peekable();
    let mut t = text.chars().peekable();

    while p.peek().is_some() || t.peek().is_some() {
        match p.peek() {
            Some('*') => {
                p.next();
                if p.peek().is_none() {
                    return true; // trailing * matches everything
                }
                // Try matching rest of pattern at each position
                let remaining: String = p.collect();
                let text_remaining: String = t.collect();
                for i in 0..=text_remaining.len() {
                    if glob_match(&remaining, &text_remaining[i..]) {
                        return true;
                    }
                }
                return false;
            }
            Some('?') => {
                p.next();
                if t.next().is_none() {
                    return false;
                }
            }
            Some(&pc) => {
                p.next();
                match t.next() {
                    Some(tc) if tc == pc => {}
                    _ => return false,
                }
            }
            None => return false,
        }
    }

    true
}

/// Classify a changed file as backend, frontend, or unknown.
///
/// Uses file extension to determine the build target. The caller uses
/// `frontend_dir` to disambiguate files that could be either (e.g. `.json`
/// files exist in both Rust and JS projects, but we only treat them as
/// frontend if they're inside the frontend directory).
pub enum FileKind {
    Backend,
    Frontend,
    PluginManifest,
    Unknown,
}

pub fn classify_file(path: &Path, plugin_dir: &Path, frontend_dir: Option<&Path>) -> FileKind {
    let relative = path.strip_prefix(plugin_dir).unwrap_or(path);
    let filename = relative.file_name().unwrap_or_default().to_string_lossy();

    if filename == "plugin.toml" {
        return FileKind::PluginManifest;
    }

    let ext = path.extension().unwrap_or_default().to_string_lossy();

    let in_fe_dir = frontend_dir.is_some_and(|fd| path.starts_with(fd));

    match ext.as_ref() {
        // Unambiguously backend
        "rs" => FileKind::Backend,
        // Unambiguously frontend
        "tsx" | "jsx" | "css" | "scss" | "less" | "svg" | "html" => FileKind::Frontend,
        // Ambiguous — use location to disambiguate
        "ts" | "js" | "json" => {
            if in_fe_dir {
                FileKind::Frontend
            } else {
                // .ts/.js outside frontend dir is unknown (could be tooling scripts)
                // .toml outside frontend dir is backend (Cargo.toml, etc.)
                FileKind::Unknown
            }
        }
        "toml" => {
            if in_fe_dir {
                FileKind::Unknown
            } else {
                FileKind::Backend
            }
        }
        _ => FileKind::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glob_match() {
        assert!(glob_match("*.log", "test.log"));
        assert!(glob_match("*.log", "app.log"));
        assert!(!glob_match("*.log", "test.txt"));
        assert!(glob_match("*.rs", "lib.rs"));
        assert!(glob_match("test_*", "test_foo"));
        assert!(!glob_match("test_*", "prod_foo"));
        assert!(glob_match("*", "anything"));
        assert!(glob_match("?.rs", "a.rs"));
        assert!(!glob_match("?.rs", "ab.rs"));
    }

    #[test]
    fn test_should_ignore_builtin() {
        assert!(should_ignore(Path::new("target/debug/foo.rs"), &[], None));
        assert!(should_ignore(Path::new(".git/config"), &[], None));
        assert!(should_ignore(
            Path::new("node_modules/pkg/index.js"),
            &[],
            None
        ));
        assert!(!should_ignore(Path::new("src/lib.rs"), &[], None));
    }

    #[test]
    fn test_should_ignore_web_root() {
        let wr = Path::new("frontend/dist");
        assert!(should_ignore(
            Path::new("frontend/dist/index.js"),
            &[],
            Some(wr)
        ));
        assert!(!should_ignore(
            Path::new("frontend/src/App.tsx"),
            &[],
            Some(wr)
        ));
    }

    #[test]
    fn test_should_ignore_extra_patterns() {
        let extras = vec!["*.log".to_string(), "tmp/".to_string()];
        assert!(should_ignore(Path::new("app.log"), &extras, None));
        assert!(should_ignore(Path::new("tmp/cache"), &extras, None));
        assert!(!should_ignore(Path::new("src/main.rs"), &extras, None));
    }

    #[test]
    fn test_shell_words() {
        assert_eq!(shell_words("pnpm build"), vec!["pnpm", "build"]);
        assert_eq!(shell_words("npm run build"), vec!["npm", "run", "build"]);
        assert_eq!(shell_words("bun build"), vec!["bun", "build"]);
        assert_eq!(
            shell_words(r#"sh -c "npm run build""#),
            vec!["sh", "-c", "npm run build"]
        );
    }
}
