use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, bail};
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

    std::fs::copy(&wasm_src, &wasm_dest).with_context(|| {
        format!(
            "Failed to copy WASM artifact from '{}' to '{}'",
            wasm_src.display(),
            wasm_dest.display()
        )
    })?;

    Ok(())
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
