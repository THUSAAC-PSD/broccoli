use crate::entity::{plugin_config, problem, test_case};
use crate::registry::{BatchState, EvaluateBatches, EvaluatorRegistry};
use common::submission_dispatch::{
    SdkVerdict, SourceFile, StartEvaluateBatchInput, TestCaseVerdict,
};
use extism::{Function, UserData, Val, ValType};
use plugin_core::traits::PluginManager;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Input for get_next_evaluate_result
#[derive(Deserialize)]
struct GetNextEvaluateResultInput {
    batch_id: String,
    timeout_ms: u64,
}

/// Input for cancel_evaluate_batch
#[derive(Deserialize)]
struct CancelEvaluateBatchInput {
    batch_id: String,
}

/// Named context for evaluate host functions.
struct EvaluateContext {
    plugin_id: String,
    plugin_manager: Arc<dyn PluginManager>,
    evaluator_registry: EvaluatorRegistry,
    evaluate_batches: EvaluateBatches,
    db: DatabaseConnection,
}

type EvaluateUserData = EvaluateContext;

pub fn create_evaluate_functions(
    plugin_id: String,
    plugin_manager: Arc<dyn PluginManager>,
    evaluator_registry: EvaluatorRegistry,
    evaluate_batches: EvaluateBatches,
    db: DatabaseConnection,
) -> Vec<Function> {
    let user_data: UserData<EvaluateUserData> = UserData::new(EvaluateContext {
        plugin_id,
        plugin_manager,
        evaluator_registry,
        evaluate_batches,
        db,
    });

    vec![
        Function::new(
            "start_evaluate_batch",
            [ValType::I64],
            [ValType::I64],
            user_data.clone(),
            start_evaluate_batch_fn,
        ),
        Function::new(
            "get_next_evaluate_result",
            [ValType::I64],
            [ValType::I64],
            user_data.clone(),
            get_next_evaluate_result_fn,
        ),
        Function::new(
            "cancel_evaluate_batch",
            [ValType::I64],
            [ValType::I64],
            user_data,
            cancel_evaluate_batch_fn,
        ),
    ]
}

fn start_evaluate_batch_fn(
    plugin: &mut extism::CurrentPlugin,
    inputs: &[Val],
    outputs: &mut [Val],
    user_data: UserData<EvaluateUserData>,
) -> Result<(), extism::Error> {
    let input_bytes: Vec<u8> = plugin.memory_get_val(&inputs[0])?;
    let input: StartEvaluateBatchInput = serde_json::from_slice(&input_bytes)
        .map_err(|e| extism::Error::msg(format!("Failed to deserialize input: {}", e)))?;

    let (caller_plugin_id, plugin_manager, evaluator_registry, evaluate_batches, db) = {
        let user_data_guard = user_data.get()?;
        let guard = user_data_guard
            .lock()
            .map_err(|_| extism::Error::msg("Lock poisoned"))?;
        (
            guard.plugin_id.clone(),
            guard.plugin_manager.clone(),
            guard.evaluator_registry.clone(),
            guard.evaluate_batches.clone(),
            guard.db.clone(),
        )
    };

    let evaluator = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            let registry = evaluator_registry.read().await;
            registry.get(&input.problem_type).cloned()
        })
    });

    let evaluator = evaluator.ok_or_else(|| {
        extism::Error::msg(format!(
            "No evaluator registered for problem type: {}",
            input.problem_type
        ))
    })?;

    let mut input = input;
    if !input.test_cases.is_empty() {
        let tc_ids: Vec<i32> = input.test_cases.iter().map(|tc| tc.test_case_id).collect();
        let problem_id = input.test_cases[0].problem_id;

        if input
            .test_cases
            .iter()
            .any(|tc| tc.problem_id != problem_id)
        {
            return Err(extism::Error::msg(
                "All test cases in a batch must belong to the same problem",
            ));
        }

        let (tc_data_map, problem_model, checker_config_model) =
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    // Get test case input + expected_output
                    let tc_models = test_case::Entity::find()
                        .filter(test_case::Column::Id.is_in(tc_ids))
                        .all(&db)
                        .await
                        .map_err(|e| {
                            extism::Error::msg(format!("Failed to query test case data: {}", e))
                        })?;

                    let tc_data: HashMap<i32, (String, String)> = tc_models
                        .into_iter()
                        .map(|m| (m.id, (m.input, m.expected_output)))
                        .collect();

                    // Get problem checker info
                    let problem_model = problem::Entity::find_by_id(problem_id)
                        .one(&db)
                        .await
                        .map_err(|e| {
                            extism::Error::msg(format!("Failed to query problem: {}", e))
                        })?;

                    // Get checker config
                    let checker_config = plugin_config::Entity::find_by_id((
                        "problem".to_string(),
                        problem_id.to_string(),
                        "checker".to_string(),
                    ))
                    .one(&db)
                    .await
                    .map_err(|e| {
                        extism::Error::msg(format!("Failed to query checker config: {}", e))
                    })?;

                    Ok::<_, extism::Error>((tc_data, problem_model, checker_config))
                })
            })?;

        let problem_model = problem_model
            .ok_or_else(|| extism::Error::msg(format!("Problem {} not found", problem_id)))?;

        let checker_format = Some(problem_model.checker_format.clone());
        let parsed_checker_source: Option<Vec<SourceFile>> =
            problem_model.checker_source.as_ref().and_then(|v| {
                match serde_json::from_value::<Vec<SourceFile>>(v.clone()) {
                    Ok(parsed) => Some(parsed),
                    Err(e) => {
                        tracing::warn!(
                            problem_id,
                            error = %e,
                            "Failed to parse checker_source JSON"
                        );
                        None
                    }
                }
            });

        let checker_config_value: Option<serde_json::Value> =
            checker_config_model.map(|pc| pc.config);

        for tc in &mut input.test_cases {
            let (test_input, expected_output) =
                tc_data_map.get(&tc.test_case_id).ok_or_else(|| {
                    extism::Error::msg(format!(
                        "Test case {} not found in database",
                        tc.test_case_id
                    ))
                })?;
            tc.test_input = test_input.clone();
            tc.expected_output = expected_output.clone();
            tc.checker_format = checker_format.clone();
            tc.checker_config = checker_config_value.clone();
            tc.checker_source = parsed_checker_source.clone();
        }
    }

    let test_case_count = input.test_cases.len();
    let batch_id = Uuid::new_v4().to_string();

    let (batch_tx, batch_rx) = crossbeam::channel::unbounded();
    let pending_count = Arc::new(AtomicUsize::new(test_case_count));

    evaluate_batches.insert(
        batch_id.clone(),
        BatchState {
            result_rx: batch_rx,
            pending_count: pending_count.clone(),
            created_at: Instant::now(),
        },
    );

    tracing::info!(
        caller = %caller_plugin_id,
        batch_id = %batch_id,
        problem_type = %input.problem_type,
        test_case_count = test_case_count,
        evaluator_plugin = %evaluator.plugin_id,
        evaluator_fn = %evaluator.function_name,
        "Starting evaluate batch"
    );

    for tc_input in input.test_cases {
        let pm = plugin_manager.clone();
        let eval_plugin_id = evaluator.plugin_id.clone();
        let eval_fn_name = evaluator.function_name.clone();
        let batch_tx = batch_tx.clone();
        let pending = pending_count.clone();
        let tc_id = tc_input.test_case_id;

        tokio::spawn(async move {
            let input_bytes = match serde_json::to_vec(&tc_input) {
                Ok(b) => b,
                Err(e) => {
                    let _ = batch_tx.send(TestCaseVerdict {
                        test_case_id: tc_id,
                        verdict: SdkVerdict::SystemError,
                        score: 0.0,
                        time_used_ms: None,
                        memory_used_kb: None,
                        message: Some(format!("Failed to serialize evaluator input: {}", e)),
                    });
                    pending.fetch_sub(1, Ordering::SeqCst);
                    return;
                }
            };

            match pm
                .call_raw(&eval_plugin_id, &eval_fn_name, input_bytes)
                .await
            {
                Ok(result_bytes) => {
                    match serde_json::from_slice::<TestCaseVerdict>(&result_bytes) {
                        Ok(verdict) => {
                            let _ = batch_tx.send(verdict);
                        }
                        Err(e) => {
                            let _ = batch_tx.send(TestCaseVerdict {
                                test_case_id: tc_id,
                                verdict: SdkVerdict::SystemError,
                                score: 0.0,
                                time_used_ms: None,
                                memory_used_kb: None,
                                message: Some(format!(
                                    "Failed to deserialize evaluator result: {}",
                                    e
                                )),
                            });
                        }
                    }
                }
                Err(e) => {
                    let _ = batch_tx.send(TestCaseVerdict {
                        test_case_id: tc_id,
                        verdict: SdkVerdict::SystemError,
                        score: 0.0,
                        time_used_ms: None,
                        memory_used_kb: None,
                        message: Some(format!("Evaluator call failed: {}", e)),
                    });
                }
            }
            pending.fetch_sub(1, Ordering::SeqCst);
        });
    }

    let response = serde_json::json!({ "batch_id": batch_id });
    let output_bytes = serde_json::to_vec(&response)
        .map_err(|e| extism::Error::msg(format!("Failed to serialize batch_id: {}", e)))?;
    let offset = plugin.memory_new(&output_bytes)?;
    outputs[0] = Val::I64(offset.offset() as i64);

    Ok(())
}

fn get_next_evaluate_result_fn(
    plugin: &mut extism::CurrentPlugin,
    inputs: &[Val],
    outputs: &mut [Val],
    user_data: UserData<EvaluateUserData>,
) -> Result<(), extism::Error> {
    let input_bytes: Vec<u8> = plugin.memory_get_val(&inputs[0])?;
    let input: GetNextEvaluateResultInput = serde_json::from_slice(&input_bytes)
        .map_err(|e| extism::Error::msg(format!("Failed to deserialize input: {}", e)))?;

    let (plugin_id, batches) = {
        let user_data_guard = user_data.get()?;
        let guard = user_data_guard
            .lock()
            .map_err(|_| extism::Error::msg("Lock poisoned"))?;
        (guard.plugin_id.clone(), guard.evaluate_batches.clone())
    };

    let (result_rx, pending_count) = {
        let batch = batches
            .get(&input.batch_id)
            .ok_or_else(|| extism::Error::msg(format!("Batch not found: {}", input.batch_id)))?;
        (batch.result_rx.clone(), batch.pending_count.clone())
    };

    let result = result_rx.recv_timeout(Duration::from_millis(input.timeout_ms));

    match result {
        Ok(verdict) => {
            tracing::debug!(
                plugin_id = %plugin_id,
                batch_id = %input.batch_id,
                test_case_id = verdict.test_case_id,
                verdict = %verdict.verdict,
                "Evaluate result received"
            );

            if pending_count.load(Ordering::SeqCst) == 0 && result_rx.is_empty() {
                batches.remove(&input.batch_id);
            }

            let response = serde_json::json!({ "result": verdict });
            let output_bytes = serde_json::to_vec(&response)
                .map_err(|e| extism::Error::msg(format!("Failed to serialize result: {}", e)))?;
            let offset = plugin.memory_new(&output_bytes)?;
            outputs[0] = Val::I64(offset.offset() as i64);
            Ok(())
        }
        Err(crossbeam::channel::RecvTimeoutError::Timeout) => {
            let response = serde_json::json!({ "result": null });
            let output_bytes = serde_json::to_vec(&response)
                .map_err(|e| extism::Error::msg(format!("Failed to serialize result: {}", e)))?;
            let offset = plugin.memory_new(&output_bytes)?;
            outputs[0] = Val::I64(offset.offset() as i64);
            Ok(())
        }
        Err(crossbeam::channel::RecvTimeoutError::Disconnected) => {
            Err(extism::Error::msg("Evaluate batch channel disconnected"))
        }
    }
}

fn cancel_evaluate_batch_fn(
    plugin: &mut extism::CurrentPlugin,
    inputs: &[Val],
    outputs: &mut [Val],
    user_data: UserData<EvaluateUserData>,
) -> Result<(), extism::Error> {
    let input_bytes: Vec<u8> = plugin.memory_get_val(&inputs[0])?;
    let input: CancelEvaluateBatchInput = serde_json::from_slice(&input_bytes)
        .map_err(|e| extism::Error::msg(format!("Failed to deserialize input: {}", e)))?;

    let (plugin_id, batches) = {
        let user_data_guard = user_data.get()?;
        let guard = user_data_guard
            .lock()
            .map_err(|_| extism::Error::msg("Lock poisoned"))?;
        (guard.plugin_id.clone(), guard.evaluate_batches.clone())
    };

    batches.remove(&input.batch_id);

    tracing::info!(
        plugin_id = %plugin_id,
        batch_id = %input.batch_id,
        "Evaluate batch cancelled"
    );

    let output = serde_json::json!({});
    let output_bytes = serde_json::to_vec(&output)
        .map_err(|e| extism::Error::msg(format!("Failed to serialize output: {}", e)))?;
    let offset = plugin.memory_new(&output_bytes)?;
    outputs[0] = Val::I64(offset.offset() as i64);

    Ok(())
}
