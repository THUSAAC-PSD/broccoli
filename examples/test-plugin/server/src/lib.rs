//! Example backend plugin for Broccoli.
//!
//! Built on `broccoli-server-sdk`, which owns the host ABI (function imports
//! and JSON envelopes). Plugins should never hand-roll raw
//! `extern "ExtismHost"` declarations: the SDK keeps them in sync with the
//! server's host functions.
//!
//! Route handlers are declared in `plugin.toml` and wired through
//! `run_api_handler`, which decodes the incoming `PluginHttpRequest`,
//! provides a `Host` handle, and encodes the `PluginHttpResponse`.

#[cfg(target_arch = "wasm32")]
mod plugin {
    use broccoli_server_sdk::prelude::*;
    use extism_pdk::{FnResult, plugin_fn};
    use serde::{Deserialize, Serialize};

    #[derive(Serialize)]
    struct DemoOutput {
        greeting: String,
        visit_count: u32,
    }

    #[derive(Deserialize)]
    struct PostMessageInput {
        name: String,
        message: String,
    }

    #[derive(Serialize)]
    struct PostMessageOutput {
        status: String,
        total_messages: i64,
    }

    fn json_response(body: impl Serialize) -> Result<PluginHttpResponse, ApiError> {
        Ok(PluginHttpResponse {
            status: 200,
            headers: None,
            body: Some(serde_json::to_value(body)?),
        })
    }

    /// `POST /{name}/greet` — greets the caller and counts visits per name
    /// using the plugin key-value store (`storage` permission).
    #[plugin_fn]
    pub fn greet(input: String) -> FnResult<String> {
        run_api_handler(&input, |host, req| {
            let name = req
                .params
                .get("name")
                .cloned()
                .unwrap_or_else(|| "World".to_string());

            host.log.info(&format!("Guest is greeting user: {name}"))?;

            let key = format!("stats:{name}");
            let count: u32 = match host.storage.get_one(&key)? {
                Some(raw) => raw.parse().unwrap_or(0),
                None => 0,
            } + 1;
            host.storage.set(&[(key.as_str(), count.to_string().as_str())])?;

            json_response(DemoOutput {
                greeting: format!("Hello, {name}! This is from Rust Wasm."),
                visit_count: count,
            })
        })
    }

    /// `POST /post-message` — stores a message in a plugin-owned table and
    /// returns how many messages that name has posted (`sql` permission).
    #[plugin_fn]
    pub fn post_message(input: String) -> FnResult<String> {
        run_api_handler(&input, |host, req| {
            let args: PostMessageInput =
                serde_json::from_value(req.body.clone().unwrap_or_default()).map_err(|e| {
                    PluginHttpResponse::error(400, format!("Invalid request body: {e}"))
                })?;

            host.db.execute(
                "CREATE TABLE IF NOT EXISTS plugin_messages ( \
                     id SERIAL PRIMARY KEY, \
                     name TEXT, \
                     message TEXT \
                 )",
            )?;

            // Always bind user input as query parameters via `Params`;
            // never interpolate it into the SQL string.
            let mut p = Params::new();
            let insert_sql = format!(
                "INSERT INTO plugin_messages (name, message) VALUES ({}, {})",
                p.bind(args.name.as_str()),
                p.bind(args.message.as_str()),
            );
            host.db.execute_with_args(&insert_sql, &p.into_args())?;

            host.log.info(&format!("Message posted by {}", args.name))?;

            #[derive(Deserialize)]
            struct CountRow {
                count: i64,
            }
            let mut p = Params::new();
            let count_sql = format!(
                "SELECT COUNT(*)::bigint AS count FROM plugin_messages WHERE name = {}",
                p.bind(args.name.as_str()),
            );
            let total_messages = host
                .db
                .query_one_with_args::<CountRow>(&count_sql, &p.into_args())?
                .map(|row| row.count)
                .unwrap_or(0);

            json_response(PostMessageOutput {
                status: "success".to_string(),
                total_messages,
            })
        })
    }
}
