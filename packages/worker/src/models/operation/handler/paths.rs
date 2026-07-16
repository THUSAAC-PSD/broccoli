use super::super::sandbox::{DirectoryOptions, DirectoryRule};
use anyhow::{Context, Result, anyhow};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

pub(super) fn safe_join(base: &Path, relative: &str) -> Result<PathBuf> {
    let mut resolved = base.to_path_buf();
    for component in Path::new(relative).components() {
        match component {
            Component::Normal(part) => resolved.push(part),
            Component::CurDir => {}
            _ => {
                return Err(anyhow!(
                    "Unsafe path component in '{}': {:?}",
                    relative,
                    component
                ));
            }
        }
    }
    Ok(resolved)
}

/// Translate a `MountSpec::PlatformTool { name }` into a read-only directory
/// rule that makes `<tools_dir>/<name>` reachable inside the box at `inside_path`.
/// The tool name must be a single safe path component - no separators, NUL, or
/// `..` - so a malicious op cannot escape the configured tools directory.
///
/// isolate's `--dir` bind-mounts **directories**, not single files (a file source
/// fails with `ENOTDIR`: "Cannot mount ... Not a directory"). So we mount the whole
/// `tools_dir` read-only at the *parent* of `inside_path` - e.g. mount
/// `<tools_dir>` at `/tools` so the tool is reachable at `/tools/<name>`, exactly
/// where the resolver's argv invokes it. `inside_path` must therefore name a
/// directory parent, and its basename must equal `name` (so the in-box path lands
/// on the tool). `tools_dir` is platform-controlled, so exposing its contents
/// read-only to the trusted checker step is safe.
///
/// The default [`DirectoryOptions`] are intentional: `read_write = false` (the
/// tools are immutable to the sandboxed process) and `no_exec = false` (they must
/// be runnable). Mirrors the safety stance of [`validate_pipe_name`].
pub(super) fn platform_tool_directory_rule(
    inside_path: &str,
    name: &str,
    tools_dir: &Path,
) -> Result<DirectoryRule> {
    if name.is_empty()
        || name.contains('/')
        || name.contains('\\')
        || name.contains('\0')
        || name.contains("..")
    {
        return Err(anyhow!("Unsafe platform tool name: '{name}'"));
    }
    let inside = PathBuf::from(inside_path);
    if inside.file_name().and_then(|n| n.to_str()) != Some(name) {
        return Err(anyhow!(
            "platform tool inside_path '{inside_path}' basename must equal tool name '{name}'"
        ));
    }
    let inside_dir = match inside.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => {
            return Err(anyhow!(
                "platform tool inside_path '{inside_path}' must include a directory component"
            ));
        }
    };
    Ok(DirectoryRule {
        inside_path: inside_dir,
        outside_path: Some(tools_dir.to_path_buf()),
        options: DirectoryOptions::default(),
    })
}

/// Resolve a `MountSource::StepOutput { from_step, file }` to the absolute path
/// of the producer step's captured `file`. This is the intra-op handoff: a
/// dependent step consumes a prior step's output file directly from that step's
/// working dir - no blob round-trip.
///
/// Safety: `from_step` must be in the consuming step's `depends_on` (so it has
/// already run and the file exists) and `file` must resolve safely within the
/// producer's working dir (no `..` or absolute escape, via [`safe_join`]).
///
/// The resolved file is COPIED into the consumer's box by
/// [`stage_step_output_file`], NOT bind-mounted. isolate's `--dir` bind-mounts
/// **directories**, not single files - a file source fails with `ENOTDIR`
/// ("Cannot mount ... Not a directory"). Mounting the producer's *directory*
/// instead is unacceptable here: the producer (a solution exec/compile step)
/// holds the contestant's source and binary, which must never be exposed to the
/// author-controlled checker. A copy hands over exactly the one named file.
pub(super) fn resolve_step_output_src(
    from_step: &str,
    file: &str,
    step_working_dirs: &HashMap<String, PathBuf>,
    depends_on: &[String],
) -> Result<PathBuf> {
    if !depends_on.iter().any(|dep| dep == from_step) {
        return Err(anyhow!(
            "step mounts output of '{from_step}' but does not declare it in depends_on"
        ));
    }
    let from_dir = step_working_dirs
        .get(from_step)
        .ok_or_else(|| anyhow!("step mounts output of unknown step '{from_step}'"))?;
    safe_join(from_dir, file)
}

/// Copy a resolved [`resolve_step_output_src`] file into the consuming step's box
/// working dir at `inside_path` (a box-relative path; `safe_join` keeps it inside
/// the box). See [`resolve_step_output_src`] for why the handoff is a copy rather
/// than an isolate bind mount. The copy is read by the consumer from its own cwd.
pub(super) async fn stage_step_output_file(
    src: &Path,
    inside_path: &str,
    consumer_working_dir: &Path,
) -> Result<()> {
    let dest = safe_join(consumer_working_dir, inside_path)?;
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("creating parent dir for staged output '{inside_path}'"))?;
    }
    tokio::fs::copy(src, &dest)
        .await
        .with_context(|| format!("staging step output '{}' -> '{inside_path}'", src.display()))?;
    Ok(())
}

pub(super) fn validate_pipe_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(anyhow!("Pipe/channel name cannot be empty"));
    }
    if name.contains('/') || name.contains('\\') || name.contains('\0') || name.contains("..") {
        return Err(anyhow!(
            "Pipe/channel name contains unsafe characters: '{name}'"
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(anyhow!(
            "Pipe/channel name must be alphanumeric, underscore, or hyphen: '{name}'"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod mount_tests {
    use super::*;

    #[test]
    fn platform_tool_mount_is_read_only_executable_under_tools_dir() {
        let tools = Path::new("/opt/broccoli/tools");
        let rule =
            platform_tool_directory_rule("/tools/broccoli-compare", "broccoli-compare", tools)
                .unwrap();

        // isolate binds directories, not files: mount the tools dir itself at the
        // parent of inside_path, so `/tools/broccoli-compare` resolves in-box.
        assert_eq!(rule.inside_path, PathBuf::from("/tools"));
        assert_eq!(
            rule.outside_path,
            Some(PathBuf::from("/opt/broccoli/tools")),
            "must bind the tools directory, not the single tool file"
        );
        // A mounted tool must be runnable but not writable.
        assert!(!rule.options.read_write, "tool mount must be read-only");
        assert!(!rule.options.no_exec, "tool mount must be executable");
    }

    #[test]
    fn platform_tool_inside_path_basename_must_match_name() {
        let tools = Path::new("/opt/broccoli/tools");
        // basename ("cmp") != tool name ("broccoli-compare") => the in-box path
        // would not land on the tool, so the rule must be rejected.
        assert!(
            platform_tool_directory_rule("/tools/cmp", "broccoli-compare", tools).is_err(),
            "mismatched basename must be rejected"
        );
    }

    #[test]
    fn platform_tool_inside_path_requires_directory_component() {
        let tools = Path::new("/opt/broccoli/tools");
        // A bare filename has no parent dir to mount the tools dir onto.
        assert!(
            platform_tool_directory_rule("broccoli-compare", "broccoli-compare", tools).is_err(),
            "bare inside_path (no directory component) must be rejected"
        );
    }

    #[test]
    fn platform_tool_name_rejects_path_traversal() {
        let tools = Path::new("/opt/broccoli/tools");
        for bad in ["../etc/passwd", "a/b", "..", "", "a\\b", "x\0y"] {
            assert!(
                platform_tool_directory_rule("/tools/x", bad, tools).is_err(),
                "name {bad:?} must be rejected"
            );
        }
    }

    fn dirs_with(step: &str, dir: &str) -> HashMap<String, PathBuf> {
        let mut m = HashMap::new();
        m.insert(step.to_string(), PathBuf::from(dir));
        m
    }

    #[test]
    fn step_output_src_resolves_under_from_step_dir() {
        let dirs = dirs_with("producer", "/work/a");
        let deps = ["producer".to_string()];
        let src = resolve_step_output_src("producer", "out.txt", &dirs, &deps).unwrap();
        assert_eq!(src, PathBuf::from("/work/a/out.txt"));
    }

    #[test]
    fn step_output_src_requires_declared_dependency() {
        let dirs = dirs_with("producer", "/work/a");
        // from_step not in depends_on -> rejected (ordering + visibility unsafe).
        assert!(resolve_step_output_src("producer", "out.txt", &dirs, &[]).is_err());
    }

    #[test]
    fn step_output_src_rejects_file_path_traversal() {
        let dirs = dirs_with("producer", "/work/a");
        let deps = ["producer".to_string()];
        for bad in ["../escape", "/abs", "a/../../b"] {
            assert!(
                resolve_step_output_src("producer", bad, &dirs, &deps).is_err(),
                "file {bad:?} must be rejected"
            );
        }
    }

    #[test]
    fn step_output_src_unknown_from_step_errors() {
        let dirs: HashMap<String, PathBuf> = HashMap::new();
        let deps = ["ghost".to_string()];
        assert!(resolve_step_output_src("ghost", "out.txt", &dirs, &deps).is_err());
    }

    #[test]
    fn stage_step_output_file_copies_only_the_named_file() {
        // The handoff copies exactly the one named file into the consumer box -
        // never the producer's other files (which hold the contestant's source).
        // This is what makes testlib (File-output) checkers work at all: isolate
        // cannot bind-mount a single file (ENOTDIR), so we copy instead.
        let producer = tempfile::tempdir().unwrap();
        let consumer = tempfile::tempdir().unwrap();
        std::fs::write(producer.path().join("out.txt"), b"contestant output").unwrap();
        std::fs::write(producer.path().join("solution.cpp"), b"secret source").unwrap();

        let dirs = dirs_with("producer", producer.path().to_str().unwrap());
        let deps = ["producer".to_string()];
        let src = resolve_step_output_src("producer", "out.txt", &dirs, &deps).unwrap();

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(stage_step_output_file(&src, "output.txt", consumer.path()))
            .unwrap();

        assert_eq!(
            std::fs::read(consumer.path().join("output.txt")).unwrap(),
            b"contestant output"
        );
        assert!(
            !consumer.path().join("solution.cpp").exists(),
            "only the single named file must be exposed to the checker"
        );
    }
}
