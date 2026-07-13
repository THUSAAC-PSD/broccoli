//! Printing backends. A folder sink, a command template, or the OS default.
//! The OS default is CUPS `lp` on macOS/Linux and a bundled SumatraPDF on
//! Windows, which prints silently with no dialog and no separate install.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};

use crate::config::PrinterCfg;

const FOLDER_PREFIX: &str = "folder:";

/// Returns the target dir when this printer is a folder sink.
fn sink_dir(printer: &PrinterCfg) -> Option<String> {
    for v in [printer.command.as_deref(), printer.os_id.as_deref()]
        .into_iter()
        .flatten()
    {
        if let Some(dir) = v.strip_prefix(FOLDER_PREFIX) {
            return Some(dir.trim().to_string());
        }
    }
    None
}

pub fn print_pdf(printer: &PrinterCfg, pdf_path: &Path) -> Result<()> {
    if let Some(dir) = sink_dir(printer) {
        return write_to_folder(&dir, pdf_path);
    }
    if let Some(template) = printer
        .command
        .as_deref()
        .filter(|c| !c.starts_with(FOLDER_PREFIX))
    {
        return run_template(template, printer, pdf_path);
    }
    default_print(printer, pdf_path)
}

fn write_to_folder(dir: &str, pdf_path: &Path) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating sink dir {dir}"))?;
    let name = pdf_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "print.pdf".to_string());
    let dest = Path::new(dir).join(name);
    std::fs::copy(pdf_path, &dest).with_context(|| format!("copying to {}", dest.display()))?;
    println!("    → wrote {}", dest.display());
    Ok(())
}

/// Substitute per-token so paths with spaces survive.
fn run_template(template: &str, printer: &PrinterCfg, pdf_path: &Path) -> Result<()> {
    let printer_id = printer
        .os_id
        .clone()
        .unwrap_or_else(|| printer.name.clone());
    let file = pdf_path.to_string_lossy().to_string();
    let mut tokens = template.split_whitespace();
    let program = tokens
        .next()
        .ok_or_else(|| anyhow!("empty print command for printer '{}'", printer.name))?;
    let args: Vec<String> = tokens
        .map(|tok| {
            tok.replace("{printer}", &printer_id)
                .replace("{file}", &file)
        })
        .collect();
    run(program, &args)
}

/// CUPS prints silently to any installed printer, including USB and driverless
/// queues, so macOS/Linux need nothing more.
#[cfg(not(windows))]
fn default_print(printer: &PrinterCfg, pdf_path: &Path) -> Result<()> {
    let mut args = Vec::new();
    if let Some(p) = printer.os_id.as_deref().filter(|s| !s.is_empty()) {
        args.push("-d".to_string());
        args.push(p.to_string());
    }
    args.push(pdf_path.to_string_lossy().to_string());
    run("lp", &args)
}

/// Windows has no built-in silent PDF print, so print through the bundled
/// SumatraPDF. It prints and exits, so the exit status is a real result.
#[cfg(windows)]
fn default_print(printer: &PrinterCfg, pdf_path: &Path) -> Result<()> {
    let exe = windows_backend::ensure_sumatra()?;
    let target = printer.os_id.as_deref().filter(|s| !s.is_empty());
    let args = sumatra_args(target, &pdf_path.to_string_lossy());
    run(&exe.to_string_lossy(), &args)
}

/// Build the SumatraPDF silent-print arguments. Pure, so it is tested on every
/// platform even though it only runs on Windows.
#[cfg_attr(not(windows), allow(dead_code))]
fn sumatra_args(os_id: Option<&str>, file: &str) -> Vec<String> {
    let mut args = Vec::new();
    match os_id {
        Some(p) if !p.is_empty() => {
            args.push("-print-to".into());
            args.push(p.to_string());
        }
        _ => args.push("-print-to-default".into()),
    }
    args.push("-silent".into());
    args.push(file.to_string());
    args
}

/// The bundled SumatraPDF, embedded only in the Windows build and extracted to a
/// stable per-user path on first use.
#[cfg(windows)]
mod windows_backend {
    use std::path::PathBuf;

    use anyhow::{Context, Result};

    /// Pinned version of the vendored exe. Bump when the asset is updated so the
    /// extracted copy is replaced.
    const SUMATRA_VERSION: &str = "3.5.2";
    const SUMATRA_EXE: &[u8] = include_bytes!("../../assets/windows/SumatraPDF.exe");

    /// Extract SumatraPDF to a per-user cache dir on first use and return its
    /// path. Idempotent: a matching existing copy is reused.
    pub fn ensure_sumatra() -> Result<PathBuf> {
        let dir = cache_dir();
        std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        let exe = dir.join(format!("SumatraPDF-{SUMATRA_VERSION}.exe"));
        let up_to_date = std::fs::metadata(&exe)
            .map(|m| m.len() == SUMATRA_EXE.len() as u64)
            .unwrap_or(false);
        if !up_to_date {
            // Write then rename so a concurrent station never sees a partial exe.
            let tmp = dir.join(format!("SumatraPDF-{SUMATRA_VERSION}.exe.tmp"));
            std::fs::write(&tmp, SUMATRA_EXE)
                .with_context(|| format!("writing {}", tmp.display()))?;
            std::fs::rename(&tmp, &exe).with_context(|| format!("installing {}", exe.display()))?;
        }
        Ok(exe)
    }

    fn cache_dir() -> PathBuf {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("broccoli-print")
    }
}

fn run(program: &str, args: &[String]) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .status()
        .with_context(|| format!("running `{program}` (is it installed and on PATH?)"))?;
    if !status.success() {
        bail!("`{program}` exited with {status}");
    }
    Ok(())
}

pub fn enumerate_printers() -> Vec<String> {
    if cfg!(target_os = "windows") {
        Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "Get-Printer | Select-Object -ExpandProperty Name",
            ])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    } else {
        Command::new("lpstat")
            .arg("-a")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .filter_map(|l| l.split_whitespace().next().map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// On Windows, make sure the bundled silent print helper is extracted and
/// report its status. Returns `None` on macOS/Linux, where CUPS needs no helper.
pub fn silent_helper_status() -> Option<String> {
    #[cfg(windows)]
    {
        Some(match windows_backend::ensure_sumatra() {
            Ok(p) => format!("bundled SumatraPDF ready ({})", p.display()),
            Err(e) => format!("bundled SumatraPDF unavailable: {e}"),
        })
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn printer(command: Option<&str>, os_id: Option<&str>) -> PrinterCfg {
        PrinterCfg {
            name: "p".into(),
            os_id: os_id.map(String::from),
            command: command.map(String::from),
        }
    }

    #[test]
    fn detects_folder_sink_from_command_or_os_id() {
        assert_eq!(
            sink_dir(&printer(Some("folder:/tmp/out"), None)).as_deref(),
            Some("/tmp/out")
        );
        assert_eq!(
            sink_dir(&printer(None, Some("folder:/var/spool"))).as_deref(),
            Some("/var/spool")
        );
        assert_eq!(sink_dir(&printer(Some("lp -d X {file}"), None)), None);
    }

    #[test]
    fn sumatra_args_named_printer() {
        assert_eq!(
            sumatra_args(Some("HP_LaserJet"), "C:/tmp/job.pdf"),
            vec!["-print-to", "HP_LaserJet", "-silent", "C:/tmp/job.pdf"]
        );
    }

    #[test]
    fn sumatra_args_default_printer() {
        assert_eq!(
            sumatra_args(None, "C:/tmp/job.pdf"),
            vec!["-print-to-default", "-silent", "C:/tmp/job.pdf"]
        );
        assert_eq!(
            sumatra_args(Some(""), "j.pdf"),
            vec!["-print-to-default", "-silent", "j.pdf"]
        );
    }

    #[test]
    fn folder_sink_copies_pdf() {
        let dir = std::env::temp_dir().join(format!("print-sink-test-{}", std::process::id()));
        let src = dir.join("src.pdf");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&src, b"%PDF-1.7\n").unwrap();
        let out = dir.join("out");
        write_to_folder(out.to_str().unwrap(), &src).unwrap();
        assert!(out.join("src.pdf").exists());
        std::fs::remove_dir_all(&dir).ok();
    }
}
