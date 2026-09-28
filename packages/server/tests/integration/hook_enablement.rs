//! Which plugins' resource-scoped hooks run for a submission
//! (`hooks::resource_enablements`): config-enabled plugins everywhere, plus
//! the plugin that owns the contest's type for contest submissions only.

use crate::common::TestApp;
use sea_orm::{ActiveModelTrait, Set};
use server::entity::plugin_config;
use server::hooks::{HookContest, resource_enablements};
use server::registry::{ContestTypeHandlers, ContestTypeRegistry};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

fn registry_owning(contest_type: &str, plugin_id: &str) -> ContestTypeRegistry {
    let mut map = HashMap::new();
    map.insert(
        contest_type.to_string(),
        ContestTypeHandlers {
            plugin_id: plugin_id.to_string(),
            submission_fn: "handle_submission".into(),
            code_run_fn: "handle_code_run".into(),
        },
    );
    Arc::new(RwLock::new(map))
}

async fn config_row(app: &TestApp, scope: &str, ref_id: &str, plugin: &str, enabled: bool) {
    plugin_config::ActiveModel {
        scope: Set(scope.to_string()),
        ref_id: Set(ref_id.to_string()),
        namespace: Set(format!("{plugin}:before_submission")),
        config: Set(serde_json::json!({})),
        enabled: Set(Some(enabled)),
        position: Set(3),
        ..Default::default()
    }
    .insert(&app.db)
    .await
    .expect("insert plugin_config row");
}

#[tokio::test]
async fn the_contest_type_owner_runs_for_its_own_contests_only() {
    let app = TestApp::spawn().await;
    let types = registry_owning("fmt-a", "format-a");
    let (problem, contest) = (41, 42);

    let own = resource_enablements(
        &app.db,
        &types,
        problem,
        Some(HookContest { id: contest, contest_type: "fmt-a" }),
    )
    .await
    .unwrap();
    assert!(own.contains_key("format-a"), "owner must run with no config: {own:?}");

    let other = resource_enablements(
        &app.db,
        &types,
        problem,
        Some(HookContest { id: contest, contest_type: "fmt-b" }),
    )
    .await
    .unwrap();
    assert!(!other.contains_key("format-a"), "not another format's contest: {other:?}");

    let practice = resource_enablements(&app.db, &types, problem, None).await.unwrap();
    assert!(!practice.contains_key("format-a"), "not practice: {practice:?}");
}

#[tokio::test]
async fn opt_in_plugins_still_need_config_and_the_owner_cannot_be_switched_off() {
    let app = TestApp::spawn().await;
    let types = registry_owning("fmt-a", "format-a");
    let (problem, contest) = (51, 52);
    config_row(&app, "contest", &contest.to_string(), "cooldown", true).await;
    config_row(&app, "contest", &contest.to_string(), "format-a", false).await;

    let enabled = resource_enablements(
        &app.db,
        &types,
        problem,
        Some(HookContest { id: contest, contest_type: "fmt-a" }),
    )
    .await
    .unwrap();
    assert_eq!(enabled.get("cooldown"), Some(&3), "opt-in plugin keeps its config");
    assert!(
        enabled.contains_key("format-a"),
        "a format's own rules are not optional for its contests: {enabled:?}"
    );

    let unconfigured = resource_enablements(
        &app.db,
        &types,
        problem,
        Some(HookContest { id: contest + 1, contest_type: "fmt-a" }),
    )
    .await
    .unwrap();
    assert!(!unconfigured.contains_key("cooldown"), "{unconfigured:?}");
}
