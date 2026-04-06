use broccoli_server_sdk::types::{FileRef, OutputSpec, ResolveLanguageInput};

use crate::EntryPointConfig;
use crate::resolve;

fn req(lang: &str, files: Vec<&str>) -> ResolveLanguageInput {
    ResolveLanguageInput {
        language_id: lang.into(),
        submitted_files: files.iter().map(|s| s.to_string()).collect(),
        additional_files: vec![],
        problem_id: None,
        contest_id: None,
        overrides: None,
    }
}

fn default_cpp_flags() -> Vec<String> {
    vec!["-O2".into(), "-std=c++17".into()]
}

fn default_c_flags() -> Vec<String> {
    vec!["-O2".into(), "-std=c17".into()]
}

// C++
#[test]
fn cpp_single_file() {
    let result = resolve::resolve_cpp(
        &req("cpp", vec!["solution.cpp"]),
        None,
        "/usr/bin/g++",
        &default_cpp_flags(),
        &[],
    );
    let compile = result.compile.unwrap();
    assert_eq!(compile.command[0], "/usr/bin/g++");
    assert!(compile.command.contains(&"solution.cpp".to_string()));
    assert!(compile.command.contains(&"-o".to_string()));
    assert_eq!(result.run.command, vec!["./solution"]);
    assert!(result.run.extra_files.is_empty());
}

#[test]
fn cpp_with_grader_stubs() {
    let result = resolve::resolve_cpp(
        &req("cpp", vec!["solution.cpp", "grader.cpp", "grader.h"]),
        None,
        "/usr/bin/g++",
        &default_cpp_flags(),
        &[],
    );
    let compile = result.compile.unwrap();
    assert!(compile.command.contains(&"solution.cpp".to_string()));
    assert!(compile.command.contains(&"grader.cpp".to_string()));
    assert!(compile.command.contains(&"grader.h".to_string()));
    assert_eq!(
        compile.cache_inputs,
        vec!["solution.cpp", "grader.cpp", "grader.h"]
    );
}

#[test]
fn cpp_primary_matched_by_default_source_filename() {
    let result = resolve::resolve_cpp(
        &req("cpp", vec!["grader.cpp", "solution.cpp"]),
        None,
        "/usr/bin/g++",
        &default_cpp_flags(),
        &[],
    );
    assert_eq!(result.run.command, vec!["./solution"]);
}

#[test]
fn cpp_compiler_override() {
    let result = resolve::resolve_cpp(
        &req("cpp", vec!["solution.cpp"]),
        None,
        "/usr/local/bin/g++-13",
        &default_cpp_flags(),
        &[],
    );
    assert_eq!(result.compile.unwrap().command[0], "/usr/local/bin/g++-13");
}

#[test]
fn cpp_flag_override_replaces_defaults() {
    let custom_flags = vec!["-O3".into(), "-std=c++20".into()];
    let result = resolve::resolve_cpp(
        &req("cpp", vec!["solution.cpp"]),
        None,
        "/usr/bin/g++",
        &custom_flags,
        &[],
    );
    let cmd = result.compile.unwrap().command;
    assert!(cmd.contains(&"-O3".to_string()));
    assert!(cmd.contains(&"-std=c++20".to_string()));
    assert!(!cmd.contains(&"-O2".to_string()));
    // Structural args preserved
    assert!(cmd.contains(&"-o".to_string()));
    assert!(cmd.contains(&"solution".to_string()));
}

#[test]
fn cpp_extra_compile_flags() {
    let result = resolve::resolve_cpp(
        &req("cpp", vec!["solution.cpp"]),
        None,
        "/usr/bin/g++",
        &default_cpp_flags(),
        &["-DONLINE_JUDGE".into()],
    );
    let cmd = result.compile.unwrap().command;
    assert!(cmd.contains(&"-DONLINE_JUDGE".to_string()));
    assert!(cmd.contains(&"-O2".to_string()));
}

// C
#[test]
fn c_compiles_with_gcc() {
    let result = resolve::resolve_c(
        &req("c", vec!["solution.c"]),
        None,
        "/usr/bin/gcc",
        &default_c_flags(),
        &[],
    );
    let compile = result.compile.unwrap();
    assert_eq!(compile.command[0], "/usr/bin/gcc");
    assert!(compile.command.contains(&"-std=c17".to_string()));
    // -lm should come after source files and -o binary
    let lm_pos = compile.command.iter().position(|a| a == "-lm").unwrap();
    let source_pos = compile
        .command
        .iter()
        .position(|a| a == "solution.c")
        .unwrap();
    assert!(lm_pos > source_pos, "-lm should be after source files");
}

// Python 3
#[test]
fn python_interpreted() {
    let result = resolve::resolve_python3(
        &req("python3", vec!["solution.py"]),
        None,
        "/usr/bin/python3",
    );
    assert!(result.compile.is_none());
    assert_eq!(result.run.command, vec!["/usr/bin/python3", "solution.py"]);
    assert_eq!(result.run.extra_files, vec!["solution.py"]);
}

#[test]
fn python_with_grader() {
    let result = resolve::resolve_python3(
        &req("python3", vec!["solution.py", "grader.py"]),
        None,
        "/usr/bin/python3",
    );
    assert_eq!(result.run.command, vec!["/usr/bin/python3", "solution.py"]);
    assert_eq!(result.run.extra_files, vec!["solution.py", "grader.py"]);
}

// Java
#[test]
fn java_uses_glob_output() {
    let result = resolve::resolve_java(
        &req("java", vec!["Solver.java"]),
        None,
        "javac",
        "java",
        &[],
        &[],
    );
    let compile = result.compile.unwrap();
    assert_eq!(compile.command, vec!["javac", "Solver.java"]);
    assert_eq!(compile.outputs, vec![OutputSpec::Glob("*.class".into())]);
    assert_eq!(result.run.command, vec!["java", "-cp", ".", "Solver"]);
}

#[test]
fn all_standard_languages_defined() {
    assert!(resolve::LANGUAGE_IDS.contains(&"c"));
    assert!(resolve::LANGUAGE_IDS.contains(&"cpp"));
    assert!(resolve::LANGUAGE_IDS.contains(&"python3"));
    assert!(resolve::LANGUAGE_IDS.contains(&"java"));
}

#[test]
fn entry_point_override_changes_primary() {
    let result = resolve::resolve_python3(
        &req("python3", vec!["solution.py", "grader.py"]),
        Some(&EntryPointConfig {
            entry_point: Some("grader.py".into()),
            extra_compile_flags: None,
        }),
        "/usr/bin/python3",
    );
    assert_eq!(result.run.command, vec!["/usr/bin/python3", "grader.py"]);
    assert!(result.run.extra_files.contains(&"solution.py".to_string()));
    assert!(result.run.extra_files.contains(&"grader.py".to_string()));
}

#[test]
fn entry_point_override_cpp() {
    let result = resolve::resolve_cpp(
        &req("cpp", vec!["solution.cpp", "grader.cpp"]),
        Some(&EntryPointConfig {
            entry_point: Some("grader.cpp".into()),
            extra_compile_flags: None,
        }),
        "/usr/bin/g++",
        &default_cpp_flags(),
        &[],
    );
    assert_eq!(result.run.command, vec!["./grader"]);
    let compile = result.compile.unwrap();
    // Primary source comes first, extras follow
    assert!(compile.command.contains(&"grader.cpp".to_string()));
    assert!(compile.command.contains(&"solution.cpp".to_string()));
}

#[test]
fn cpp_with_additional_file_refs() {
    let mut input = req("cpp", vec!["solution.cpp"]);
    input.additional_files = vec![
        FileRef {
            filename: "grader.cpp".into(),
            content_type: Some("text/x-c++src".into()),
        },
        FileRef {
            filename: "grader.h".into(),
            content_type: Some("text/x-c".into()),
        },
    ];
    let result = resolve::resolve_cpp(&input, None, "/usr/bin/g++", &default_cpp_flags(), &[]);
    let compile = result.compile.unwrap();
    assert!(compile.command.contains(&"grader.cpp".to_string()));
    assert!(compile.command.contains(&"grader.h".to_string()));
    assert_eq!(
        compile.cache_inputs,
        vec!["solution.cpp", "grader.cpp", "grader.h"]
    );
}

#[test]
fn compile_spec_has_no_resource_limits_by_default() {
    let result = resolve::resolve_cpp(
        &req("cpp", vec!["solution.cpp"]),
        None,
        "/usr/bin/g++",
        &default_cpp_flags(),
        &[],
    );
    assert!(result.compile.unwrap().resource_limits.is_none());
}
