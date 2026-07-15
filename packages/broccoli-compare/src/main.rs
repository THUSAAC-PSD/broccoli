//! `broccoli-compare` — native output comparator.
//!
//! Reads a solution's output on **stdin** and an expected answer from a
//! **file** (`--answer <path>`), compares them according to `--mode`, prints a
//! short human message to **stderr**, and exits with a verdict code:
//! `0` = Accepted, `1` = WrongAnswer, `2` = PresentationError.
//!
//! CLI contract:
//! ```text
//! broccoli-compare --mode <exact|tokens|tokens-case-insensitive|tokens-float|lines> \
//!     --answer <path> [--epsilon <f64>] [--rel-epsilon <f64>]
//! ```
//!
//! Exit codes: `0` = Accepted, `1` = WrongAnswer, `64` = usage/IO error (mapped
//! to a checker SystemError by the interpreter). The built-in comparers never
//! emit a PresentationError; only testlib does, through its own interpreter.
//!
//! `--epsilon` and `--rel-epsilon` only affect `tokens-float`: they override the
//! absolute and relative tolerances respectively. Together they reproduce the
//! WASM `FloatConfig { abs_tol, rel_tol }` so verdict identity holds for problems
//! that customize either tolerance (the Phase 5 resolver maps `config.abs_tol ->
//! --epsilon` and `config.rel_tol -> --rel-epsilon`).
//!
//! Output channels (so the fused checker step returns both inline, no blob):
//!   * **stdout** receives the **first 64 KiB** of stdin (the display preview),
//!     tee'd as bytes flow through — captured regardless of the verdict (even an
//!     early-decided WA), never by buffering the whole stream. The worker reads a
//!     File-redirected stdout back into the step result, so the coordinator gets
//!     the preview without a blob round-trip.
//!   * **stderr** receives the short verdict message — the same channel testlib
//!     uses, so the evaluator sources the checker message uniformly.
//!   * stdin is always drained to EOF before exit, so a still-running solution
//!     piping into us never receives SIGPIPE/EPIPE when the verdict is decided
//!     early. `main` owns the stdin reader (wrapped in [`TeeReader`]) so it can
//!     keep draining after the borrowing `compare_*` call returns.

mod modes;

use std::fs::File;
use std::io::{self, BufReader, Read, Write};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};

use modes::{TokenMode, Verdict};

/// Default tolerances for `tokens-float`, mirroring
/// `plugins/standard-checkers/src/checkers/tokens_float.rs` (`abs_tol = 1e-9`,
/// `rel_tol = 1e-6`).
const DEFAULT_ABS_TOL: f64 = 1e-9;
const DEFAULT_REL_TOL: f64 = 1e-6;

/// First-N-bytes preview cap. Matches `INLINE_OUTPUT_PREVIEW_BYTES` in
/// `packages/worker/src/models/operation/sandbox/isolate.rs`.
const INLINE_OUTPUT_PREVIEW_BYTES: usize = 64 * 1024;

/// Parsed command-line arguments.
struct Args {
    mode: String,
    answer: String,
    /// `--epsilon`, when present, overrides the absolute float tolerance.
    epsilon: Option<f64>,
    /// `--rel-epsilon`, when present, overrides the relative float tolerance.
    rel_epsilon: Option<f64>,
}

fn main() -> ExitCode {
    match run() {
        Ok(verdict) => {
            // The verdict message goes to stderr (stdout carries the preview).
            // Best-effort: a broken stderr pipe must not change the verdict.
            let _ = writeln!(io::stderr(), "{}", verdict.message());
            ExitCode::from(verdict.exit_code() as u8)
        }
        Err(err) => {
            eprintln!("broccoli-compare: {err:#}");
            // Reserve 0/1/2 for verdicts; use a distinct code for usage/IO errors.
            ExitCode::from(64)
        }
    }
}

fn run() -> Result<Verdict> {
    let args = parse_args(std::env::args().skip(1))?;

    let answer = File::open(&args.answer)
        .with_context(|| format!("opening answer file '{}'", args.answer))?;
    let answer = BufReader::new(answer);

    // The preview sink is stdout: the first 64 KiB of the solution output is
    // tee'd there as a display preview, which the worker reads back into the
    // step result inline (no blob round-trip). The verdict message goes to
    // stderr (see `main`), so the two never interleave.
    let preview = Some(io::stdout());

    // `main` owns the stdin reader, wrapped in a tee. The `compare_*` call below
    // only *borrows* it (&mut), so after the verdict is decided we still own the
    // reader and can drain it to EOF — never SIGPIPE-killing a live producer.
    let stdin = io::stdin();
    let mut output = TeeReader::new(stdin.lock(), preview);

    // Run the comparison. It may stop reading stdin early (e.g. an early
    // mismatch); whatever happens, we drain stdin to EOF afterwards.
    let verdict = compare(&args, &mut output, answer);

    // Drain whatever the comparison left unread so the upstream solution never
    // gets SIGPIPE. This also keeps teeing into the preview until the 64 KiB cap
    // is reached. Do this even when the comparison errored.
    let drained = output.drain_to_eof();
    let flushed = output.flush_preview();

    finalize(verdict, drained, flushed)
}

/// Combine the comparison result with the post-verdict side-channel results.
///
/// The verdict (or the error that prevented one) is **authoritative**. The
/// preview/drain side channel must never mask a decided AC/WA: a flaky write or
/// a stdin read error during the post-verdict drain is reported as a warning
/// only once we know the comparison itself produced a verdict. A genuine
/// comparison error is still propagated.
fn finalize(
    verdict: std::io::Result<Verdict>,
    drained: std::io::Result<()>,
    flushed: std::io::Result<()>,
) -> Result<Verdict> {
    let verdict = verdict.context("comparing output against answer")?;
    if let Err(e) = drained {
        eprintln!("broccoli-compare: warning: draining stdin to EOF: {e}");
    }
    if let Err(e) = flushed {
        eprintln!("broccoli-compare: warning: flushing preview file: {e}");
    }
    Ok(verdict)
}

/// Dispatch on `--mode`, comparing the borrowed `output` reader against
/// `answer`.
fn compare<O: Read, A: Read>(args: &Args, output: &mut O, answer: A) -> std::io::Result<Verdict> {
    match args.mode.as_str() {
        "exact" => modes::compare_exact(output, answer),
        "tokens" => modes::compare_tokens(output, answer, TokenMode::Exact),
        "tokens-case-insensitive" => {
            modes::compare_tokens(output, answer, TokenMode::CaseInsensitive)
        }
        "tokens-float" => {
            // `--epsilon`/`--rel-epsilon` override the absolute/relative
            // tolerances; whichever is omitted keeps its default so the combined
            // `max(abs_tol, rel_tol * max(|a|,|b|))` rule still holds and matches
            // the WASM `FloatConfig` defaults (1e-9 / 1e-6).
            let abs_tol = args.epsilon.unwrap_or(DEFAULT_ABS_TOL);
            let rel_tol = args.rel_epsilon.unwrap_or(DEFAULT_REL_TOL);
            modes::compare_tokens(output, answer, TokenMode::Float { abs_tol, rel_tol })
        }
        "lines" => modes::compare_lines(output, answer),
        // Unreachable: parse_args (below) does not validate the mode string, so
        // an unknown mode reaches here and is rejected as a usage error.
        other => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "unsupported --mode '{other}' (expected one of: exact, tokens, \
                 tokens-case-insensitive, tokens-float, lines)"
            ),
        )),
    }
}

/// A `Read` adapter that copies the **first `INLINE_OUTPUT_PREVIEW_BYTES`** bytes
/// flowing through it into an optional preview sink, while passing every byte to
/// the caller unchanged. Only the preview cap's worth of bytes is ever written
/// to disk — the stream itself is never accumulated in memory. The tee happens
/// on the read path so the preview is the true first 64 KiB even when the
/// comparison stops reading early.
///
/// The preview is a **best-effort side channel**: a write error disables it and
/// is otherwise ignored, so a broken preview never fails an in-flight comparison
/// (production uses a `File`; the generic `W` lets tests inject a failing sink).
struct TeeReader<R: Read, W: Write> {
    inner: R,
    preview: Option<W>,
    captured: usize,
}

impl<R: Read, W: Write> TeeReader<R, W> {
    fn new(inner: R, preview: Option<W>) -> Self {
        Self {
            inner,
            preview,
            captured: 0,
        }
    }

    /// Tee the freshly-read `chunk` into the preview sink, up to the cap. On any
    /// write error the preview is dropped and teeing stops — the comparison is
    /// unaffected.
    fn tee(&mut self, chunk: &[u8]) {
        if let Some(sink) = self.preview.as_mut()
            && self.captured < INLINE_OUTPUT_PREVIEW_BYTES
        {
            let remaining = INLINE_OUTPUT_PREVIEW_BYTES - self.captured;
            let take = remaining.min(chunk.len());
            match sink.write_all(&chunk[..take]) {
                Ok(()) => self.captured += take,
                Err(_) => self.preview = None,
            }
        }
    }

    /// Read (and tee) the rest of the stream to EOF, discarding the bytes. Called
    /// after the verdict is decided so a live producer never gets SIGPIPE.
    fn drain_to_eof(&mut self) -> io::Result<()> {
        let mut scratch = [0u8; 64 * 1024];
        loop {
            match self.read(&mut scratch) {
                Ok(0) => return Ok(()),
                Ok(_) => continue,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
    }

    /// Flush the preview sink, if it is still alive.
    fn flush_preview(&mut self) -> io::Result<()> {
        if let Some(sink) = self.preview.as_mut() {
            sink.flush()?;
        }
        Ok(())
    }
}

impl<R: Read, W: Write> Read for TeeReader<R, W> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        if n > 0 {
            self.tee(&buf[..n]);
        }
        Ok(n)
    }
}

fn parse_args<I: Iterator<Item = String>>(mut iter: I) -> Result<Args> {
    let mut mode: Option<String> = None;
    let mut answer: Option<String> = None;
    let mut epsilon: Option<f64> = None;
    let mut rel_epsilon: Option<f64> = None;

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--mode" => mode = Some(expect_value(&mut iter, "--mode")?),
            "--answer" => answer = Some(expect_value(&mut iter, "--answer")?),
            "--epsilon" => {
                let raw = expect_value(&mut iter, "--epsilon")?;
                epsilon = Some(
                    raw.parse::<f64>()
                        .with_context(|| format!("invalid --epsilon value '{raw}'"))?,
                );
            }
            "--rel-epsilon" => {
                let raw = expect_value(&mut iter, "--rel-epsilon")?;
                rel_epsilon = Some(
                    raw.parse::<f64>()
                        .with_context(|| format!("invalid --rel-epsilon value '{raw}'"))?,
                );
            }
            other => bail!("unknown argument '{other}'"),
        }
    }

    Ok(Args {
        mode: mode.context("missing required --mode")?,
        answer: answer.context("missing required --answer")?,
        epsilon,
        rel_epsilon,
    })
}

fn expect_value<I: Iterator<Item = String>>(iter: &mut I, flag: &str) -> Result<String> {
    iter.next()
        .with_context(|| format!("flag '{flag}' requires a value"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `Write` that fails every operation, to prove the preview side channel
    /// is best-effort.
    struct FailingWriter;

    impl Write for FailingWriter {
        fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("preview write failed"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::other("preview flush failed"))
        }
    }

    #[test]
    fn preview_write_error_does_not_fail_the_read() {
        let data = b"some solution output";
        let mut tee = TeeReader::new(&data[..], Some(FailingWriter));
        let mut buf = [0u8; 8];

        // The first read tees into a preview writer that errors. The read must
        // still succeed and return the real bytes — a broken preview must never
        // turn a valid comparison into an error.
        let n = tee
            .read(&mut buf)
            .expect("read must not fail when the preview write fails");
        assert!(n > 0);
        assert_eq!(&buf[..n], &data[..n]);

        // The preview is disabled after the error; later reads keep working.
        let n2 = tee
            .read(&mut buf)
            .expect("read still works after preview died");
        assert!(n2 > 0);
    }

    #[test]
    fn finalize_returns_verdict_despite_drain_error() {
        let v = finalize(
            Ok(Verdict::WrongAnswer),
            Err(io::Error::other("drain boom")),
            Ok(()),
        )
        .expect("a decided verdict must survive a drain error");
        assert_eq!(v, Verdict::WrongAnswer);
    }

    #[test]
    fn finalize_returns_verdict_despite_flush_error() {
        let v = finalize(
            Ok(Verdict::Accepted),
            Ok(()),
            Err(io::Error::other("flush boom")),
        )
        .expect("a decided verdict must survive a flush error");
        assert_eq!(v, Verdict::Accepted);
    }

    #[test]
    fn finalize_propagates_comparison_error() {
        let r = finalize(
            Err(io::Error::new(io::ErrorKind::InvalidData, "not utf8")),
            Ok(()),
            Ok(()),
        );
        assert!(
            r.is_err(),
            "a real comparison failure must not be swallowed"
        );
    }
}
