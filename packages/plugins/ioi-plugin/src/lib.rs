use extism_pdk::{FnResult, host_fn, plugin_fn};
use serde::{Deserialize, Serialize};

mod mock;
mod models;

use models::*;

// ============================================================================
// Host Functions
// ============================================================================

#[host_fn]
extern "ExtismHost" {
    fn log_info(msg: String);

    fn store_set(collection: String, key: String, value: String);
    fn store_get(collection: String, key: String) -> String;

    // fn db_query(table: String, filter: String) -> String;
    // fn db_update(table: String, filter: String, data: String) -> String;
}

fn db_query(table: String, filter: String) -> Result<String,String>{
    Err("db_query not implemented".to_string())
}
fn db_update(table: String, filter: String, data: String) -> Result<String,String>{
    Err("db_update not implemented".to_string())
}

// ============================================================================
// Database Filter/Update Structures
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbFilter {
    pub field: String,
    pub op: String,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbUpdateResult {
    pub success: bool,
    pub affected_rows: i32,
    pub message: Option<String>,
}

fn build_filter(filters: Vec<DbFilter>) -> String {
    serde_json::to_string(&filters).unwrap_or_else(|_| "[]".to_string())
}

// ============================================================================
// Data Source Abstraction
// All mock fallbacks are isolated here. To remove mock support,
// simply delete the mock module and the fallback branches below.
// ============================================================================

//TODO: use real database queries instead of mocks

mod data_source {
    use super::*;

    pub fn query_problems(contest_id: i32) -> Vec<Problem> {
        let filter = build_filter(vec![DbFilter {
            field: "contest_id".to_string(),
            op: "eq".to_string(),
            value: serde_json::Value::Number(contest_id.into()),
        }]);
        
        match unsafe { db_query("problem".to_string(), filter) } {
            Ok(json) if !json.is_empty() && json != "null" => {
                serde_json::from_str(&json).unwrap_or_else(|_| mock::get_mock_problems(contest_id))
            }
            _ => mock::get_mock_problems(contest_id),
        }
    }

    pub fn query_users(contest_id: i32) -> Vec<User> {
        let filter = build_filter(vec![DbFilter {
            field: "contest_id".to_string(),
            op: "eq".to_string(),
            value: serde_json::Value::Number(contest_id.into()),
        }]);
        
        match unsafe { db_query("user".to_string(), filter) } {
            Ok(json) if !json.is_empty() && json != "null" => {
                serde_json::from_str(&json).unwrap_or_else(|_| mock::get_mock_users(contest_id))
            }
            _ => mock::get_mock_users(contest_id),
        }
    }

    pub fn query_submissions_with_results(contest_id: i32) -> Vec<SubmissionWithResult> {
        let filter = build_filter(vec![DbFilter {
            field: "contest_id".to_string(),
            op: "eq".to_string(),
            value: serde_json::Value::Number(contest_id.into()),
        }]);
        
        match unsafe { db_query("submission_with_result".to_string(), filter) } {
            Ok(json) if !json.is_empty() && json != "null" => {
                serde_json::from_str(&json).unwrap_or_else(|_| mock::get_mock_submissions(contest_id))
            }
            _ => mock::get_mock_submissions(contest_id),
        }
    }

    pub fn query_submission_by_id(submission_id: i32) -> Option<Submission> {
        let filter = build_filter(vec![DbFilter {
            field: "id".to_string(),
            op: "eq".to_string(),
            value: serde_json::Value::Number(submission_id.into()),
        }]);
        
        match unsafe { db_query("submission".to_string(), filter) } {
            Ok(json) if !json.is_empty() && json != "null" => {
                let results: Vec<Submission> = serde_json::from_str(&json)
                    .unwrap_or_else(|_| mock::get_mock_submission_by_id(submission_id));
                results.into_iter().next()
            }
            _ => mock::get_mock_submission_by_id(submission_id).into_iter().next(),
        }
    }

    pub fn query_judge_result_by_submission(submission_id: i32) -> Option<JudgeResult> {
        let filter = build_filter(vec![DbFilter {
            field: "submission_id".to_string(),
            op: "eq".to_string(),
            value: serde_json::Value::Number(submission_id.into()),
        }]);
        
        match unsafe { db_query("judge_result".to_string(), filter) } {
            Ok(json) if !json.is_empty() && json != "null" => {
                let results: Vec<JudgeResult> = serde_json::from_str(&json)
                    .unwrap_or_else(|_| mock::get_mock_judge_result(submission_id));
                results.into_iter().next()
            }
            _ => mock::get_mock_judge_result(submission_id).into_iter().next(),
        }
    }

    pub fn query_test_case_results(judge_result_id: i32) -> Vec<TestCaseResult> {
        let filter = build_filter(vec![DbFilter {
            field: "judge_result_id".to_string(),
            op: "eq".to_string(),
            value: serde_json::Value::Number(judge_result_id.into()),
        }]);
        
        match unsafe { db_query("test_case_result".to_string(), filter) } {
            Ok(json) if !json.is_empty() && json != "null" => {
                serde_json::from_str(&json)
                    .unwrap_or_else(|_| mock::get_mock_test_case_results(judge_result_id))
            }
            _ => mock::get_mock_test_case_results(judge_result_id),
        }
    }

    pub fn query_problem_config(problem_id: i32) -> ProblemIOIConfig {
        let key = format!("problem_{}", problem_id);
        match unsafe { store_get(PROBLEM_CONFIG_COLLECTION.to_string(), key) } {
            Ok(json) if !json.is_empty() && json != "null" => {
                serde_json::from_str(&json)
                    .unwrap_or_else(|_| mock::get_mock_problem_config(problem_id))
            }
            _ => mock::get_mock_problem_config(problem_id),
        }
    }

    pub fn update_judge_result(judge_result: &JudgeResult) -> DbUpdateResult {
        let filter = build_filter(vec![DbFilter {
            field: "id".to_string(),
            op: "eq".to_string(),
            value: serde_json::Value::Number(judge_result.id.into()),
        }]);
        
        let data = serde_json::to_string(judge_result).unwrap_or_else(|_| "{}".to_string());
        
        match unsafe { db_update("judge_result".to_string(), filter, data) } {
            Ok(json) if !json.is_empty() && json != "null" => {
                serde_json::from_str(&json).unwrap_or(DbUpdateResult {
                    success: true,
                    affected_rows: 1,
                    message: Some("Update successful".to_string()),
                })
            }
            _ => {
                // Fallback to mock: apply update to mock state
                mock::apply_judge_result_update(judge_result.clone());
                DbUpdateResult {
                    success: true,
                    affected_rows: 1,
                    message: Some("Mock update applied".to_string()),
                }
            }
        }
    }

    const PROBLEM_CONFIG_COLLECTION: &str = "ioi_problem_config";
}

// ============================================================================
// Storage Helpers
// ============================================================================

const PROBLEM_CONFIG_COLLECTION: &str = "ioi_problem_config";

fn get_problem_config(problem_id: i32) -> ProblemIOIConfig {
    data_source::query_problem_config(problem_id)
}

fn save_problem_config(config: &ProblemIOIConfig) -> Result<(), String> {
    let key = format!("problem_{}", config.problem_id);
    let json = serde_json::to_string(config).map_err(|e| e.to_string())?;
    unsafe { store_set(PROBLEM_CONFIG_COLLECTION.to_string(), key, json) }
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ============================================================================
// IOI Scoring Logic
// ============================================================================

fn calculate_subtask_score(
    test_case_results: &[&TestCaseResult],
    config: &SubtaskConfig,
) -> (i32, String, i32, i32) {
    if test_case_results.is_empty() {
        return (0, "NoData".to_string(), 0, 0);
    }

    let time_used = test_case_results.iter().map(|r| r.time_used).max().unwrap_or(0);
    let memory_used = test_case_results.iter().map(|r| r.memory_used).max().unwrap_or(0);

    let score = match config.scoring_method {
        SubtaskScoringMethod::Sum => {
            test_case_results.iter().map(|r| r.score).sum()
        }
        SubtaskScoringMethod::GroupMin => {
            let all_accepted = test_case_results.iter().all(|r| r.verdict == "Accepted");
            if all_accepted {
                config.max_score
            } else {
                0
            }
        }
        SubtaskScoringMethod::GroupMul => {
            let n = test_case_results.len() as f64;
            let max_per_test = config.max_score as f64 / n;
            
            let product: f64 = test_case_results
                .iter()
                .map(|r| {
                    if max_per_test > 0.0 {
                        (r.score as f64 / max_per_test).min(1.0)
                    } else {
                        0.0
                    }
                })
                .product();
            
            (config.max_score as f64 * product).round() as i32
        }
    };

    let all_accepted = test_case_results.iter().all(|r| r.verdict == "Accepted");
    let verdict = if all_accepted {
        "Accepted".to_string()
    } else if score > 0 {
        "PartiallyCorrect".to_string()
    } else {
        test_case_results
            .iter()
            .find(|r| r.verdict != "Accepted")
            .map(|r| r.verdict.clone())
            .unwrap_or_else(|| "Unknown".to_string())
    };

    (score, verdict, time_used, memory_used)
}

fn compute_subtask_results(
    test_case_results: &[TestCaseResult],
    config: &ProblemIOIConfig,
) -> Vec<SubtaskResult> {
    if !config.subtask_enabled {
        return vec![];
    }

    config
        .subtasks
        .iter()
        .map(|subtask| {
            let tc_results: Vec<&TestCaseResult> = test_case_results
                .iter()
                .filter(|r| subtask.test_case_ids.contains(&r.test_case_id))
                .collect();

            let (score, verdict, time_used, memory_used) =
                calculate_subtask_score(&tc_results, subtask);

            SubtaskResult {
                subtask_id: subtask.id,
                subtask_name: subtask.name.clone(),
                score,
                max_score: subtask.max_score,
                verdict,
                time_used,
                memory_used,
            }
        })
        .collect()
}

fn compute_total_score_from_subtasks(subtask_results: &[SubtaskResult]) -> i32 {
    subtask_results.iter().map(|s| s.score).sum()
}

fn compute_total_score_from_test_cases(
    test_case_results: &[TestCaseResult],
    config: &ProblemIOIConfig,
) -> i32 {
    if config.subtask_enabled && !config.subtasks.is_empty() {
        let subtask_results = compute_subtask_results(test_case_results, config);
        compute_total_score_from_subtasks(&subtask_results)
    } else {
        test_case_results.iter().map(|r| r.score).sum()
    }
}

fn determine_overall_verdict(subtask_results: &[SubtaskResult], total_score: i32, max_score: i32) -> String {
    if subtask_results.is_empty() {
        return "Unknown".to_string();
    }
    
    if total_score >= max_score {
        "Accepted".to_string()
    } else if total_score > 0 {
        "PartiallyCorrect".to_string()
    } else {
        subtask_results
            .iter()
            .find(|s| s.verdict != "Accepted" && s.verdict != "NoData")
            .map(|s| s.verdict.clone())
            .unwrap_or_else(|| "WrongAnswer".to_string())
    }
}

fn calculate_problem_final_score(
    submissions: &[&SubmissionWithResult],
    config: &ProblemIOIConfig,
) -> (i32, Option<Vec<SubtaskBestScore>>) {
    if submissions.is_empty() {
        return (0, None);
    }

    match config.final_score_method {
        FinalScoreMethod::BestSubmission => {
            // Directly read score from judge_result (already calculated and stored)
            let best_score = submissions
                .iter()
                .filter_map(|s| s.result.as_ref())
                .map(|r| r.score)
                .max()
                .unwrap_or(0);
            (best_score, None)
        }
        FinalScoreMethod::BestSubtaskSum => {
            // For BestSubtaskSum, we need to find best score per subtask across all submissions
            // This requires accessing test_case_results which are stored in SubmissionWithResult
            if !config.subtask_enabled || config.subtasks.is_empty() {
                let best_score = submissions
                    .iter()
                    .filter_map(|s| s.result.as_ref())
                    .map(|r| r.score)
                    .max()
                    .unwrap_or(0);
                return (best_score, None);
            }

            let subtask_best_scores: Vec<SubtaskBestScore> = config
                .subtasks
                .iter()
                .map(|subtask| {
                    let best_score = submissions
                        .iter()
                        .map(|sub| {
                            let tc_results: Vec<&TestCaseResult> = sub
                                .test_case_results
                                .iter()
                                .filter(|r| subtask.test_case_ids.contains(&r.test_case_id))
                                .collect();
                            
                            let (score, _, _, _) = calculate_subtask_score(&tc_results, subtask);
                            score
                        })
                        .max()
                        .unwrap_or(0);

                    SubtaskBestScore {
                        subtask_id: subtask.id,
                        subtask_name: subtask.name.clone(),
                        best_score,
                        max_score: subtask.max_score,
                    }
                })
                .collect();

            let total_score: i32 = subtask_best_scores.iter().map(|s| s.best_score).sum();
            (total_score, Some(subtask_best_scores))
        }
    }
}

fn calculate_leaderboard(
    users: Vec<User>,
    problems: Vec<Problem>,
    all_submissions: Vec<SubmissionWithResult>,
) -> Vec<LeaderboardEntry> {
    let mut entries: Vec<LeaderboardEntry> = users
        .into_iter()
        .map(|user| {
            let user_submissions: Vec<&SubmissionWithResult> = all_submissions
                .iter()
                .filter(|s| s.submission.user_id == user.id)
                .collect();

            let problem_scores: Vec<ProblemScore> = problems
                .iter()
                .map(|problem| {
                    let problem_submissions: Vec<&SubmissionWithResult> = user_submissions
                        .iter()
                        .filter(|s| s.submission.problem_id == problem.id)
                        .copied()
                        .collect();

                    let config = get_problem_config(problem.id);
                    let max_score: i32 = if config.subtask_enabled {
                        config.subtasks.iter().map(|s| s.max_score).sum()
                    } else {
                        100
                    };

                    let (score, subtask_scores) =
                        calculate_problem_final_score(&problem_submissions, &config);

                    ProblemScore {
                        problem_id: problem.id,
                        problem_title: problem.title.clone(),
                        score,
                        max_score,
                        submission_count: problem_submissions.len() as i32,
                        subtask_scores,
                    }
                })
                .collect();

            let total_score: i32 = problem_scores.iter().map(|ps| ps.score).sum();

            LeaderboardEntry {
                rank: 0,
                user,
                problem_scores,
                total_score,
            }
        })
        .collect();

    entries.sort_by(|a, b| b.total_score.cmp(&a.total_score));

    let mut current_rank = 1;
    let mut last_score: Option<i32> = None;

    for (i, entry) in entries.iter_mut().enumerate() {
        if let Some(score) = last_score {
            if entry.total_score != score {
                current_rank = i as i32 + 1;
            }
        }
        entry.rank = current_rank;
        last_score = Some(entry.total_score);
    }

    entries
}

// ============================================================================
// Plugin Functions
// ============================================================================

#[plugin_fn]
pub fn get_leaderboard(input: String) -> FnResult<String> {
    let args: GetLeaderboardInput = serde_json::from_str(&input)?;

    unsafe {
        log_info(format!(
            "IOI Plugin: Getting leaderboard for contest {}",
            args.contest_id
        ))?;
    }

    let page = args.page.unwrap_or(1);
    let page_size = args.page_size.unwrap_or(50);
    let contest_id = args.contest_id;

    let problems = data_source::query_problems(contest_id);
    let users = data_source::query_users(contest_id);
    let submissions = data_source::query_submissions_with_results(contest_id);

    let all_entries = calculate_leaderboard(users, problems.clone(), submissions);
    let total_count = all_entries.len() as i32;

    let start = ((page - 1) * page_size) as usize;
    let end = (start + page_size as usize).min(all_entries.len());
    let entries = if start < all_entries.len() {
        all_entries[start..end].to_vec()
    } else {
        vec![]
    };

    let output = GetLeaderboardOutput {
        contest_id,
        problems,
        entries,
        total_count,
        page,
        page_size,
    };

    Ok(serde_json::to_string(&output)?)
}

#[plugin_fn]
pub fn get_submission_detail(input: String) -> FnResult<String> {
    let args: GetSubmissionDetailInput = serde_json::from_str(&input)?;

    unsafe {
        log_info(format!(
            "IOI Plugin: Getting submission detail for {}",
            args.submission_id
        ))?;
    }

    let mut submission = match data_source::query_submission_by_id(args.submission_id) {
        Some(s) => s,
        None => {
            return Ok(serde_json::to_string(&GetSubmissionDetailOutput {
                submission: None,
                judge_result: None,
                test_case_results: vec![],
                subtask_results: vec![],
                problem_config: None,
            })?);
        }
    };

    if !args.include_code.unwrap_or(false) {
        submission.code = String::new();
    }

    let problem_id = submission.problem_id;
    let judge_result = data_source::query_judge_result_by_submission(args.submission_id);

    let test_case_results = match &judge_result {
        Some(jr) => data_source::query_test_case_results(jr.id),
        None => vec![],
    };

    let config = get_problem_config(problem_id);
    let subtask_results = compute_subtask_results(&test_case_results, &config);

    let output = GetSubmissionDetailOutput {
        submission: Some(submission),
        judge_result,
        test_case_results,
        subtask_results,
        problem_config: Some(config),
    };

    Ok(serde_json::to_string(&output)?)
}

#[plugin_fn]
pub fn configure_problem(input: String) -> FnResult<String> {
    let args: ConfigureProblemInput = serde_json::from_str(&input)?;

    unsafe {
        log_info(format!(
            "IOI Plugin: Configuring problem {} (subtask={}, method={:?})",
            args.problem_id, args.subtask_enabled, args.final_score_method
        ))?;
    }

    let config = ProblemIOIConfig {
        problem_id: args.problem_id,
        subtask_enabled: args.subtask_enabled,
        final_score_method: args.final_score_method,
        subtasks: args.subtasks,
    };

    match save_problem_config(&config) {
        Ok(()) => Ok(serde_json::to_string(&ConfigureOutput {
            success: true,
            message: format!("Problem {} configured successfully", args.problem_id),
        })?),
        Err(e) => Ok(serde_json::to_string(&ConfigureOutput {
            success: false,
            message: format!("Failed to configure problem: {}", e),
        })?),
    }
}

#[plugin_fn]
pub fn get_problem_config_api(input: String) -> FnResult<String> {
    let args: GetProblemConfigInput = serde_json::from_str(&input)?;

    unsafe {
        log_info(format!(
            "IOI Plugin: Getting config for problem {}",
            args.problem_id
        ))?;
    }

    let config = get_problem_config(args.problem_id);
    Ok(serde_json::to_string(&config)?)
}

/// Calculate and update submission score
/// TODO: This function should be called after judging completes (via hook or API)
/// It calculates the IOI score based on test case results and writes back to database
#[plugin_fn]
pub fn calculate_submission_score(input: String) -> FnResult<String> {
    let args: CalculateScoreInput = serde_json::from_str(&input)?;

    unsafe {
        log_info(format!(
            "IOI Plugin: Calculating score for submission {}",
            args.submission_id
        ))?;
    }

    let submission = match data_source::query_submission_by_id(args.submission_id) {
        Some(s) => s,
        None => {
            return Ok(serde_json::to_string(&CalculateScoreOutput {
                success: false,
                submission_id: args.submission_id,
                score: 0,
                verdict: "NotFound".to_string(),
                subtask_results: vec![],
                message: "Submission not found".to_string(),
            })?);
        }
    };

    let judge_result = match data_source::query_judge_result_by_submission(args.submission_id) {
        Some(jr) => jr,
        None => {
            return Ok(serde_json::to_string(&CalculateScoreOutput {
                success: false,
                submission_id: args.submission_id,
                score: 0,
                verdict: "NotJudged".to_string(),
                subtask_results: vec![],
                message: "Judge result not found".to_string(),
            })?);
        }
    };

    let test_case_results = data_source::query_test_case_results(judge_result.id);
    let config = get_problem_config(submission.problem_id);

    let subtask_results = compute_subtask_results(&test_case_results, &config);
    
    let total_score = compute_total_score_from_test_cases(&test_case_results, &config);
    
    let max_score: i32 = if config.subtask_enabled {
        config.subtasks.iter().map(|s| s.max_score).sum()
    } else {
        100
    };
    
    let verdict = determine_overall_verdict(&subtask_results, total_score, max_score);

    let time_used = test_case_results.iter().map(|r| r.time_used).max().unwrap_or(0);
    let memory_used = test_case_results.iter().map(|r| r.memory_used).max().unwrap_or(0);

    // Build updated JudgeResult
    let updated_judge_result = JudgeResult {
        id: judge_result.id,
        verdict: verdict.clone(),
        score: total_score,
        time_used,
        memory_used,
        submission_id: judge_result.submission_id,
        created_at: judge_result.created_at.clone(),
    };

    let update_result = data_source::update_judge_result(&updated_judge_result);

    unsafe {
        log_info(format!(
            "IOI Plugin: Updated judge_result {} with score={}, verdict={}",
            judge_result.id, total_score, verdict
        ))?;
    }

    Ok(serde_json::to_string(&CalculateScoreOutput {
        success: update_result.success,
        submission_id: args.submission_id,
        score: total_score,
        verdict,
        subtask_results,
        message: update_result.message.unwrap_or_else(|| "Score calculated and saved".to_string()),
    })?)
}
