use std::collections::HashSet;
use std::io::Write as IoWrite;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use clap::Args;
use console::style;
use notify::{Event, RecursiveMode, Watcher};
use serde::Deserialize;

use crate::auth;
use crate::dev_config::{self, FileKind, ResolvedDevConfig};

use super::wasm::copy_wasm_artifact;

/// Watches plugin source files and auto-rebuilds + uploads on changes.
///
/// For plugins with a `[web]` section, the watch command spawns the frontend
/// dev server (`pnpm dev` by default, configurable via `broccoli.dev.toml`)
/// as a long-running background process. tsup's built-in `--watch` mode
/// handles incremental frontend rebuilds; the CLI watches the `[web].root`
/// output directory for changes and triggers package + upload when new
/// build artifacts appear.
///
/// For backend changes (`.rs`, `.toml`), the CLI runs `cargo build` itself
/// and then packages + uploads.
///
/// Create a `broccoli.dev.toml` in the plugin directory to customize behavior:
///
///   [watch]
///   ignore = ["*.log", "tmp/"]         # extra patterns to ignore
///
///   [build]
///   frontend_dir = "client"            # frontend source directory
///   frontend_cmd = "npm run build"     # one-shot build (default: "pnpm build")
///   frontend_dev_cmd = "npm run dev"   # dev server (default: "pnpm dev")
///
/// Built-in ignores (always active): target/, .git/, node_modules/.
#[derive(Args)]
pub struct WatchPluginArgs {
    /// Path to the plugin directory
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Broccoli server URL (overrides saved credentials)
    #[arg(long, env = "BROCCOLI_URL")]
    pub server: Option<String>,

    /// Auth token (overrides saved credentials)
    #[arg(long, env = "BROCCOLI_TOKEN")]
    pub token: Option<String>,

    /// Build in release mode
    #[arg(long)]
    pub release: bool,

    /// Debounce interval in milliseconds
    #[arg(long, default_value = "500")]
    pub debounce: u64,
}

/// Minimal manifest struct (only fields we need for watch/build/package).
#[derive(Deserialize)]
struct WatchManifest {
    name: Option<String>,
    server: Option<ServerSection>,
    web: Option<WebSection>,
    #[serde(default)]
    translations: std::collections::HashMap<String, String>,
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

/// What triggered the change and what action to take.
enum ChangeKind {
    /// Backend source changed. Run cargo build, then package + upload.
    Backend,
    /// Frontend dist output changed (from tsup --watch). Just package + upload.
    FrontendOutput,
    /// plugin.toml changed. Rebuild backend + package + upload.
    ManifestChanged,
}

pub fn run(args: WatchPluginArgs) -> anyhow::Result<()> {
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

    let creds = auth::resolve_credentials(args.server.as_deref(), args.token.as_deref())?;

    let manifest = read_manifest(&manifest_path)?;
    let plugin_name = manifest.name.as_deref().unwrap_or("plugin");

    println!(
        "{}  Watching plugin {}...",
        style("→").blue().bold(),
        style(plugin_name).cyan()
    );
    println!("   Server: {}", style(&creds.server).dim());

    let web_root_str = manifest.web.as_ref().map(|w| w.root.as_str());
    let dev = dev_config::resolve(&plugin_dir, web_root_str);

    let web_root_abs = manifest.web.as_ref().map(|w| plugin_dir.join(&w.root));

    let mut fe_child: Option<Child> = None;
    if manifest.web.is_some() {
        match spawn_frontend_dev(&dev, &plugin_dir) {
            Ok(child) => {
                fe_child = Some(child);
                println!(
                    "{}  Frontend dev server started ({})",
                    style("✓").green().bold(),
                    style(dev.frontend_dev_cmd.join(" ")).dim()
                );
            }
            Err(e) => {
                eprintln!(
                    "{}  Failed to start frontend dev server: {}",
                    style("✗").red().bold(),
                    e
                );
                eprintln!(
                    "   Frontend changes will not be auto-rebuilt.\n\
                     Set build.frontend_dev_cmd in broccoli.dev.toml to customize."
                );
            }
        }
    }

    // Install Ctrl+C handler to kill the child process
    let child_id = fe_child.as_ref().map(|c| c.id());
    ctrlc::set_handler(move || {
        if let Some(pid) = child_id {
            // Best-effort kill. The process may have already exited
            #[cfg(unix)]
            {
                unsafe {
                    libc::kill(pid as i32, libc::SIGTERM);
                }
            }
            #[cfg(not(unix))]
            {
                let _ = pid; // suppress unused warning
            }
        }
        std::process::exit(0);
    })
    .ok(); // Ignore error if handler already set

    let (tx, rx) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |res: Result<Event, _>| {
        if let Ok(event) = res {
            let _ = tx.send(event);
        }
    })
    .context("Failed to create file watcher")?;

    watcher
        .watch(&plugin_dir, RecursiveMode::Recursive)
        .context("Failed to watch plugin directory")?;

    println!(
        "{}  Watching for changes... (Ctrl+C to stop)",
        style("✓").green().bold()
    );

    if let Err(e) =
        initial_build_and_upload(&plugin_dir, &manifest_path, &creds, &dev, args.release)
    {
        eprintln!("{}  Initial build failed: {}", style("✗").red().bold(), e);
    }

    let debounce = Duration::from_millis(args.debounce);
    let mut last_build = Instant::now();
    let mut pending_changes: HashSet<PathBuf> = HashSet::new();
    let mut manifest_changed = false;

    loop {
        match rx.recv_timeout(debounce) {
            Ok(event) => {
                for path in event.paths {
                    let relative = path.strip_prefix(&plugin_dir).unwrap_or(&path);

                    // Never ignore the web root output dir. We need to detect
                    // changes from tsup's --watch mode. Only ignore built-in
                    // dirs (target/, .git/, node_modules/) and extra patterns.
                    if dev_config::should_ignore(
                        relative,
                        &dev.extra_ignores,
                        None, // Don't ignore web root
                    ) {
                        continue;
                    }

                    if relative.file_name().is_some_and(|f| f == "plugin.toml") {
                        manifest_changed = true;
                    }

                    pending_changes.insert(path);
                }
                continue;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if pending_changes.is_empty() {
                    continue;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                break;
            }
        }

        if last_build.elapsed() < debounce {
            continue;
        }

        let change_kind = classify_changes(
            &pending_changes,
            &plugin_dir,
            web_root_abs.as_deref(),
            dev.frontend_dir.as_deref(),
        );
        pending_changes.clear();
        last_build = Instant::now();

        if manifest_changed {
            manifest_changed = false;
        }

        match change_kind {
            ChangeKind::ManifestChanged => {
                println!(
                    "\n{}  plugin.toml changed, rebuilding backend...",
                    style("→").blue().bold(),
                );

                if let Err(e) =
                    backend_build_and_upload(&plugin_dir, &manifest_path, &creds, args.release)
                {
                    eprintln!("{}  Build/upload failed: {}", style("✗").red().bold(), e);
                    eprintln!("   Waiting for next change...");
                }
            }
            ChangeKind::Backend => {
                println!(
                    "\n{}  Backend changes detected, rebuilding...",
                    style("→").blue().bold(),
                );

                if let Err(e) =
                    backend_build_and_upload(&plugin_dir, &manifest_path, &creds, args.release)
                {
                    eprintln!("{}  Build/upload failed: {}", style("✗").red().bold(), e);
                    eprintln!("   Waiting for next change...");
                }
            }
            ChangeKind::FrontendOutput => {
                println!(
                    "\n{}  Frontend output changed, uploading...",
                    style("→").blue().bold(),
                );

                if let Err(e) = package_and_upload(&plugin_dir, &manifest_path, &creds) {
                    eprintln!("{}  Upload failed: {}", style("✗").red().bold(), e);
                    eprintln!("   Waiting for next change...");
                }
            }
        }
    }

    if let Some(mut child) = fe_child {
        let _ = child.kill();
        let _ = child.wait();
    }

    Ok(())
}

fn read_manifest(path: &Path) -> anyhow::Result<WatchManifest> {
    let content = std::fs::read_to_string(path).context("Failed to read plugin.toml")?;
    toml::from_str(&content).context("Failed to parse plugin.toml")
}

/// Classify a batch of changed files into a single action to take.
fn classify_changes(
    changed: &HashSet<PathBuf>,
    plugin_dir: &Path,
    web_root_abs: Option<&Path>,
    frontend_dir: Option<&Path>,
) -> ChangeKind {
    let mut has_backend = false;
    let mut has_frontend_output = false;

    for path in changed {
        let relative = path.strip_prefix(plugin_dir).unwrap_or(path);
        let filename = relative.file_name().unwrap_or_default().to_string_lossy();

        if filename == "plugin.toml" {
            return ChangeKind::ManifestChanged;
        }

        // Check if this is inside the web root output directory
        if web_root_abs.is_some_and(|wr| path.starts_with(wr)) {
            has_frontend_output = true;
            continue;
        }

        // Check if this is a frontend source file (inside frontend_dir).
        // We ignore these because tsup --watch handles rebuilds.
        if frontend_dir.is_some_and(|fd| path.starts_with(fd)) {
            continue;
        }

        // Everything else is backend-relevant
        match dev_config::classify_file(path, plugin_dir, frontend_dir) {
            FileKind::Backend => has_backend = true,
            FileKind::PluginManifest => return ChangeKind::ManifestChanged,
            _ => {}
        }
    }

    if has_backend {
        ChangeKind::Backend
    } else if has_frontend_output {
        ChangeKind::FrontendOutput
    } else {
        // Unknown files changed. Treat as backend to be safe
        ChangeKind::Backend
    }
}

/// Spawn the frontend dev server (e.g. `pnpm dev` which runs `tsup --watch`).
fn spawn_frontend_dev(dev: &ResolvedDevConfig, _plugin_dir: &Path) -> anyhow::Result<Child> {
    let fe_dir = dev.frontend_dir.as_deref().context(
        "Cannot determine frontend directory. Set build.frontend_dir in broccoli.dev.toml",
    )?;

    if !fe_dir.exists() {
        bail!(
            "Frontend directory '{}' does not exist.\n\
             Check build.frontend_dir in broccoli.dev.toml.",
            fe_dir.display()
        );
    }

    let (program, cmd_args) = dev
        .frontend_dev_cmd
        .split_first()
        .context("frontend_dev_cmd is empty in broccoli.dev.toml")?;

    let child = Command::new(program)
        .args(cmd_args)
        .current_dir(fe_dir)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| {
            format!(
                "Failed to run '{}' in '{}'. Is it installed?",
                dev.frontend_dev_cmd.join(" "),
                fe_dir.display()
            )
        })?;

    Ok(child)
}

/// Initial build: build backend + one-shot frontend build + package + upload.
fn initial_build_and_upload(
    plugin_dir: &Path,
    manifest_path: &Path,
    creds: &auth::Credentials,
    dev: &ResolvedDevConfig,
    release: bool,
) -> anyhow::Result<()> {
    let manifest = read_manifest(manifest_path)?;

    if manifest.server.is_some() {
        build_backend(plugin_dir, release)?;

        if let Some(ref server) = manifest.server {
            copy_wasm_artifact(plugin_dir, &server.entry, release)?;
        }
    }

    if manifest.web.is_some() {
        build_frontend(dev)?;
    }

    let archive = package_plugin(plugin_dir, &manifest)?;
    upload_plugin(&archive, creds)?;

    println!("{}  Plugin uploaded to server", style("✓").green().bold());

    Ok(())
}

/// Backend change: cargo build + copy wasm + package + upload.
fn backend_build_and_upload(
    plugin_dir: &Path,
    manifest_path: &Path,
    creds: &auth::Credentials,
    release: bool,
) -> anyhow::Result<()> {
    let manifest = read_manifest(manifest_path)?;

    if manifest.server.is_some() {
        build_backend(plugin_dir, release)?;

        if let Some(ref server) = manifest.server {
            copy_wasm_artifact(plugin_dir, &server.entry, release)?;
        }
    }

    let archive = package_plugin(plugin_dir, &manifest)?;
    upload_plugin(&archive, creds)?;

    println!("{}  Plugin reloaded on server", style("✓").green().bold());

    Ok(())
}

/// Frontend output change: just package + upload (no build needed).
fn package_and_upload(
    plugin_dir: &Path,
    manifest_path: &Path,
    creds: &auth::Credentials,
) -> anyhow::Result<()> {
    let manifest = read_manifest(manifest_path)?;
    let archive = package_plugin(plugin_dir, &manifest)?;
    upload_plugin(&archive, creds)?;

    println!("{}  Plugin reloaded on server", style("✓").green().bold());

    Ok(())
}

fn build_backend(plugin_dir: &Path, release: bool) -> anyhow::Result<()> {
    println!("  {}  Building backend...", style("→").blue());

    let mut cargo_args = vec!["build", "--target", "wasm32-wasip1"];
    if release {
        cargo_args.push("--release");
    }

    let status = Command::new("cargo")
        .args(&cargo_args)
        .current_dir(plugin_dir)
        .status()
        .context("Failed to run cargo build")?;

    if !status.success() {
        bail!("Backend build failed");
    }

    println!("  {}  Backend build complete", style("✓").green());
    Ok(())
}

/// One-shot frontend build (used for initial build only).
fn build_frontend(dev: &ResolvedDevConfig) -> anyhow::Result<()> {
    println!("  {}  Building frontend...", style("→").blue());

    let fe_dir = dev.frontend_dir.as_deref().context(
        "Cannot determine frontend directory. Set build.frontend_dir in broccoli.dev.toml",
    )?;

    if !fe_dir.exists() {
        bail!(
            "Frontend directory '{}' does not exist.\n\
             Check build.frontend_dir in broccoli.dev.toml.",
            fe_dir.display()
        );
    }

    let (program, cmd_args) = dev
        .frontend_cmd
        .split_first()
        .context("frontend_cmd is empty in broccoli.dev.toml")?;

    let status = Command::new(program)
        .args(cmd_args)
        .current_dir(fe_dir)
        .status()
        .with_context(|| {
            format!(
                "Failed to run '{}'. Is it installed?",
                dev.frontend_cmd.join(" ")
            )
        })?;

    if !status.success() {
        bail!("Frontend build failed");
    }

    println!("  {}  Frontend build complete", style("✓").green());
    Ok(())
}

fn package_plugin(plugin_dir: &Path, manifest: &WatchManifest) -> anyhow::Result<Vec<u8>> {
    let plugin_id = plugin_dir
        .file_name()
        .and_then(|n| n.to_str())
        .context("Invalid plugin directory name")?;

    let mut builder = tar::Builder::new(Vec::new());

    add_file_to_tar(&mut builder, plugin_dir, "plugin.toml", plugin_id)?;

    if let Some(ref server) = manifest.server {
        add_file_to_tar(&mut builder, plugin_dir, &server.entry, plugin_id)?;
    }

    if let Some(ref web) = manifest.web {
        let web_root = plugin_dir.join(&web.root);
        if web_root.exists() {
            add_dir_to_tar(&mut builder, plugin_dir, &web.root, plugin_id)?;
        }
    }

    // Include translation files
    for path in manifest.translations.values() {
        add_file_to_tar(&mut builder, plugin_dir, path, plugin_id)?;
    }

    // Include config directory
    let config_dir = plugin_dir.join("config");
    if config_dir.exists() {
        add_dir_to_tar(&mut builder, plugin_dir, "config", plugin_id)?;
    }

    let tar_data = builder.into_inner().context("Failed to finalize tar")?;

    // Compress with gzip
    use flate2::Compression;
    use flate2::write::GzEncoder;
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(&tar_data)?;
    encoder.finish().context("Failed to compress archive")
}

fn add_file_to_tar(
    builder: &mut tar::Builder<Vec<u8>>,
    base_dir: &Path,
    relative_path: &str,
    plugin_id: &str,
) -> anyhow::Result<()> {
    let full_path = base_dir.join(relative_path);
    if !full_path.exists() {
        return Ok(()); // Skip missing files
    }
    let tar_path = format!("{}/{}", plugin_id, relative_path);
    builder
        .append_path_with_name(&full_path, &tar_path)
        .with_context(|| format!("Failed to add '{}' to archive", relative_path))?;
    Ok(())
}

fn add_dir_to_tar(
    builder: &mut tar::Builder<Vec<u8>>,
    base_dir: &Path,
    relative_dir: &str,
    plugin_id: &str,
) -> anyhow::Result<()> {
    let full_dir = base_dir.join(relative_dir);
    if !full_dir.exists() {
        return Ok(());
    }
    let tar_prefix = format!("{}/{}", plugin_id, relative_dir);
    builder
        .append_dir_all(&tar_prefix, &full_dir)
        .with_context(|| format!("Failed to add directory '{}' to archive", relative_dir))?;
    Ok(())
}

fn upload_plugin(archive: &[u8], creds: &auth::Credentials) -> anyhow::Result<()> {
    let client = reqwest::blocking::Client::new();

    let form = reqwest::blocking::multipart::Form::new().part(
        "plugin",
        reqwest::blocking::multipart::Part::bytes(archive.to_vec())
            .file_name("plugin.tar.gz")
            .mime_str("application/gzip")?,
    );

    let resp = client
        .post(format!("{}/api/v1/admin/plugins/upload", creds.server))
        .bearer_auth(&creds.token)
        .multipart(form)
        .send()
        .context("Failed to upload plugin")?;

    if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        bail!(
            "Authentication failed (401). Your token may have expired.\n\
             Run `broccoli login` again to refresh your credentials."
        );
    }

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        bail!("Upload failed ({}): {}", status, body);
    }

    Ok(())
}
