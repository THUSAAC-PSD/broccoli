#[cfg(target_arch = "wasm32")]
use broccoli_server_sdk::prelude::*;
#[cfg(target_arch = "wasm32")]
use extism_pdk::{FnResult, plugin_fn};

pub mod checkers;
pub mod util;

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn init() -> FnResult<String> {
    host::registry::register_checker_format("exact", "check_exact")?;
    host::registry::register_checker_format("lines", "check_lines")?;
    host::registry::register_checker_format("tokens", "check_tokens")?;
    host::registry::register_checker_format("tokens-case-insensitive", "check_tokens_ci")?;
    host::registry::register_checker_format("tokens-float", "check_tokens_float")?;
    host::registry::register_checker_format("unordered-tokens", "check_unordered_tokens")?;
    host::registry::register_checker_format("unordered-lines", "check_unordered_lines")?;
    host::registry::register_checker_format("testlib", "check_testlib")?;
    host::logger::log_info("Standard checkers registered")?;
    Ok("ok".to_string())
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn check_exact(input: String) -> FnResult<String> {
    let req: CheckerParseInput = serde_json::from_str(&input)?;
    let verdict = checkers::exact::check(&req).map_err(extism_pdk::Error::msg)?;
    Ok(serde_json::to_string(&verdict)?)
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn check_lines(input: String) -> FnResult<String> {
    let req: CheckerParseInput = serde_json::from_str(&input)?;
    let verdict = checkers::lines::check(&req).map_err(extism_pdk::Error::msg)?;
    Ok(serde_json::to_string(&verdict)?)
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn check_tokens(input: String) -> FnResult<String> {
    let req: CheckerParseInput = serde_json::from_str(&input)?;
    let verdict = checkers::tokens::check(&req).map_err(extism_pdk::Error::msg)?;
    Ok(serde_json::to_string(&verdict)?)
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn check_tokens_ci(input: String) -> FnResult<String> {
    let req: CheckerParseInput = serde_json::from_str(&input)?;
    let verdict = checkers::tokens_case::check(&req).map_err(extism_pdk::Error::msg)?;
    Ok(serde_json::to_string(&verdict)?)
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn check_tokens_float(input: String) -> FnResult<String> {
    let req: CheckerParseInput = serde_json::from_str(&input)?;
    let verdict = checkers::tokens_float::check(&req).map_err(extism_pdk::Error::msg)?;
    Ok(serde_json::to_string(&verdict)?)
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn check_unordered_tokens(input: String) -> FnResult<String> {
    let req: CheckerParseInput = serde_json::from_str(&input)?;
    let verdict = checkers::unordered_tokens::check(&req).map_err(extism_pdk::Error::msg)?;
    Ok(serde_json::to_string(&verdict)?)
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn check_unordered_lines(input: String) -> FnResult<String> {
    let req: CheckerParseInput = serde_json::from_str(&input)?;
    let verdict = checkers::unordered_lines::check(&req).map_err(extism_pdk::Error::msg)?;
    Ok(serde_json::to_string(&verdict)?)
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn check_testlib(input: String) -> FnResult<String> {
    let req: CheckerParseInput = serde_json::from_str(&input)?;
    let verdict = checkers::testlib::dispatch_testlib_checker(&req);
    Ok(serde_json::to_string(&verdict)?)
}
