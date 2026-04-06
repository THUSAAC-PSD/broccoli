use broccoli_server_sdk::types::{
    CompileSpec, OutputSpec, ResolveLanguageInput, ResolveLanguageOutput, RunSpec,
};
use std::path::Path;

use crate::EntryPointConfig;

pub struct LanguageMeta {
    pub id: &'static str,
    pub display_name: &'static str,
    pub default_filename: &'static str,
    pub extensions: &'static [&'static str],
    pub template: &'static str,
}

pub const LANGUAGES: &[LanguageMeta] = &[
    LanguageMeta {
        id: "c",
        display_name: "C",
        default_filename: "solution.c",
        extensions: &["c"],
        template: "#include <stdio.h>\n\nint main() {\n    // Your code here\n    return 0;\n}\n",
    },
    LanguageMeta {
        id: "cpp",
        display_name: "C++",
        default_filename: "solution.cpp",
        extensions: &["cpp", "cc", "cxx", "c++"],
        template: "#include <iostream>\nusing namespace std;\n\nint main() {\n    // Your code here\n    return 0;\n}\n",
    },
    LanguageMeta {
        id: "python3",
        display_name: "Python 3",
        default_filename: "solution.py",
        extensions: &["py"],
        template: "# Your code here\n",
    },
    LanguageMeta {
        id: "java",
        display_name: "Java",
        default_filename: "Main.java",
        extensions: &["java"],
        template: "public class Main {\n    public static void main(String[] args) {\n        // Your code here\n    }\n}\n",
    },
];

pub const LANGUAGE_IDS: &[&str] = &["c", "cpp", "python3", "java"];

fn default_source(lang: &str) -> &str {
    match lang {
        "c" => "solution.c",
        "cpp" => "solution.cpp",
        "python3" => "solution.py",
        "java" => "Main.java",
        _ => "",
    }
}

fn default_basename(lang: &str) -> &str {
    match lang {
        "java" => "Main",
        _ => "solution",
    }
}

/// Resolve the primary source file and its basename from submitted files.
///
/// Priority: entry_point config > default source filename match > first file.
fn resolve_primary<'a>(
    lang: &str,
    all_files: &[&'a str],
    entry_point: Option<&'a str>,
) -> (&'a str, String) {
    let primary = if let Some(ep) = entry_point {
        all_files.iter().find(|f| **f == ep).copied().unwrap_or(ep)
    } else {
        let default = default_source(lang);
        all_files
            .iter()
            .find(|f| **f == default)
            .or(all_files.first())
            .copied()
            .unwrap_or_default()
    };

    let basename = Path::new(primary)
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(default_basename(lang))
        .to_string();

    (primary, basename)
}

fn collect_files<'a>(req: &'a ResolveLanguageInput) -> Vec<&'a str> {
    req.submitted_files
        .iter()
        .map(|s| s.as_str())
        .chain(req.additional_files.iter().map(|f| f.filename.as_str()))
        .collect()
}

/// Shared resolver for compiled languages that produce a single binary (C, C++).
///
/// Command: `[compiler, flags..., extra_flags..., primary, extras..., -o, basename, suffix_args...]`
fn resolve_compiled(
    lang: &str,
    req: &ResolveLanguageInput,
    entry_point_config: Option<&EntryPointConfig>,
    compiler: &str,
    flags: &[String],
    extra_compile_flags: &[String],
    suffix_args: &[&str],
) -> ResolveLanguageOutput {
    let all_files = collect_files(req);
    let ep = entry_point_config.and_then(|c| c.entry_point.as_deref());
    let (primary, basename) = resolve_primary(lang, &all_files, ep);

    let mut command = vec![compiler.to_string()];
    command.extend(flags.iter().cloned());
    command.extend(extra_compile_flags.iter().cloned());
    command.push(primary.to_string());
    for f in &all_files {
        if *f != primary {
            command.push(f.to_string());
        }
    }
    command.push("-o".into());
    command.push(basename.clone());
    command.extend(suffix_args.iter().map(|s| s.to_string()));

    let cache_inputs: Vec<String> = all_files.iter().map(|s| s.to_string()).collect();

    ResolveLanguageOutput {
        compile: Some(CompileSpec {
            command,
            cache_inputs,
            outputs: vec![OutputSpec::File(basename.clone())],
            resource_limits: None,
        }),
        run: RunSpec {
            command: vec![format!("./{basename}")],
            extra_files: vec![],
        },
    }
}

pub fn resolve_c(
    req: &ResolveLanguageInput,
    entry_point_config: Option<&EntryPointConfig>,
    compiler: &str,
    flags: &[String],
    extra_compile_flags: &[String],
) -> ResolveLanguageOutput {
    resolve_compiled(
        "c",
        req,
        entry_point_config,
        compiler,
        flags,
        extra_compile_flags,
        &["-lm"],
    )
}

pub fn resolve_cpp(
    req: &ResolveLanguageInput,
    entry_point_config: Option<&EntryPointConfig>,
    compiler: &str,
    flags: &[String],
    extra_compile_flags: &[String],
) -> ResolveLanguageOutput {
    resolve_compiled(
        "cpp",
        req,
        entry_point_config,
        compiler,
        flags,
        extra_compile_flags,
        &[],
    )
}

pub fn resolve_python3(
    req: &ResolveLanguageInput,
    entry_point_config: Option<&EntryPointConfig>,
    interpreter: &str,
) -> ResolveLanguageOutput {
    let all_files = collect_files(req);
    let ep = entry_point_config.and_then(|c| c.entry_point.as_deref());
    let (primary, _) = resolve_primary("python3", &all_files, ep);

    ResolveLanguageOutput {
        compile: None,
        run: RunSpec {
            command: vec![interpreter.to_string(), primary.to_string()],
            extra_files: all_files.iter().map(|s| s.to_string()).collect(),
        },
    }
}

pub fn resolve_java(
    req: &ResolveLanguageInput,
    entry_point_config: Option<&EntryPointConfig>,
    compiler: &str,
    runner: &str,
    flags: &[String],
    extra_compile_flags: &[String],
) -> ResolveLanguageOutput {
    let all_files = collect_files(req);
    let ep = entry_point_config.and_then(|c| c.entry_point.as_deref());
    let (primary, basename) = resolve_primary("java", &all_files, ep);

    let mut command = vec![compiler.to_string()];
    command.extend(flags.iter().cloned());
    command.extend(extra_compile_flags.iter().cloned());
    command.push(primary.to_string());
    for f in &all_files {
        if *f != primary {
            command.push(f.to_string());
        }
    }

    let cache_inputs: Vec<String> = all_files.iter().map(|s| s.to_string()).collect();

    ResolveLanguageOutput {
        compile: Some(CompileSpec {
            command,
            cache_inputs,
            // javac may produce multiple .class files (inner classes)
            outputs: vec![OutputSpec::Glob("*.class".into())],
            resource_limits: None,
        }),
        run: RunSpec {
            command: vec![runner.to_string(), "-cp".into(), ".".into(), basename],
            extra_files: vec![],
        },
    }
}
