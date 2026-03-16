use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Configuration for a single programming language.
///
/// Commands and filenames support three template variables:
///
/// - `{basename}`: stem of the submitted filename, or [`Self::basename_fallback`]
/// - `{source}`: resolved `source_filename` (after `{basename}` expansion)
/// - `{binary}`: resolved `binary_name` (after `{basename}` expansion)
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LanguageDefinition {
    /// Compile command. May contain template variables.
    ///
    /// `None` = interpreted language (no compile step).
    pub compile_cmd: Option<Vec<String>>,

    /// Run command. May contain template variables.
    #[serde(default)]
    pub run_cmd: Vec<String>,

    /// Source filename placed in the sandbox. May contain `{basename}`.
    #[serde(default)]
    pub source_filename: String,

    /// Compiled output filename. May contain `{basename}`.
    ///
    /// For interpreted languages set this equal to `source_filename`.
    #[serde(default)]
    pub binary_name: String,

    /// Command used to probe the toolchain version at worker startup.
    ///
    /// `None` = skip fingerprinting for this language.
    pub version_cmd: Option<Vec<String>>,

    /// Fallback value for `{basename}` when no submitted filename is provided. Defaults to `"solution"`.
    #[serde(default = "default_basename_fallback")]
    pub basename_fallback: String,
}

fn default_basename_fallback() -> String {
    "solution".to_string()
}

/// Fully resolved language config for one submission.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ResolvedLanguage {
    /// `None` = interpreted (no compile step).
    pub compile_cmd: Option<Vec<String>>,
    pub run_cmd: Vec<String>,
    pub source_filename: String,
    pub binary_name: String,
}

/// Resolve a `LanguageDefinition` to concrete commands and filenames for one submission.
///
/// Returns `Err` if the language ID is unknown or if any required field
/// is empty after expansion.
pub fn resolve_language(
    lang_id: &str,
    submitted_filename: &str,
    definitions: &HashMap<String, LanguageDefinition>,
) -> Result<ResolvedLanguage, String> {
    let def = definitions
        .get(lang_id)
        .ok_or_else(|| format!("Unsupported language: {}", lang_id))?;

    let basename = Path::new(submitted_filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(def.basename_fallback.as_str());

    let source_filename = def.source_filename.replace("{basename}", basename);
    let binary_name = def.binary_name.replace("{basename}", basename);

    if source_filename.is_empty() {
        return Err(format!(
            "Language '{}' has no source_filename. Set source_filename in [languages.{}]",
            lang_id, lang_id
        ));
    }
    if binary_name.is_empty() {
        return Err(format!(
            "Language '{}' has no binary_name. Set binary_name in [languages.{}]",
            lang_id, lang_id
        ));
    }
    if def.run_cmd.is_empty() {
        return Err(format!(
            "Language '{}' has no run_cmd. Set run_cmd in [languages.{}]",
            lang_id, lang_id
        ));
    }

    let expand = |arg: &str| -> String {
        arg.replace("{basename}", basename)
            .replace("{source}", &source_filename)
            .replace("{binary}", &binary_name)
    };
    let expand_cmd = |cmd: &[String]| cmd.iter().map(|a| expand(a)).collect::<Vec<_>>();

    Ok(ResolvedLanguage {
        compile_cmd: def.compile_cmd.as_deref().map(expand_cmd),
        run_cmd: expand_cmd(&def.run_cmd),
        source_filename,
        binary_name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> LanguageDefinition {
        LanguageDefinition {
            compile_cmd: Some(vec![
                "cc".into(),
                "{source}".into(),
                "-o".into(),
                "{binary}".into(),
            ]),
            run_cmd: vec!["./{binary}".into()],
            source_filename: "sol.c".into(),
            binary_name: "sol".into(),
            version_cmd: None,
            basename_fallback: "solution".into(),
        }
    }

    fn one(lang_id: &str, def: LanguageDefinition) -> HashMap<String, LanguageDefinition> {
        [(lang_id.to_string(), def)].into()
    }

    fn resolve(lang_id: &str, submitted: &str, def: LanguageDefinition) -> ResolvedLanguage {
        resolve_language(lang_id, submitted, &one(lang_id, def)).unwrap()
    }

    #[test]
    fn source_template_in_compile_cmd_expands_to_source_filename() {
        let r = resolve("x", "", base());
        assert_eq!(r.compile_cmd.unwrap()[1], "sol.c");
    }

    #[test]
    fn binary_template_in_compile_cmd_expands_to_binary_name() {
        let r = resolve("x", "", base());
        assert_eq!(r.compile_cmd.unwrap()[3], "sol");
    }

    #[test]
    fn binary_template_in_run_cmd_expands_to_binary_name() {
        let r = resolve("x", "", base());
        assert_eq!(r.run_cmd, vec!["./sol"]);
    }

    #[test]
    fn basename_in_source_filename_expands_to_submitted_file_stem() {
        let def = LanguageDefinition {
            source_filename: "{basename}.java".into(),
            ..base()
        };
        let r = resolve("x", "MyClass.java", def);
        assert_eq!(r.source_filename, "MyClass.java");
    }

    #[test]
    fn basename_in_binary_name_expands_to_submitted_file_stem() {
        let def = LanguageDefinition {
            binary_name: "{basename}.class".into(),
            ..base()
        };
        let r = resolve("x", "MyClass.java", def);
        assert_eq!(r.binary_name, "MyClass.class");
    }

    #[test]
    fn basename_in_run_cmd_expands_to_submitted_file_stem() {
        let def = LanguageDefinition {
            run_cmd: vec!["{basename}".into()],
            ..base()
        };
        let r = resolve("x", "MyClass.java", def);
        assert_eq!(r.run_cmd, vec!["MyClass"]);
    }

    #[test]
    fn source_in_command_reflects_basename_expanded_source_filename() {
        let def = LanguageDefinition {
            compile_cmd: Some(vec!["javac".into(), "{source}".into()]),
            source_filename: "{basename}.java".into(),
            ..base()
        };
        let r = resolve("x", "Solver.java", def);
        assert_eq!(r.compile_cmd.unwrap()[1], "Solver.java");
    }

    #[test]
    fn empty_submitted_filename_uses_basename_fallback() {
        let def = LanguageDefinition {
            source_filename: "{basename}.c".into(),
            basename_fallback: "solution".into(),
            ..base()
        };
        let r = resolve("x", "", def);
        assert_eq!(r.source_filename, "solution.c");
    }

    #[test]
    fn basename_fallback_is_per_language() {
        let def = LanguageDefinition {
            source_filename: "{basename}.java".into(),
            binary_name: "{basename}.class".into(),
            run_cmd: vec!["java".into(), "{basename}".into()],
            basename_fallback: "Main".into(),
            ..base()
        };
        let r = resolve("java", "", def);
        assert_eq!(r.source_filename, "Main.java");
        assert_eq!(r.binary_name, "Main.class");
        assert_eq!(r.run_cmd[1], "Main");
    }

    #[test]
    fn interpreted_language_has_no_compile_cmd() {
        let def = LanguageDefinition {
            compile_cmd: None,
            ..base()
        };
        let r = resolve("x", "", def);
        assert!(r.compile_cmd.is_none());
    }

    #[test]
    fn unknown_language_id_returns_err() {
        let err = resolve_language("brainfuck", "", &HashMap::new()).unwrap_err();
        assert!(err.contains("brainfuck"), "error = {err}");
    }

    #[test]
    fn empty_source_filename_returns_err() {
        let def = LanguageDefinition {
            source_filename: String::new(),
            ..base()
        };
        let err = resolve_language("x", "", &one("x", def)).unwrap_err();
        assert!(err.contains("source_filename"), "error = {err}");
    }

    #[test]
    fn empty_binary_name_returns_err() {
        let def = LanguageDefinition {
            binary_name: String::new(),
            ..base()
        };
        let err = resolve_language("x", "", &one("x", def)).unwrap_err();
        assert!(err.contains("binary_name"), "error = {err}");
    }

    #[test]
    fn empty_run_cmd_returns_err() {
        let def = LanguageDefinition {
            run_cmd: vec![],
            ..base()
        };
        let err = resolve_language("x", "", &one("x", def)).unwrap_err();
        assert!(err.contains("run_cmd"), "error = {err}");
    }
}
