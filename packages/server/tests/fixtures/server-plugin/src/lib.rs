use std::collections::HashMap;

use extism_pdk::{FnResult, host_fn, plugin_fn};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct PluginHttpRequest {
    pub method: String,
    pub params: HashMap<String, String>,
    pub query: HashMap<String, String>,
    pub body: Option<serde_json::Value>,
}

#[derive(Serialize)]
struct PluginHttpResponse {
    pub status: u16,
    pub body: Option<serde_json::Value>,
}

#[host_fn]
extern "ExtismHost" {
    fn store_set(collection: String, key: String, value: String);
    fn store_get(collection: String, key: String) -> String;
    fn db_execute(sql: String) -> u64;
    fn db_query(sql: String) -> String;
}

/// POST /kv/{key}
#[plugin_fn]
pub fn kv_write(input: String) -> FnResult<String> {
    let req: PluginHttpRequest = serde_json::from_str(&input)?;
    let key = req.params.get("key").cloned().unwrap();
    let val = req.body.and_then(|b| b.get("value").cloned()).unwrap();

    unsafe {
        store_set("tests".into(), key, val.as_str().unwrap().into())?;
    }
    Ok(serde_json::to_string(&PluginHttpResponse {
        status: 200,
        body: None,
    })?)
}

/// GET /kv/{key}
#[plugin_fn]
pub fn kv_read(input: String) -> FnResult<String> {
    let req: PluginHttpRequest = serde_json::from_str(&input)?;
    let key = req.params.get("key").cloned().unwrap();
    let raw = unsafe { store_get("tests".into(), key)? };

    let (status, body) = if raw == "null" {
        (404, serde_json::json!(null))
    } else {
        (200, serde_json::json!({ "value": raw }))
    };
    Ok(serde_json::to_string(&PluginHttpResponse {
        status,
        body: Some(body),
    })?)
}

/// POST /sql/counter
#[plugin_fn]
pub fn sql_counter(_: String) -> FnResult<String> {
    let res = unsafe {
        db_execute("CREATE TABLE IF NOT EXISTS p_count (id SERIAL)".into())?;
        db_execute("INSERT INTO p_count DEFAULT VALUES".into())?;
        db_query("SELECT COUNT(*) as cnt FROM p_count".into())?
    };
    let rows: Vec<serde_json::Value> = serde_json::from_str(&res)?;
    let count = rows[0]["cnt"].as_i64().unwrap();

    Ok(serde_json::to_string(&PluginHttpResponse {
        status: 200,
        body: Some(serde_json::json!({ "count": count })),
    })?)
}

/// GET /reflect/{id}
/// Used to test path params and query params together
#[plugin_fn]
pub fn reflect(input: String) -> FnResult<String> {
    let req: PluginHttpRequest = serde_json::from_str(&input)?;
    Ok(serde_json::to_string(&PluginHttpResponse {
        status: 200,
        body: Some(serde_json::json!({
            "method": req.method,
            "params": req.params,
            "query": req.query
        })),
    })?)
}
