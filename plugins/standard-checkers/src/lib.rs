#[cfg(target_arch = "wasm32")]
use broccoli_server_sdk::prelude::*;
#[cfg(target_arch = "wasm32")]
use extism_pdk::{FnResult, plugin_fn};
#[cfg(target_arch = "wasm32")]
use streaming::{BlobByteSource, ByteSource, MemoryByteSource, StreamingFormat};

pub mod checkers;
pub mod streaming;
pub mod util;

#[cfg(target_arch = "wasm32")]
const SUPPORTED_CHECKER_FORMATS: &[(&str, &str)] = &[
    ("exact", "check_exact"),
    ("lines", "check_lines"),
    ("tokens", "check_tokens"),
    ("tokens-case-insensitive", "check_tokens_ci"),
    ("tokens-float", "check_tokens_float"),
    ("testlib", "check_testlib"),
    ("none", "check_none"),
];

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn init() -> FnResult<String> {
    let host = Host::new();
    for (format, handler) in SUPPORTED_CHECKER_FORMATS {
        host.registry.register_checker_format(format, handler)?;
    }
    host.log.info("Standard checkers registered")?;
    Ok("ok".to_string())
}

#[cfg(target_arch = "wasm32")]
const STREAMING_CHECKER_CHUNK_BYTES: usize = 1024 * 1024;

#[cfg(target_arch = "wasm32")]
fn streaming_source<'a>(host: &'a Host, file: &'a JudgeFile) -> Box<dyn ByteSource + 'a> {
    match file {
        JudgeFile::Blob { file } => Box::new(BlobByteSource::new(
            &host.storage,
            file.blob_hash.clone(),
            file.read_token.clone().unwrap_or_default(),
            STREAMING_CHECKER_CHUNK_BYTES,
        )),
        JudgeFile::Inline { text } => Box::new(MemoryByteSource::new(
            text.as_bytes().to_vec(),
            STREAMING_CHECKER_CHUNK_BYTES,
        )),
        JudgeFile::Missing => Box::new(MemoryByteSource::new(
            Vec::new(),
            STREAMING_CHECKER_CHUNK_BYTES,
        )),
    }
}

#[cfg(target_arch = "wasm32")]
fn blob_backed(req: &CheckerParseInput) -> bool {
    req.stdout.is_blob() || req.expected_output.is_blob()
}

#[cfg(target_arch = "wasm32")]
fn check_streaming_if_blob(
    host: &Host,
    req: &CheckerParseInput,
    format: StreamingFormat,
) -> Result<Option<CheckerVerdict>, extism_pdk::Error> {
    if !blob_backed(req) {
        return Ok(None);
    }

    let expected = streaming_source(host, &req.expected_output);
    let actual = streaming_source(host, &req.stdout);
    let verdict = streaming::check_streaming(format, expected, actual, req.config.as_ref())
        .map_err(extism_pdk::Error::msg)?;
    Ok(Some(verdict))
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn check_exact(input: String) -> FnResult<String> {
    let host = Host::new();
    let req: CheckerParseInput = serde_json::from_str(&input)?;
    let verdict = match check_streaming_if_blob(&host, &req, StreamingFormat::Exact)? {
        Some(verdict) => verdict,
        None => checkers::exact::check(&req).map_err(extism_pdk::Error::msg)?,
    };
    Ok(serde_json::to_string(&verdict)?)
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn check_lines(input: String) -> FnResult<String> {
    let host = Host::new();
    let req: CheckerParseInput = serde_json::from_str(&input)?;
    let verdict = match check_streaming_if_blob(&host, &req, StreamingFormat::Lines)? {
        Some(verdict) => verdict,
        None => checkers::lines::check(&req).map_err(extism_pdk::Error::msg)?,
    };
    Ok(serde_json::to_string(&verdict)?)
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn check_tokens(input: String) -> FnResult<String> {
    let host = Host::new();
    let req: CheckerParseInput = serde_json::from_str(&input)?;
    let verdict = match check_streaming_if_blob(&host, &req, StreamingFormat::Tokens)? {
        Some(verdict) => verdict,
        None => checkers::tokens::check(&req).map_err(extism_pdk::Error::msg)?,
    };
    Ok(serde_json::to_string(&verdict)?)
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn check_tokens_ci(input: String) -> FnResult<String> {
    let host = Host::new();
    let req: CheckerParseInput = serde_json::from_str(&input)?;
    let verdict =
        match check_streaming_if_blob(&host, &req, StreamingFormat::TokensCaseInsensitive)? {
            Some(verdict) => verdict,
            None => checkers::tokens_case::check(&req).map_err(extism_pdk::Error::msg)?,
        };
    Ok(serde_json::to_string(&verdict)?)
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn check_tokens_float(input: String) -> FnResult<String> {
    let host = Host::new();
    let req: CheckerParseInput = serde_json::from_str(&input)?;
    let verdict = match check_streaming_if_blob(&host, &req, StreamingFormat::TokensFloat)? {
        Some(verdict) => verdict,
        None => checkers::tokens_float::check(&req).map_err(extism_pdk::Error::msg)?,
    };
    Ok(serde_json::to_string(&verdict)?)
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn check_none(input: String) -> FnResult<String> {
    let req: CheckerParseInput = serde_json::from_str(&input)?;
    let verdict = checkers::none::check(&req).map_err(extism_pdk::Error::msg)?;
    Ok(serde_json::to_string(&verdict)?)
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn check_testlib(input: String) -> FnResult<String> {
    let host = Host::new();
    let req: CheckerParseInput = serde_json::from_str(&input)?;
    let verdict = checkers::testlib::dispatch_testlib_checker(&host, &req);
    Ok(serde_json::to_string(&verdict)?)
}
