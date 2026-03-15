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

#[derive(Deserialize)]
struct HostDbResponse {
    pub data: Option<serde_json::Value>,
    pub error: Option<String>,
}

#[host_fn]
extern "ExtismHost" {
    fn store_set(collection: String, key: String, value: String);
    fn store_get(collection: String, key: String) -> String;

    fn db_execute(sql: String, args: String) -> String;
    fn db_query(sql: String, args: String) -> String;
    fn db_transaction(queries_json: String) -> String;
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

/// POST /sql/params
#[plugin_fn]
pub fn sql_parameterized(input: String) -> FnResult<String> {
    let req: PluginHttpRequest = serde_json::from_str(&input)?;
    let body = req.body.unwrap_or_default();
    let name = body["name"].as_str().unwrap_or("unknown");

    unsafe {
        db_execute(
            "CREATE TABLE IF NOT EXISTS p_names (name TEXT)".into(),
            "[]".into(),
        )?;
    }

    // Use parameters to insert
    let args = serde_json::to_string(&vec![name])?;
    unsafe {
        db_execute("INSERT INTO p_names (name) VALUES ($1)".into(), args)?;
    }

    // Use parameters to query
    let query_args = serde_json::to_string(&vec![name])?;
    let res_json = unsafe {
        db_query(
            "SELECT name FROM p_names WHERE name = $1".into(),
            query_args,
        )?
    };
    let res: HostDbResponse = serde_json::from_str(&res_json)?;

    let rows: Vec<serde_json::Value> = serde_json::from_value(res.data.unwrap_or_default())?;

    Ok(serde_json::to_string(&PluginHttpResponse {
        status: 200,
        body: Some(serde_json::json!({ "found": rows.len() })),
    })?)
}

/// POST /sql/transaction
#[plugin_fn]
pub fn sql_transaction(input: String) -> FnResult<String> {
    let req: PluginHttpRequest = serde_json::from_str(&input)?;
    let queries_json = serde_json::to_string(&req.body.unwrap_or_default())?;

    let res_json = unsafe { db_transaction(queries_json)? };
    let res: HostDbResponse = serde_json::from_str(&res_json)?;

    if let Some(err) = res.error {
        return Ok(serde_json::to_string(&PluginHttpResponse {
            status: 400,
            body: Some(serde_json::json!({
                "code": "TRANSACTION_ERROR",
                "message": err
            })),
        })?);
    }

    Ok(serde_json::to_string(&PluginHttpResponse {
        status: 200,
        body: Some(serde_json::json!({
            "results": res.data
        })),
    })?)
}
