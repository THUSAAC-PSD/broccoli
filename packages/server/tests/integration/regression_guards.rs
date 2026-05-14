//! Static-source regression guards for the server crate.
//!
//! UP#14g — `tokio::task::block_in_place` was removed from every host
//! function in UP#13 (see roadmap entry at
//! `docs/profiling/run-2/roadmap.md`). Re-parking a tokio worker thread
//! from a `spawn_blocking` task is harmful and was found to starve the
//! API runtime under load. This file holds a structural invariant: no
//! `block_in_place(` may reappear in `packages/server/src/host_funcs/`.
//!
//! The original UP#14g acceptance asked for a dynamic counter
//! (`host_fn_block_in_place_total`) check at the end of a 60s stress
//! wave. That formulation is weaker than a static-source guard: the
//! counter only increments if a future host function explicitly opts
//! in via `record_block_in_place_regression`, so a careless
//! re-introduction would silently bypass the check. A static scan is
//! the invariant we actually want.

use std::fs;
use std::path::{Path, PathBuf};

/// Returns the path to `packages/server/src/host_funcs/` from the
/// crate manifest dir of the `server` package.
fn host_funcs_root() -> PathBuf {
    // CARGO_MANIFEST_DIR points at packages/server when this test runs.
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir).join("src").join("host_funcs")
}

/// Recursively collect `.rs` files under `dir`.
fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(err) => panic!("failed to read {}: {err}", dir.display()),
    };
    for entry in entries {
        let entry = entry.expect("read_dir entry");
        let path = entry.path();
        let file_type = entry.file_type().expect("file_type");
        if file_type.is_dir() {
            collect_rs_files(&path, out);
        } else if file_type.is_file() && path.extension().and_then(|s| s.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

/// True iff the character immediately preceding the match position is
/// part of an identifier (alphanumeric or `_`). In that case
/// `block_in_place(` is the tail of a longer identifier such as
/// `record_block_in_place_regression(` and is not a real toxic call.
fn match_is_inside_identifier(line: &str, match_start: usize) -> bool {
    if match_start == 0 {
        return false;
    }
    // Walk back to the previous char boundary.
    let bytes = line.as_bytes();
    let mut i = match_start;
    while i > 0 && !line.is_char_boundary(i - 1) {
        i -= 1;
    }
    if i == 0 {
        return false;
    }
    let prev_byte = bytes[i - 1];
    prev_byte.is_ascii_alphanumeric() || prev_byte == b'_'
}

/// True iff this line is a `//` comment (single-line). We use this to
/// allow-list the documentation around the regression sentinel.
fn line_is_comment(line: &str) -> bool {
    line.trim_start().starts_with("//")
}

#[derive(Debug)]
struct Offender {
    path: PathBuf,
    line_no: usize,
    line: String,
}

/// UP#14g: scan `packages/server/src/host_funcs/` for the literal
/// `block_in_place(` token outside identifier and comment context.
/// Fail loudly with file/line offenders if any are found.
///
/// See also UP#13 (the collapse that removed the toxic pattern) at
/// `docs/profiling/run-2/roadmap.md`.
#[test]
fn host_funcs_must_not_use_tokio_block_in_place() {
    const NEEDLE: &str = "block_in_place(";

    let root = host_funcs_root();
    assert!(
        root.is_dir(),
        "expected host_funcs directory at {}",
        root.display()
    );

    let mut files = Vec::new();
    collect_rs_files(&root, &mut files);
    assert!(
        !files.is_empty(),
        "no .rs files found under {}; the regression guard would silently pass",
        root.display()
    );

    let mut offenders: Vec<Offender> = Vec::new();

    for path in &files {
        let contents =
            fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        for (idx, line) in contents.lines().enumerate() {
            // Allowlist: comment-only lines (the regression-guard doc
            // comments in host_funcs/mod.rs reference the token).
            if line_is_comment(line) {
                continue;
            }
            // Scan all occurrences on the line (very unlikely to be
            // more than one, but cheap to be exhaustive).
            let mut search_from = 0usize;
            while let Some(rel) = line[search_from..].find(NEEDLE) {
                let abs = search_from + rel;
                if !match_is_inside_identifier(line, abs) {
                    offenders.push(Offender {
                        path: path.clone(),
                        line_no: idx + 1,
                        line: line.to_string(),
                    });
                    break;
                }
                search_from = abs + NEEDLE.len();
            }
        }
    }

    if !offenders.is_empty() {
        let mut msg = String::from(
            "UP#14g regression: tokio::task::block_in_place reintroduced in host_funcs/. \
             This re-parks tokio worker threads and was removed in UP#13. \
             Offenders:\n",
        );
        for o in &offenders {
            msg.push_str(&format!(
                "  {}:{}  {}\n",
                o.path.display(),
                o.line_no,
                o.line.trim_end()
            ));
        }
        msg.push_str("If this is legitimate, document the rationale and remove this test.");
        panic!("{msg}");
    }
}

#[cfg(test)]
mod helper_tests {
    use super::*;

    #[test]
    fn identifier_context_is_recognized() {
        // Hypothetical user-defined wrapper `my_block_in_place(` — the char
        // immediately before `block_in_place(` is `_`, so the guard must
        // treat the match as an identifier tail and skip it.
        let line = "    my_block_in_place(|| {});";
        let pos = line.find("block_in_place(").expect("substring present");
        assert!(match_is_inside_identifier(line, pos));
    }

    #[test]
    fn path_qualified_call_is_not_identifier_context() {
        let line = "    tokio::task::block_in_place(|| { /* ... */ });";
        let pos = line.find("block_in_place(").expect("substring present");
        assert!(!match_is_inside_identifier(line, pos));
    }

    #[test]
    fn bare_call_at_line_start_is_not_identifier_context() {
        let line = "block_in_place(|| {});";
        let pos = line.find("block_in_place(").expect("substring present");
        assert!(!match_is_inside_identifier(line, pos));
    }

    #[test]
    fn comment_lines_are_skipped() {
        assert!(line_is_comment("// block_in_place(|| {})"));
        assert!(line_is_comment("    // mentions block_in_place"));
        assert!(!line_is_comment("    tokio::task::block_in_place(|| {});"));
    }
}
