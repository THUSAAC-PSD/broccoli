//! Printing backends. CUPS `lp`, the Windows print verb, a command template,
//! or a folder sink. No third-party PDF viewer needed.

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

fn default_print(printer: &PrinterCfg, pdf_path: &Path) -> Result<()> {
    let target = printer.os_id.as_deref().filter(|s| !s.is_empty());
    let file = pdf_path.to_string_lossy().to_string();

    if cfg!(target_os = "windows") {
        let script = match target {
            Some(p) => {
                format!("Start-Process -FilePath '{file}' -Verb PrintTo -ArgumentList '{p}'")
            }
            None => format!("Start-Process -FilePath '{file}' -Verb Print"),
        };
        run(
            "powershell",
            &["-NoProfile".into(), "-Command".into(), script],
        )
    } else {
        let mut args = Vec::new();
        if let Some(p) = target {
            args.push("-d".to_string());
            args.push(p.to_string());
        }
        args.push(file);
        run("lp", &args)
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
