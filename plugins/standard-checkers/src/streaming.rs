use std::collections::VecDeque;

use broccoli_server_sdk::types::{CheckerVerdict, Verdict};
use serde_json::Value;

use crate::util::{
    line_count_msg, line_mismatch_msg, token_count_msg, token_mismatch_msg, truncate,
};

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct FloatConfig {
    #[serde(default = "default_abs_tol")]
    abs_tol: f64,
    #[serde(default = "default_rel_tol")]
    rel_tol: f64,
}

impl Default for FloatConfig {
    fn default() -> Self {
        Self {
            abs_tol: default_abs_tol(),
            rel_tol: default_rel_tol(),
        }
    }
}

fn default_abs_tol() -> f64 {
    1e-9
}

fn default_rel_tol() -> f64 {
    1e-6
}

pub trait ByteSource {
    fn next_chunk(&mut self) -> Result<Option<Vec<u8>>, String>;
}

pub struct MemoryByteSource {
    bytes: Vec<u8>,
    offset: usize,
    chunk_size: usize,
}

impl MemoryByteSource {
    pub fn new(bytes: Vec<u8>, chunk_size: usize) -> Self {
        Self {
            bytes,
            offset: 0,
            chunk_size: chunk_size.max(1),
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub struct BlobByteSource<'a> {
    storage: &'a broccoli_server_sdk::Storage,
    hash: String,
    token: String,
    offset: u64,
    chunk_size: usize,
    done: bool,
}

#[cfg(target_arch = "wasm32")]
impl<'a> BlobByteSource<'a> {
    pub fn new(
        storage: &'a broccoli_server_sdk::Storage,
        hash: String,
        token: String,
        chunk_size: usize,
    ) -> Self {
        Self {
            storage,
            hash,
            token,
            offset: 0,
            chunk_size: chunk_size.max(1),
            done: false,
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl ByteSource for BlobByteSource<'_> {
    fn next_chunk(&mut self) -> Result<Option<Vec<u8>>, String> {
        if self.done {
            return Ok(None);
        }
        let range = self
            .storage
            .read_blob_range(&self.token, &self.hash, self.offset, self.chunk_size)
            .map_err(|e| e.to_string())?;
        self.offset += range.bytes.len() as u64;
        self.done = range.eof;
        if range.bytes.is_empty() {
            Ok(None)
        } else {
            Ok(Some(range.bytes))
        }
    }
}

impl ByteSource for MemoryByteSource {
    fn next_chunk(&mut self) -> Result<Option<Vec<u8>>, String> {
        if self.offset >= self.bytes.len() {
            return Ok(None);
        }
        let end = self
            .offset
            .saturating_add(self.chunk_size)
            .min(self.bytes.len());
        let chunk = self.bytes[self.offset..end].to_vec();
        self.offset = end;
        Ok(Some(chunk))
    }
}

#[derive(Debug, Clone)]
pub enum StreamingFormat {
    Exact,
    Lines,
    Tokens,
    TokensCaseInsensitive,
    TokensFloat,
}

pub fn check_streaming(
    format: StreamingFormat,
    expected: Box<dyn ByteSource + '_>,
    actual: Box<dyn ByteSource + '_>,
    config: Option<&Value>,
) -> Result<CheckerVerdict, String> {
    match format {
        StreamingFormat::Exact => compare_exact(expected, actual),
        StreamingFormat::Lines => compare_lines(expected, actual),
        StreamingFormat::Tokens => compare_tokens(expected, actual, TokenMode::Exact),
        StreamingFormat::TokensCaseInsensitive => {
            compare_tokens(expected, actual, TokenMode::CaseInsensitive)
        }
        StreamingFormat::TokensFloat => {
            let cfg: FloatConfig = config
                .map(|v| serde_json::from_value(v.clone()))
                .transpose()
                .map_err(|e| format!("Invalid checker config: {e}"))?
                .unwrap_or_default();
            compare_tokens(
                expected,
                actual,
                TokenMode::Float {
                    abs_tol: cfg.abs_tol,
                    rel_tol: cfg.rel_tol,
                },
            )
        }
    }
}

fn accepted() -> CheckerVerdict {
    CheckerVerdict {
        verdict: Verdict::Accepted,
        score: 1.0,
        message: None,
    }
}

fn wrong(message: String) -> CheckerVerdict {
    CheckerVerdict {
        verdict: Verdict::WrongAnswer,
        score: 0.0,
        message: Some(message),
    }
}

fn compare_exact(
    mut expected: Box<dyn ByteSource + '_>,
    mut actual: Box<dyn ByteSource + '_>,
) -> Result<CheckerVerdict, String> {
    let mut expected_buf = VecDeque::<u8>::new();
    let mut actual_buf = VecDeque::<u8>::new();
    let mut expected_done = false;
    let mut actual_done = false;
    let mut offset = 0usize;

    loop {
        fill_byte_queue(&mut *expected, &mut expected_buf, &mut expected_done)?;
        fill_byte_queue(&mut *actual, &mut actual_buf, &mut actual_done)?;

        while !expected_buf.is_empty() && !actual_buf.is_empty() {
            let exp = expected_buf.pop_front().unwrap();
            let act = actual_buf.pop_front().unwrap();
            if exp != act {
                return Ok(wrong(format!(
                    "Expected and actual output differ near byte offset {offset}"
                )));
            }
            offset += 1;
        }

        if expected_done && actual_done && expected_buf.is_empty() && actual_buf.is_empty() {
            return Ok(accepted());
        }
        if (expected_done && expected_buf.is_empty() && !actual_buf.is_empty())
            || (actual_done && actual_buf.is_empty() && !expected_buf.is_empty())
        {
            return Ok(wrong(format!(
                "Expected and actual output differ near byte offset {offset}"
            )));
        }
    }
}

fn fill_byte_queue(
    source: &mut dyn ByteSource,
    queue: &mut VecDeque<u8>,
    done: &mut bool,
) -> Result<(), String> {
    if !queue.is_empty() || *done {
        return Ok(());
    }
    match source.next_chunk()? {
        Some(chunk) => queue.extend(chunk),
        None => *done = true,
    }
    Ok(())
}

struct CharStream<'a> {
    source: Box<dyn ByteSource + 'a>,
    pending_bytes: Vec<u8>,
    chars: VecDeque<char>,
    eof: bool,
}

impl<'a> CharStream<'a> {
    fn new(source: Box<dyn ByteSource + 'a>) -> Self {
        Self {
            source,
            pending_bytes: Vec::new(),
            chars: VecDeque::new(),
            eof: false,
        }
    }

    fn next_char(&mut self) -> Result<Option<char>, String> {
        loop {
            if let Some(ch) = self.chars.pop_front() {
                return Ok(Some(ch));
            }
            if self.eof {
                if self.pending_bytes.is_empty() {
                    return Ok(None);
                }
                return Err("Stream ended with incomplete UTF-8 sequence".to_string());
            }
            self.fill()?;
        }
    }

    fn fill(&mut self) -> Result<(), String> {
        match self.source.next_chunk()? {
            Some(chunk) => self.pending_bytes.extend(chunk),
            None => self.eof = true,
        }

        match std::str::from_utf8(&self.pending_bytes) {
            Ok(valid) => {
                self.chars.extend(valid.chars());
                self.pending_bytes.clear();
                Ok(())
            }
            Err(e) => {
                if e.error_len().is_some() {
                    return Err(format!("Stream is not UTF-8: {e}"));
                }
                let valid_up_to = e.valid_up_to();
                if valid_up_to == 0 {
                    return Ok(());
                }
                let valid = std::str::from_utf8(&self.pending_bytes[..valid_up_to])
                    .map_err(|err| format!("Stream is not UTF-8: {err}"))?;
                self.chars.extend(valid.chars());
                self.pending_bytes.drain(..valid_up_to);
                Ok(())
            }
        }
    }
}

struct TokenStream<'a> {
    chars: CharStream<'a>,
    partial: String,
}

impl<'a> TokenStream<'a> {
    fn new(source: Box<dyn ByteSource + 'a>) -> Self {
        Self {
            chars: CharStream::new(source),
            partial: String::new(),
        }
    }

    fn next_token(&mut self) -> Result<Option<String>, String> {
        while let Some(ch) = self.chars.next_char()? {
            if ch.is_whitespace() {
                if !self.partial.is_empty() {
                    return Ok(Some(std::mem::take(&mut self.partial)));
                }
            } else {
                self.partial.push(ch);
            }
        }

        if self.partial.is_empty() {
            Ok(None)
        } else {
            Ok(Some(std::mem::take(&mut self.partial)))
        }
    }
}

enum TokenMode {
    Exact,
    CaseInsensitive,
    Float { abs_tol: f64, rel_tol: f64 },
}

fn compare_tokens(
    expected: Box<dyn ByteSource + '_>,
    actual: Box<dyn ByteSource + '_>,
    mode: TokenMode,
) -> Result<CheckerVerdict, String> {
    let mut expected = TokenStream::new(expected);
    let mut actual = TokenStream::new(actual);
    let mut position = 1usize;

    loop {
        let exp = expected.next_token()?;
        let act = actual.next_token()?;
        match (exp, act) {
            (Some(exp), Some(act)) => {
                if !tokens_match(&exp, &act, &mode) {
                    return Ok(token_mismatch_verdict(position, &exp, &act, &mode));
                }
            }
            (None, None) => return Ok(accepted()),
            (Some(_), None) => {
                let expected_count = position + count_remaining_tokens(&mut expected)?;
                return Ok(wrong(token_count_msg(expected_count, position - 1)));
            }
            (None, Some(_)) => {
                let actual_count = position + count_remaining_tokens(&mut actual)?;
                return Ok(wrong(token_count_msg(position - 1, actual_count)));
            }
        }
        position += 1;
    }
}

fn count_remaining_tokens(stream: &mut TokenStream<'_>) -> Result<usize, String> {
    let mut count = 0usize;
    while stream.next_token()?.is_some() {
        count += 1;
    }
    Ok(count)
}

fn tokens_match(expected: &str, actual: &str, mode: &TokenMode) -> bool {
    match mode {
        TokenMode::Exact => expected == actual,
        TokenMode::CaseInsensitive => expected.to_lowercase() == actual.to_lowercase(),
        TokenMode::Float { abs_tol, rel_tol } => {
            match (expected.parse::<f64>(), actual.parse::<f64>()) {
                (Ok(exp_f), Ok(act_f)) => {
                    if !exp_f.is_finite() || !act_f.is_finite() {
                        (exp_f.is_nan() && act_f.is_nan()) || exp_f == act_f
                    } else {
                        let diff = (exp_f - act_f).abs();
                        let tolerance = abs_tol.max(rel_tol * exp_f.abs().max(act_f.abs()));
                        diff <= tolerance
                    }
                }
                _ => expected == actual,
            }
        }
    }
}

fn token_mismatch_verdict(
    position: usize,
    expected: &str,
    actual: &str,
    mode: &TokenMode,
) -> CheckerVerdict {
    if let TokenMode::Float { abs_tol, rel_tol } = mode
        && let (Ok(exp_f), Ok(act_f)) = (expected.parse::<f64>(), actual.parse::<f64>())
    {
        if !exp_f.is_finite() || !act_f.is_finite() {
            return wrong(format!(
                "Non-finite mismatch at token {}: expected {}, got {}",
                position,
                truncate(expected, 50),
                truncate(actual, 50)
            ));
        }
        let diff = (exp_f - act_f).abs();
        let tolerance = abs_tol.max(rel_tol * exp_f.abs().max(act_f.abs()));
        return wrong(format!(
            "Float mismatch at token {}: expected {}, got {}, diff {} > tolerance {}",
            position, expected, actual, diff, tolerance
        ));
    }

    wrong(token_mismatch_msg(position, expected, actual))
}

struct LineStream<'a> {
    chars: CharStream<'a>,
    current: String,
    pending_empty: usize,
    queued: VecDeque<String>,
}

impl<'a> LineStream<'a> {
    fn new(source: Box<dyn ByteSource + 'a>) -> Self {
        Self {
            chars: CharStream::new(source),
            current: String::new(),
            pending_empty: 0,
            queued: VecDeque::new(),
        }
    }

    fn next_line(&mut self) -> Result<Option<String>, String> {
        loop {
            if let Some(line) = self.queued.pop_front() {
                return Ok(Some(line));
            }
            match self.next_raw_line()? {
                Some(line) => {
                    let line = line.trim_end().to_string();
                    if line.is_empty() {
                        self.pending_empty += 1;
                        continue;
                    }
                    for _ in 0..self.pending_empty {
                        self.queued.push_back(String::new());
                    }
                    self.pending_empty = 0;
                    self.queued.push_back(line);
                }
                None => {
                    self.pending_empty = 0;
                    return Ok(None);
                }
            }
        }
    }

    fn next_raw_line(&mut self) -> Result<Option<String>, String> {
        while let Some(ch) = self.chars.next_char()? {
            if ch == '\n' {
                if self.current.ends_with('\r') {
                    self.current.pop();
                }
                return Ok(Some(std::mem::take(&mut self.current)));
            }
            self.current.push(ch);
        }
        if self.current.is_empty() {
            Ok(None)
        } else {
            Ok(Some(std::mem::take(&mut self.current)))
        }
    }
}

fn compare_lines(
    expected: Box<dyn ByteSource + '_>,
    actual: Box<dyn ByteSource + '_>,
) -> Result<CheckerVerdict, String> {
    let mut expected = LineStream::new(expected);
    let mut actual = LineStream::new(actual);
    let mut line_no = 1usize;

    loop {
        let exp = expected.next_line()?;
        let act = actual.next_line()?;
        match (exp, act) {
            (Some(exp), Some(act)) => {
                if exp != act {
                    return Ok(wrong(line_mismatch_msg(line_no, &exp, &act)));
                }
            }
            (None, None) => return Ok(accepted()),
            (Some(_), None) => {
                let expected_count = line_no + count_remaining_lines(&mut expected)?;
                return Ok(wrong(line_count_msg(expected_count, line_no - 1)));
            }
            (None, Some(_)) => {
                let actual_count = line_no + count_remaining_lines(&mut actual)?;
                return Ok(wrong(line_count_msg(line_no - 1, actual_count)));
            }
        }
        line_no += 1;
    }
}

fn count_remaining_lines(stream: &mut LineStream<'_>) -> Result<usize, String> {
    let mut count = 0usize;
    while stream.next_line()?.is_some() {
        count += 1;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use broccoli_server_sdk::types::Verdict;

    use super::*;

    #[test]
    fn token_checker_accepts_large_chunked_output() {
        let expected = format!("{} tail", "123 ".repeat(100_000));
        let actual = format!("{} tail", "123\n".repeat(100_000));

        let verdict = check_streaming(
            StreamingFormat::Tokens,
            Box::new(MemoryByteSource::new(expected.into_bytes(), 4096)),
            Box::new(MemoryByteSource::new(actual.into_bytes(), 3072)),
            None,
        )
        .unwrap();

        assert_eq!(verdict.verdict, Verdict::Accepted);
    }

    #[test]
    fn token_checker_reports_first_mismatch_position() {
        let verdict = check_streaming(
            StreamingFormat::Tokens,
            Box::new(MemoryByteSource::new(b"a b c".to_vec(), 2)),
            Box::new(MemoryByteSource::new(b"a x c".to_vec(), 2)),
            None,
        )
        .unwrap();

        assert_eq!(verdict.verdict, Verdict::WrongAnswer);
        assert!(verdict.message.unwrap().contains("position 2"));
    }

    #[test]
    fn exact_checker_compares_bytes_across_chunk_boundaries() {
        let verdict = check_streaming(
            StreamingFormat::Exact,
            Box::new(MemoryByteSource::new(b"42\n".to_vec(), 1)),
            Box::new(MemoryByteSource::new(b"42".to_vec(), 2)),
            None,
        )
        .unwrap();

        assert_eq!(verdict.verdict, Verdict::WrongAnswer);
    }

    #[test]
    fn lines_checker_preserves_internal_empty_lines_but_drops_trailing_empty_lines() {
        let verdict = check_streaming(
            StreamingFormat::Lines,
            Box::new(MemoryByteSource::new(b"a\n\nb\n\n".to_vec(), 2)),
            Box::new(MemoryByteSource::new(b"a\n\nb\n".to_vec(), 3)),
            None,
        )
        .unwrap();

        assert_eq!(verdict.verdict, Verdict::Accepted);
    }

    #[test]
    fn float_checker_uses_tolerance_config() {
        let verdict = check_streaming(
            StreamingFormat::TokensFloat,
            Box::new(MemoryByteSource::new(b"1.0".to_vec(), 1)),
            Box::new(MemoryByteSource::new(b"1.5".to_vec(), 1)),
            Some(&serde_json::json!({ "abs_tol": 1.0 })),
        )
        .unwrap();

        assert_eq!(verdict.verdict, Verdict::Accepted);
    }

    #[test]
    fn float_checker_rejects_unknown_config_fields() {
        let err = check_streaming(
            StreamingFormat::TokensFloat,
            Box::new(MemoryByteSource::new(b"1.0".to_vec(), 1)),
            Box::new(MemoryByteSource::new(b"1.0".to_vec(), 1)),
            Some(&serde_json::json!({ "unexpected": 1.0 })),
        )
        .unwrap_err();

        assert!(err.contains("Invalid checker config"));
    }
}
