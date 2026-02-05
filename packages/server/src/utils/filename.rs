use std::path::Path;

/// Result of validating a flat filename.
#[derive(Debug)]
pub enum FilenameError {
    /// Filename is empty or whitespace-only.
    Empty,
    /// Filename contains path separators (`/` or `\`).
    ContainsPathSeparator,
    /// Filename contains path traversal patterns (`..`).
    PathTraversal,
    /// Filename contains null bytes.
    NullByte,
    /// Filename starts with a dot (hidden file).
    Hidden,
}

impl FilenameError {
    /// Returns a human-readable error message.
    pub fn message(&self) -> &'static str {
        match self {
            Self::Empty => "Filename cannot be empty",
            Self::ContainsPathSeparator => "Invalid filename: path separators are not allowed",
            Self::PathTraversal => "Invalid filename: '..' is not allowed",
            Self::NullByte => "Invalid filename: null bytes are not allowed",
            Self::Hidden => "Invalid filename: hidden files (starting with '.') are not allowed",
        }
    }
}

/// Validates a flat filename (no directory components allowed).
pub fn validate_flat_filename(filename: &str) -> Result<&str, FilenameError> {
    let trimmed = filename.trim();

    if trimmed.is_empty() {
        return Err(FilenameError::Empty);
    }

    if trimmed.contains('\0') {
        return Err(FilenameError::NullByte);
    }

    if trimmed.contains('/') || trimmed.contains('\\') {
        return Err(FilenameError::ContainsPathSeparator);
    }

    if trimmed == ".." || trimmed.contains("..") {
        return Err(FilenameError::PathTraversal);
    }

    if trimmed.starts_with('.') {
        return Err(FilenameError::Hidden);
    }

    Ok(trimmed)
}

/// Checks if a path string contains path traversal patterns.
pub fn contains_path_traversal(path: &str) -> bool {
    path == ".."
        || path.starts_with("../")
        || path.contains("/../")
        || path.ends_with("/..")
        || path.starts_with("..\\")
        || path.contains("\\..\\")
        || path.ends_with("\\..")
}

/// Extracts the filename stem (without extension) from a path.
pub fn extract_stem(path: &str) -> Option<(&str, &str)> {
    let filename = Path::new(path).file_name()?.to_str()?;
    let (stem, ext) = filename.rsplit_once('.')?;

    if stem.is_empty() {
        return None;
    }

    let stem_end = path.len() - ext.len() - 1; // -1 for the dot
    Some((&path[..stem_end], ext))
}

/// Extracts the directory and filename from a path.
pub fn split_dir_filename(path: &str) -> (&str, &str) {
    match path.rfind('/') {
        Some(pos) => (&path[..pos], &path[pos + 1..]),
        None => ("", path),
    }
}

/// Checks if a directory path indicates a "sample" test case.
pub fn is_sample_directory(dir: &str) -> bool {
    let lower = dir.to_lowercase();
    lower == "sample" || lower.ends_with("/sample")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_flat_filename_accepts_valid_names() {
        assert!(validate_flat_filename("solution.cpp").is_ok());
        assert!(validate_flat_filename("Main.java").is_ok());
        assert!(validate_flat_filename("test_file.py").is_ok());
        assert!(validate_flat_filename("file-name.rs").is_ok());
        assert!(validate_flat_filename("  padded.txt  ").is_ok());
    }

    #[test]
    fn validate_flat_filename_rejects_empty() {
        assert!(matches!(
            validate_flat_filename(""),
            Err(FilenameError::Empty)
        ));
        assert!(matches!(
            validate_flat_filename("   "),
            Err(FilenameError::Empty)
        ));
    }

    #[test]
    fn validate_flat_filename_rejects_path_separators() {
        assert!(matches!(
            validate_flat_filename("src/main.rs"),
            Err(FilenameError::ContainsPathSeparator)
        ));
        assert!(matches!(
            validate_flat_filename("src\\main.rs"),
            Err(FilenameError::ContainsPathSeparator)
        ));
    }

    #[test]
    fn validate_flat_filename_rejects_path_traversal() {
        assert!(matches!(
            validate_flat_filename(".."),
            Err(FilenameError::PathTraversal)
        ));
        assert!(matches!(
            validate_flat_filename("foo..bar"),
            Err(FilenameError::PathTraversal)
        ));
    }

    #[test]
    fn validate_flat_filename_rejects_null_bytes() {
        assert!(matches!(
            validate_flat_filename("foo\0bar"),
            Err(FilenameError::NullByte)
        ));
    }

    #[test]
    fn validate_flat_filename_rejects_hidden_files() {
        assert!(matches!(
            validate_flat_filename(".hidden"),
            Err(FilenameError::Hidden)
        ));
        assert!(matches!(
            validate_flat_filename(".gitignore"),
            Err(FilenameError::Hidden)
        ));
    }

    #[test]
    fn contains_path_traversal_detects_patterns() {
        assert!(contains_path_traversal(".."));
        assert!(contains_path_traversal("../foo"));
        assert!(contains_path_traversal("foo/../bar"));
        assert!(contains_path_traversal("foo/.."));
        assert!(!contains_path_traversal("foo/bar"));
        assert!(!contains_path_traversal("foo..bar")); // Not a path component
    }

    #[test]
    fn extract_stem_works() {
        assert_eq!(extract_stem("1.in"), Some(("1", "in")));
        assert_eq!(extract_stem("sample/1.in"), Some(("sample/1", "in")));
        assert_eq!(extract_stem("no_ext"), None);
        assert_eq!(extract_stem(".hidden"), None); // stem is empty
    }

    #[test]
    fn split_dir_filename_works() {
        assert_eq!(split_dir_filename("sample/1.in"), ("sample", "1.in"));
        assert_eq!(split_dir_filename("a/b/c.txt"), ("a/b", "c.txt"));
        assert_eq!(split_dir_filename("file.txt"), ("", "file.txt"));
    }

    #[test]
    fn is_sample_directory_works() {
        assert!(is_sample_directory("sample"));
        assert!(is_sample_directory("Sample"));
        assert!(is_sample_directory("SAMPLE"));
        assert!(is_sample_directory("tests/sample"));
        assert!(!is_sample_directory("samples"));
        assert!(!is_sample_directory("main"));
    }
}
