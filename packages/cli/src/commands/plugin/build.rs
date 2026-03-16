use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, bail};
use clap::Args;
use console::style;
use serde::Deserialize;

use crate::dev_config;

use super::wasm::copy_wasm_artifact;

/// Builds a plugin's backend (Rust/WASM) and/or frontend components.
///
/// The frontend directory, install command, and build command can be customized via
/// `broccoli.dev.toml` in the plugin directory:
///
///   [build]
///   frontend_dir = "client"              # where to run the build command
///   frontend_install_cmd = "npm install" # default: "pnpm install --ignore-workspace"
///   frontend_build_cmd = "npm run build" # default: "pnpm build"
///
/// Without a config file, the frontend directory is auto-detected from
/// the [web].root field in plugin.toml, or by looking for package.json
/// in web/, frontend/, or the plugin root.
#[derive(Args)]
pub struct BuildPluginArgs {
    /// Path to the plugin directory (defaults to current directory)
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Force execution of the frontend installation command even if node_modules exists
    #[arg(long)]
    pub install: bool,

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
    entry: String,
}

#[derive(Deserialize)]
struct WebSection {
    root: String,
    #[allow(dead_code)]
    entry: String,
}

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
    if let Some(server) = manifest.server.as_ref() {
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

        copy_wasm_artifact(&plugin_dir, &server.entry, args.release)?;

        println!("{}  Backend build complete", style("✓").green().bold());
        built_anything = true;
    }

    // Build frontend
    if manifest.web.is_some() {
        let web_root = manifest.web.as_ref().map(|w| w.root.as_str());
        let dev = dev_config::resolve(&plugin_dir, web_root);

        let fe_dir = dev.frontend_dir.unwrap_or_else(|| plugin_dir.clone());

        if !fe_dir.exists() {
            bail!(
                "Frontend directory '{}' does not exist.\n\
                 Check build.frontend_dir in broccoli.dev.toml.",
                fe_dir.display()
            );
        }

        // Install frontend dependencies if node_modules is missing or install flag is set
        let node_modules_exists = fe_dir.join("node_modules").exists();
        if !node_modules_exists || args.install {
            let install_cmd_str = dev.frontend_install_cmd.join(" ");

            if args.install {
                println!(
                    "{}  Running '{}' in {}...",
                    style("→").blue().bold(),
                    style(&install_cmd_str).cyan(),
                    fe_dir.display()
                );
            } else {
                println!(
                    "{}  node_modules not found. Auto-running '{}'...",
                    style("!").yellow().bold(),
                    style(&install_cmd_str).cyan()
                );
            }

            let (program, cmd_args) = dev
                .frontend_install_cmd
                .split_first()
                .context("frontend_install_cmd is empty in broccoli.dev.toml")?;

            let status = Command::new(program)
                .args(cmd_args)
                .current_dir(&fe_dir)
                .status()
                .with_context(|| format!("Failed to run '{}'", install_cmd_str))?;

            if !status.success() {
                bail!("Frontend installation failed");
            }
            println!("{}  Dependencies installed", style("✓").green().bold());
        }

        println!(
            "{}  Building frontend for {}...",
            style("→").blue().bold(),
            style(plugin_name).cyan()
        );

        let (program, cmd_args) = dev
            .frontend_build_cmd
            .split_first()
            .context("frontend_build_cmd is empty in broccoli.dev.toml")?;

        let status = Command::new(program)
            .args(cmd_args)
            .current_dir(&fe_dir)
            .status()
            .with_context(|| {
                format!(
                    "Failed to run '{}'. Is it installed?",
                    dev.frontend_build_cmd.join(" ")
                )
            })?;

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
