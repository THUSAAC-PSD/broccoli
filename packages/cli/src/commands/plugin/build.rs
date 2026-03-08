use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, bail};
use clap::Args;
use console::style;
use serde::Deserialize;

#[derive(Args)]
pub struct BuildPluginArgs {
    /// Path to the plugin directory (defaults to current directory)
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Build in release mode (optimized)
    #[arg(long)]
    pub release: bool,
}

/// Minimal manifest struct — avoids pulling in plugin-core's transitive deps.
#[derive(Deserialize)]
struct MinimalManifest {
    name: Option<String>,
    server: Option<ServerSection>,
    web: Option<WebSection>,
}

#[derive(Deserialize)]
struct ServerSection {
    #[allow(dead_code)]
    entry: String,
}

/// Marker struct — we only need to know the `[web]` section exists.
#[derive(Deserialize)]
struct WebSection {}

pub fn run(args: BuildPluginArgs) -> anyhow::Result<()> {
    let plugin_dir = args
        .path
        .canonicalize()
        .with_context(|| format!("Cannot find directory '{}'", args.path.display()))?;

    let manifest_path = plugin_dir.join("plugin.toml");
    if !manifest_path.exists() {
        bail!(
            "Not a broccoli plugin directory: no plugin.toml found in '{}'.\n\
             Run `broccoli plugin new` to create a new plugin.",
            plugin_dir.display()
        );
    }

    let manifest_content =
        std::fs::read_to_string(&manifest_path).context("Failed to read plugin.toml")?;
    let manifest: MinimalManifest =
        toml::from_str(&manifest_content).context("Failed to parse plugin.toml")?;

    let plugin_name = manifest.name.as_deref().unwrap_or("plugin");
    let mut built_anything = false;

    // Build backend (Rust/WASM)
    if manifest.server.is_some() {
        println!(
            "{}  Building backend for {}...",
            style("→").blue().bold(),
            style(plugin_name).cyan()
        );

        let mut cargo_args = vec!["build", "--target", "wasm32-wasip1"];
        if args.release {
            cargo_args.push("--release");
        }

        let status = Command::new("cargo")
            .args(&cargo_args)
            .current_dir(&plugin_dir)
            .status()
            .context("Failed to run cargo build. Is Rust installed?")?;

        if !status.success() {
            bail!("Backend build failed");
        }

        println!("{}  Backend build complete", style("✓").green().bold());
        built_anything = true;
    }

    // Build frontend
    if manifest.web.is_some() {
        // Find the frontend source directory by looking for package.json.
        // For full plugins it's in web/, for frontend-only it's at the root.
        let fe_dir = if plugin_dir.join("web/package.json").exists() {
            plugin_dir.join("web")
        } else {
            plugin_dir.clone()
        };

        println!(
            "{}  Building frontend for {}...",
            style("→").blue().bold(),
            style(plugin_name).cyan()
        );

        let status = Command::new("pnpm")
            .args(["build"])
            .current_dir(&fe_dir)
            .status()
            .context(
                "Failed to run pnpm build. Is pnpm installed?\n\
                 Install via: npm install -g pnpm   (https://pnpm.io/installation)",
            )?;

        if !status.success() {
            bail!("Frontend build failed");
        }

        println!("{}  Frontend build complete", style("✓").green().bold());
        built_anything = true;
    }

    if !built_anything {
        println!(
            "{}  plugin.toml has no [server] or [web] section — nothing to build.",
            style("!").yellow().bold()
        );
    }

    Ok(())
}
