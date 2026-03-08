use crate::entity::plugin_config;
use chrono::Utc;
use extism::{Function, UserData, Val, ValType};
use sea_orm::{DatabaseConnection, EntityTrait, Set, sea_query::OnConflict};
use serde::Deserialize;

type ConfigUserData = (String, DatabaseConnection);

const VALID_SCOPES: &[&str] = &["problem", "contest_problem", "contest", "plugin"];

/// Validate the raw namespace provided by the plugin, BEFORE auto-prefixing.
/// Rejects `:` so plugins cannot craft ambiguous composite namespaces.
fn validate_raw_namespace(namespace: &str) -> Result<(), extism::Error> {
    if namespace.is_empty() || namespace.len() > 128 {
        return Err(extism::Error::msg("namespace must be 1-128 characters"));
    }
    if !namespace
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(extism::Error::msg(
            "namespace must contain only alphanumeric, hyphen, or underscore characters",
        ));
    }
    Ok(())
}

/// Validate the resolved (possibly prefixed) config input.
fn validate_config_input(scope: &str, ref_id: &str, namespace: &str) -> Result<(), extism::Error> {
    if !VALID_SCOPES.contains(&scope) {
        return Err(extism::Error::msg(format!(
            "Invalid scope '{}': must be one of {}",
            scope,
            VALID_SCOPES.join(", ")
        )));
    }
    if ref_id.is_empty() {
        return Err(extism::Error::msg("ref_id must not be empty"));
    }
    if namespace.is_empty() || namespace.len() > 256 {
        return Err(extism::Error::msg("namespace must be 1-256 characters"));
    }
    // After resolve_namespace, colons appear in auto-prefixed namespaces (e.g. "plugin_id:ns")
    if !namespace
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == ':')
    {
        return Err(extism::Error::msg(
            "namespace must contain only alphanumeric, hyphen, underscore, or colon characters",
        ));
    }
    Ok(())
}

/// For non-plugin scopes, prefix the namespace with `{plugin_id}:` to prevent
/// cross-plugin collisions in shared resource configs.
fn resolve_namespace(scope: &str, plugin_id: &str, namespace: String) -> String {
    if scope == "plugin" {
        namespace
    } else {
        format!("{}:{}", plugin_id, namespace)
    }
}

#[derive(Deserialize)]
struct ConfigGetInput {
    scope: String,
    ref_id: String,
    namespace: String,
}

#[derive(Deserialize)]
struct ConfigSetInput {
    scope: String,
    ref_id: String,
    namespace: String,
    config: serde_json::Value,
}

pub fn create_config_get_function(plugin_id: String, db: DatabaseConnection) -> Function {
    Function::new(
        "config_get",
        [ValType::I64],
        [ValType::I64],
        UserData::new((plugin_id, db)),
        config_get_fn,
    )
}

pub fn create_config_set_function(plugin_id: String, db: DatabaseConnection) -> Function {
    Function::new(
        "config_set",
        [ValType::I64],
        [ValType::I64],
        UserData::new((plugin_id, db)),
        config_set_fn,
    )
}

fn config_get_fn(
    plugin: &mut extism::CurrentPlugin,
    inputs: &[Val],
    outputs: &mut [Val],
    user_data: UserData<ConfigUserData>,
) -> Result<(), extism::Error> {
    let input_bytes: Vec<u8> = plugin.memory_get_val(&inputs[0])?;
    let input: ConfigGetInput = serde_json::from_slice(&input_bytes).map_err(|e| {
        extism::Error::msg(format!("Failed to deserialize config_get input: {}", e))
    })?;

    let (plugin_id, db) = {
        let guard = user_data.get()?;
        let data = guard
            .lock()
            .map_err(|_| extism::Error::msg("Lock poisoned"))?;
        data.clone()
    };

    let ref_id = if input.scope == "plugin" {
        plugin_id.clone()
    } else {
        input.ref_id
    };

    validate_raw_namespace(&input.namespace)?;
    let namespace = resolve_namespace(&input.scope, &plugin_id, input.namespace);
    validate_config_input(&input.scope, &ref_id, &namespace)?;

    let result = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            plugin_config::Entity::find_by_id((input.scope, ref_id, namespace))
                .one(&db)
                .await
        })
    })
    .map_err(|e| extism::Error::msg(format!("DB error in config_get: {}", e)))?;

    let output_value = match result {
        Some(row) => serde_json::json!({ "config": row.config }),
        None => serde_json::Value::Null,
    };

    let output_bytes = serde_json::to_vec(&output_value)
        .map_err(|e| extism::Error::msg(format!("Failed to serialize config_get output: {}", e)))?;
    let offset = plugin.memory_new(&output_bytes)?;
    outputs[0] = Val::I64(offset.offset() as i64);

    Ok(())
}

fn config_set_fn(
    plugin: &mut extism::CurrentPlugin,
    inputs: &[Val],
    outputs: &mut [Val],
    user_data: UserData<ConfigUserData>,
) -> Result<(), extism::Error> {
    let input_bytes: Vec<u8> = plugin.memory_get_val(&inputs[0])?;
    let input: ConfigSetInput = serde_json::from_slice(&input_bytes).map_err(|e| {
        extism::Error::msg(format!("Failed to deserialize config_set input: {}", e))
    })?;

    let (plugin_id, db) = {
        let guard = user_data.get()?;
        let data = guard
            .lock()
            .map_err(|_| extism::Error::msg("Lock poisoned"))?;
        data.clone()
    };

    let ref_id = if input.scope == "plugin" {
        plugin_id.clone()
    } else {
        input.ref_id
    };

    validate_raw_namespace(&input.namespace)?;
    let namespace = resolve_namespace(&input.scope, &plugin_id, input.namespace);
    validate_config_input(&input.scope, &ref_id, &namespace)?;

    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            let active = plugin_config::ActiveModel {
                scope: Set(input.scope),
                ref_id: Set(ref_id),
                namespace: Set(namespace),
                config: Set(input.config),
                updated_at: Set(Utc::now()),
            };

            plugin_config::Entity::insert(active)
                .on_conflict(
                    OnConflict::columns([
                        plugin_config::Column::Scope,
                        plugin_config::Column::RefId,
                        plugin_config::Column::Namespace,
                    ])
                    .update_columns([
                        plugin_config::Column::Config,
                        plugin_config::Column::UpdatedAt,
                    ])
                    .to_owned(),
                )
                .exec(&db)
                .await?;

            Ok::<_, sea_orm::DbErr>(())
        })
    })
    .map_err(|e| extism::Error::msg(format!("DB error in config_set: {}", e)))?;

    let output_bytes = serde_json::to_vec(&serde_json::json!({}))
        .map_err(|e| extism::Error::msg(format!("Failed to serialize config_set output: {}", e)))?;
    let offset = plugin.memory_new(&output_bytes)?;
    outputs[0] = Val::I64(offset.offset() as i64);

    Ok(())
}
