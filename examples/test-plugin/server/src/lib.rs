use extism_pdk::{FnResult, host_fn, plugin_fn};
use sea_query::*;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct DemoInput {
    name: String,
}

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

#[host_fn]
extern "ExtismHost" {
    fn log_info(msg: String);

    fn store_set(collection: String, key: String, value: String);
    fn store_get(collection: String, key: String) -> String;

    fn db_execute(sql: String) -> u64;
    fn db_query(sql: String) -> String;
}

#[plugin_fn]
pub fn greet(input: String) -> FnResult<String> {
    let args: DemoInput = serde_json::from_str(&input)?;

    unsafe {
        log_info(format!("Guest is greeting user: {}", args.name))?;
    }

    let collection = "stats".to_string();
    let key = args.name.clone();

    let raw_value = unsafe { store_get(collection.clone(), key.clone())? };
    let mut count: u32 = if raw_value == "null" {
        0
    } else {
        serde_json::from_str(&raw_value)?
    };

    count += 1;

    let new_value = serde_json::to_string(&count)?;
    unsafe {
        store_set(collection, key, new_value)?;
    }

    let output = DemoOutput {
        greeting: format!("Hello, {}! This is from Rust Wasm.", args.name),
        visit_count: count,
    };

    Ok(serde_json::to_string(&output)?)
}

#[plugin_fn]
pub fn post_message(input: String) -> FnResult<String> {
    let args: PostMessageInput = serde_json::from_str(&input)?;

    let create_sql = Table::create()
        .table("plugin_messages")
        .if_not_exists()
        .col(
            ColumnDef::new("id")
                .integer()
                .not_null()
                .auto_increment()
                .primary_key(),
        )
        .col(ColumnDef::new("name").text())
        .col(ColumnDef::new("message").text())
        .to_string(PostgresQueryBuilder);
    unsafe {
        db_execute(create_sql)?;
    }

    let insert_sql = Query::insert()
        .into_table("plugin_messages")
        .columns(["name", "message"])
        .values_panic([args.name.clone().into(), args.message.into()])
        .to_string(PostgresQueryBuilder);
    unsafe {
        db_execute(insert_sql)?;
    }

    unsafe {
        log_info(format!("Message posted by {}", args.name))?;
    }

    // Query to count total messages by this user
    // db_query returns a JSON array string like "[{"count": 5}]"
    let select_sql = Query::select()
        .expr(Func::count(Expr::col("name")))
        .from("plugin_messages")
        .and_where(Expr::col("name").eq(args.name))
        .to_string(PostgresQueryBuilder);
    let query_res = unsafe { db_query(select_sql)? };

    // Parse the result set
    let rows: Vec<serde_json::Value> = serde_json::from_str(&query_res)?;
    let count = rows
        .first()
        .and_then(|row| row.get("count"))
        .and_then(|c| c.as_i64())
        .unwrap_or(0);

    let output = PostMessageOutput {
        status: "success".to_string(),
        total_messages: count,
    };

    Ok(serde_json::to_string(&output)?)
}
