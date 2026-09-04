use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, bail};
use console::style;
use serde::Deserialize;

#[derive(Deserialize)]
struct CargoToml {
    package: Option<CargoPackage>,
    lib: Option<CargoLib>,
}

#[derive(Deserialize)]
struct CargoPackage {
    name: Option<String>,
}

#[derive(Deserialize)]
struct CargoLib {
    name: Option<String>,
}

pub fn copy_wasm_artifact(
    plugin_dir: &Path,
    server_entry: &str,
    release: bool,
) -> anyhow::Result<()> {
    let profile = if release { "release" } else { "debug" };
    let cargo_path = plugin_dir.join("Cargo.toml");

    if !cargo_path.exists() {
        bail!(
            "Plugin manifest declares a backend, but '{}' is missing",
            cargo_path.display()
        );
    }

    let cargo_content = std::fs::read_to_string(&cargo_path)
        .with_context(|| format!("Failed to read '{}'", cargo_path.display()))?;
    let cargo_toml: CargoToml =
        toml::from_str(&cargo_content).context("Failed to parse plugin Cargo.toml")?;

    let crate_name = cargo_toml
        .lib
        .and_then(|lib| lib.name)
        .or_else(|| cargo_toml.package.and_then(|package| package.name))
        .unwrap_or_default()
        .replace('-', "_");

    if crate_name.is_empty() {
        bail!("Unable to determine plugin crate name from Cargo.toml");
    }

    let target_dir = resolve_target_directory(plugin_dir)?;
    let wasm_src = target_dir
        .join("wasm32-wasip1")
        .join(profile)
        .join(format!("{crate_name}.wasm"));
    let wasm_dest = plugin_dir.join(server_entry);

    if !wasm_src.exists() {
        bail!(
            "Expected built WASM artifact '{}' was not produced",
            wasm_src.display()
        );
    }

    let original_size = std::fs::metadata(&wasm_src)
        .with_context(|| format!("Failed to stat WASM artifact '{}'", wasm_src.display()))?
        .len();

    std::fs::copy(&wasm_src, &wasm_dest).with_context(|| {
        format!(
            "Failed to copy WASM artifact from '{}' to '{}'",
            wasm_src.display(),
            wasm_dest.display()
        )
    })?;

    let optimized = maybe_optimize_wasm_artifact(&wasm_dest, release)?;
    let final_size = std::fs::metadata(&wasm_dest)
        .with_context(|| format!("Failed to stat WASM artifact '{}'", wasm_dest.display()))?
        .len();

    println!(
        "{}  WASM artifact: {}{}",
        style("•").dim(),
        format_file_size(final_size),
        if optimized {
            format!(" (wasm-opt, was {})", format_file_size(original_size))
        } else {
            format!(" (was {})", format_file_size(original_size))
        }
    );

    Ok(())
}

fn maybe_optimize_wasm_artifact(path: &Path, release: bool) -> anyhow::Result<bool> {
    if !release {
        return Ok(false);
    }

    let tmp_path = path.with_extension("wasm.opt.tmp");
    let output = match Command::new("wasm-opt")
        .arg("-Oz")
        .arg("--strip-debug")
        .arg("--strip-producers")
        .arg(path)
        .arg("-o")
        .arg(&tmp_path)
        .output()
    {
        Ok(output) => output,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            println!(
                "{}  wasm-opt not found; copied unoptimized release WASM",
                style("!").yellow().bold()
            );
            return Ok(false);
        }
        Err(e) => return Err(e).context("Failed to run wasm-opt"),
    };

    if !output.status.success() {
        let _ = std::fs::remove_file(&tmp_path);
        bail!(
            "wasm-opt failed for '{}': {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    std::fs::rename(&tmp_path, path).with_context(|| {
        format!(
            "Failed to replace '{}' with optimized WASM '{}'",
            path.display(),
            tmp_path.display()
        )
    })?;

    Ok(true)
}

fn format_file_size(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;

    if bytes >= 1024 * 1024 {
        format!("{:.1} MiB", bytes as f64 / MIB)
    } else if bytes >= 1024 {
        format!("{:.1} KiB", bytes as f64 / KIB)
    } else {
        format!("{bytes} B")
    }
}

#[derive(Deserialize)]
struct CargoMetadata {
    target_directory: PathBuf,
}

fn resolve_target_directory(plugin_dir: &Path) -> anyhow::Result<PathBuf> {
    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(plugin_dir)
        .output()
        .context("Failed to run 'cargo metadata'")?;

    if !output.status.success() {
        bail!(
            "Failed to resolve Cargo target directory: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let metadata: CargoMetadata = serde_json::from_slice(&output.stdout)
        .context("Failed to parse 'cargo metadata' output")?;
    Ok(metadata.target_directory)
}

#[cfg(test)]
mod tests {
    use super::format_file_size;

    #[test]
    fn formats_wasm_artifact_sizes_for_logs() {
        assert_eq!(format_file_size(999), "999 B");
        assert_eq!(format_file_size(1536), "1.5 KiB");
        assert_eq!(format_file_size(2 * 1024 * 1024), "2.0 MiB");
    }
}
