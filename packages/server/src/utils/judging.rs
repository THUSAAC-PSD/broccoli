use std::collections::{HashMap, HashSet};

use common::language::{LanguageDefinition, resolve_language};

use crate::error::AppError;
use crate::models::submission::{SubmissionFile, SubmissionFileDto};
use crate::utils::filename::validate_flat_filename;

/// Validates the integrity of a code-based payload (files, names, and size).
pub fn validate_code_payload(
    files: &[SubmissionFileDto],
    language: &str,
    max_size: usize,
) -> Result<(), AppError> {
    if files.is_empty() {
        return Err(AppError::Validation("At least one file is required".into()));
    }

    if language.trim().is_empty() {
        return Err(AppError::Validation("Language is required".into()));
    }

    let mut total_size = 0usize;
    let mut seen_filenames = HashSet::with_capacity(files.len());

    for file in files {
        // Validate filename using shared validation
        let filename = validate_flat_filename(&file.filename)
            .map_err(|e| AppError::Validation(e.message().into()))?;

        // Check for duplicates
        if !seen_filenames.insert(filename) {
            return Err(AppError::Validation(format!(
                "Duplicate filename: '{}'",
                filename
            )));
        }

        // Content must not be empty
        if file.content.is_empty() {
            return Err(AppError::Validation(format!(
                "File '{}' cannot be empty",
                filename
            )));
        }

        total_size = total_size.saturating_add(file.content.len());
    }

    if total_size > max_size {
        return Err(AppError::Validation(format!(
            "Total code size ({} bytes) exceeds maximum ({} bytes)",
            total_size, max_size
        )));
    }

    Ok(())
}

pub fn validate_submission_contract(
    files: &[SubmissionFileDto],
    language: &str,
    submission_format: Option<HashMap<String, Vec<String>>>,
    languages: &HashMap<String, LanguageDefinition>,
) -> Result<(), AppError> {
    let language = language.trim();
    let submitted_filename = files
        .first()
        .map(|file| file.filename.as_str())
        .unwrap_or_default();

    resolve_language(language, submitted_filename, languages, &[]).map_err(AppError::Validation)?;

    let Some(submission_format) = submission_format else {
        return Ok(());
    };

    if submission_format.is_empty() {
        return Ok(());
    }

    let mut expected = submission_format.get(language).cloned().ok_or_else(|| {
        AppError::Validation(format!(
            "Language '{}' is not allowed for this problem",
            language
        ))
    })?;
    let mut actual = files
        .iter()
        .map(|file| file.filename.trim().to_string())
        .collect::<Vec<_>>();
    expected.sort();
    actual.sort();

    if actual != expected {
        return Err(AppError::Validation(format!(
            "Files for language '{}' must exactly match: {}",
            language,
            expected.join(", ")
        )));
    }

    Ok(())
}

/// Convert files to JSON value for storage.
pub fn files_to_json(files: &[SubmissionFileDto]) -> serde_json::Value {
    let submission_files: Vec<SubmissionFile> = files
        .iter()
        .map(|f| SubmissionFile {
            filename: f.filename.trim().to_string(),
            content: f.content.clone(),
        })
        .collect();
    serde_json::to_value(&submission_files).unwrap_or(serde_json::Value::Array(vec![]))
}

/// Parse files from JSON value.
pub fn files_from_json(value: &serde_json::Value) -> Vec<SubmissionFileDto> {
    serde_json::from_value::<Vec<SubmissionFile>>(value.clone())
        .unwrap_or_default()
        .into_iter()
        .map(SubmissionFileDto::from)
        .collect()
}

/// Validate language for run code requests.
pub fn validate_run_language(
    files: &[SubmissionFileDto],
    language: &str,
    languages: &HashMap<String, LanguageDefinition>,
) -> Result<(), AppError> {
    let language = language.trim();
    let submitted_filename = files
        .first()
        .map(|file| file.filename.as_str())
        .unwrap_or_default();

    resolve_language(language, submitted_filename, languages, &[]).map_err(AppError::Validation)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_language_definitions() -> HashMap<String, LanguageDefinition> {
        HashMap::from([(
            "cpp".to_string(),
            LanguageDefinition {
                compile_cmd: None,
                run_cmd: vec!["./{binary}".to_string()],
                source_filename: "solution.cpp".to_string(),
                binary_name: "solution".to_string(),
                version_cmd: None,
                basename_fallback: "solution".to_string(),
            },
        )])
    }

    #[test]
    fn test_validate_code_payload_success() {
        let files = vec![SubmissionFileDto {
            filename: "main.cpp".into(),
            content: "int main() {}".into(),
        }];
        assert!(validate_code_payload(&files, "cpp", 1000).is_ok());
    }

    #[test]
    fn test_validate_code_payload_empty_files() {
        let files = vec![];
        assert!(validate_code_payload(&files, "cpp", 1000).is_err());
    }

    #[test]
    fn test_validate_code_payload_duplicate_filenames() {
        let files = vec![
            SubmissionFileDto {
                filename: "a.cpp".into(),
                content: "a".into(),
            },
            SubmissionFileDto {
                filename: "a.cpp".into(),
                content: "b".into(),
            },
        ];
        assert!(validate_code_payload(&files, "cpp", 1000).is_err());
    }

    #[test]
    fn test_validate_code_payload_size_limit() {
        let files = vec![SubmissionFileDto {
            filename: "large.cpp".into(),
            content: "12345".into(),
        }];
        assert!(validate_code_payload(&files, "cpp", 4).is_err());
    }

    #[test]
    fn test_validate_code_payload_empty_content() {
        let files = vec![SubmissionFileDto {
            filename: "empty.cpp".into(),
            content: "".into(),
        }];
        assert!(validate_code_payload(&files, "cpp", 1000).is_err());
    }

    #[test]
    fn test_validate_submission_contract_success() {
        let files = vec![SubmissionFileDto {
            filename: "solution.cpp".into(),
            content: "int main() {}".into(),
        }];
        let mut submission_format = HashMap::new();
        submission_format.insert("cpp".into(), vec!["solution.cpp".into()]);
        let languages = create_language_definitions();
        assert!(
            validate_submission_contract(&files, "cpp", Some(submission_format), &languages)
                .is_ok()
        );
    }

    #[test]
    fn test_validate_submission_contract_missing_language() {
        let files = vec![SubmissionFileDto {
            filename: "solution.cpp".into(),
            content: "int main() {}".into(),
        }];
        let submission_format = HashMap::new();
        let languages = HashMap::new();
        assert!(
            validate_submission_contract(&files, "cpp", Some(submission_format), &languages)
                .is_err()
        );
    }

    #[test]
    fn test_validate_submission_contract_file_mismatch() {
        let files = vec![SubmissionFileDto {
            filename: "main.cpp".into(),
            content: "int main() {}".into(),
        }];
        let mut submission_format = HashMap::new();
        submission_format.insert("cpp".into(), vec!["solution.cpp".into()]);
        let languages = create_language_definitions();
        assert!(
            validate_submission_contract(&files, "cpp", Some(submission_format), &languages)
                .is_err()
        );
    }

    #[test]
    fn test_files_to_json_and_from_json() {
        let files = vec![
            SubmissionFileDto {
                filename: "a.cpp".into(),
                content: "code a".into(),
            },
            SubmissionFileDto {
                filename: "b.cpp".into(),
                content: "code b".into(),
            },
        ];
        let json = files_to_json(&files);
        let parsed_files = files_from_json(&json);
        assert_eq!(files, parsed_files);
    }

    #[test]
    fn test_files_from_json_invalid() {
        let json = serde_json::json!({"invalid": "data"});
        let parsed_files = files_from_json(&json);
        assert!(parsed_files.is_empty());
    }

    #[test]
    fn test_files_to_json_empty() {
        let files: Vec<SubmissionFileDto> = vec![];
        let json = files_to_json(&files);
        assert_eq!(json, serde_json::Value::Array(vec![]));
    }

    #[test]
    fn test_files_from_json_empty() {
        let json = serde_json::json!([]);
        let parsed_files = files_from_json(&json);
        assert!(parsed_files.is_empty());
    }
}
