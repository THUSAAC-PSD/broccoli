//! Mock data module
//! Provides test data and simulates database operations when backend is not available.
//! Uses thread_local RefCell to support mutable state for update operations.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::models::*;

// ============================================================================
// Mock Database State
// ============================================================================

thread_local! {
    /// Stores updated judge_result data: judge_result_id -> JudgeResult
    static JUDGE_RESULT_UPDATES: RefCell<HashMap<i32, JudgeResult>> = RefCell::new(HashMap::new());
    
    /// Stores subtask results for each submission: submission_id -> Vec<SubtaskResult>
    static SUBTASK_RESULTS_CACHE: RefCell<HashMap<i32, Vec<SubtaskResult>>> = RefCell::new(HashMap::new());
}

/// Apply an update to a judge_result (mock implementation)
/// Uses the full JudgeResult struct for consistency with the database schema
pub fn apply_judge_result_update(judge_result: JudgeResult) {
    JUDGE_RESULT_UPDATES.with(|updates| {
        updates.borrow_mut().insert(judge_result.id, judge_result);
    });
}

/// Store subtask results for a submission
pub fn store_subtask_results(submission_id: i32, results: Vec<SubtaskResult>) {
    SUBTASK_RESULTS_CACHE.with(|cache| {
        cache.borrow_mut().insert(submission_id, results);
    });
}

/// Get stored subtask results for a submission
pub fn get_stored_subtask_results(submission_id: i32) -> Option<Vec<SubtaskResult>> {
    SUBTASK_RESULTS_CACHE.with(|cache| {
        cache.borrow().get(&submission_id).cloned()
    })
}

/// Reset all mock state (useful for testing)
pub fn reset_mock_state() {
    JUDGE_RESULT_UPDATES.with(|updates| {
        updates.borrow_mut().clear();
    });
    SUBTASK_RESULTS_CACHE.with(|cache| {
        cache.borrow_mut().clear();
    });
}

// ============================================================================
// Mock Data Generators
// ============================================================================

pub fn get_mock_problems(_contest_id: i32) -> Vec<Problem> {
    vec![
        Problem {
            id: 1,
            title: "Problem A - Sum".to_string(),
            content: "Calculate the sum of two integers.".to_string(),
            time_limit: 1000,
            memory_limit: 262144,
            created_at: "2024-01-01T00:00:00Z".to_string(),
        },
        Problem {
            id: 2,
            title: "Problem B - Graph".to_string(),
            content: "Find the shortest path in a graph.".to_string(),
            time_limit: 2000,
            memory_limit: 524288,
            created_at: "2024-01-01T00:00:00Z".to_string(),
        },
        Problem {
            id: 3,
            title: "Problem C - Tree".to_string(),
            content: "Calculate tree DP.".to_string(),
            time_limit: 3000,
            memory_limit: 524288,
            created_at: "2024-01-01T00:00:00Z".to_string(),
        },
    ]
}

pub fn get_mock_users(_contest_id: i32) -> Vec<User> {
    vec![
        User {
            id: 1,
            username: "alice".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
        },
        User {
            id: 2,
            username: "bob".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
        },
        User {
            id: 3,
            username: "charlie".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
        },
    ]
}

/// Get mock submissions with judge results (applies any updates from mock state)
pub fn get_mock_submissions(_contest_id: i32) -> Vec<SubmissionWithResult> {
    let base_submissions = get_base_submissions();
    
    JUDGE_RESULT_UPDATES.with(|updates| {
        let updates = updates.borrow();
        base_submissions.into_iter().map(|mut sub| {
            if let Some(ref mut result) = sub.result {
                if let Some(updated) = updates.get(&result.id) {
                    result.score = updated.score;
                    result.verdict = updated.verdict.clone();
                    result.time_used = updated.time_used;
                    result.memory_used = updated.memory_used;
                }
            }
            sub
        }).collect()
    })
}

pub fn get_mock_submission_by_id(submission_id: i32) -> Vec<Submission> {
    get_base_submissions()
        .into_iter()
        .filter(|s| s.submission.id == submission_id)
        .map(|s| {
            let mut sub = s.submission;
            sub.code = r#"#include <iostream>
using namespace std;

int main() {
    int a, b;
    cin >> a >> b;
    cout << a + b << endl;
    return 0;
}
"#.to_string();
            sub
        })
        .collect()
}

pub fn get_mock_judge_result(submission_id: i32) -> Vec<JudgeResult> {
    let base = get_base_submissions();
    
    JUDGE_RESULT_UPDATES.with(|updates| {
        let updates = updates.borrow();
        base.into_iter()
            .filter(|s| s.submission.id == submission_id)
            .filter_map(|s| s.result)
            .map(|mut jr| {
                if let Some(updated) = updates.get(&jr.id) {
                    jr.score = updated.score;
                    jr.verdict = updated.verdict.clone();
                    jr.time_used = updated.time_used;
                    jr.memory_used = updated.memory_used;
                }
                jr
            })
            .collect()
    })
}

pub fn get_mock_test_case_results(judge_result_id: i32) -> Vec<TestCaseResult> {
    get_base_submissions()
        .into_iter()
        .filter(|s| s.result.as_ref().map(|r| r.id) == Some(judge_result_id))
        .flat_map(|s| s.test_case_results)
        .collect()
}

pub fn get_mock_problem_config(problem_id: i32) -> ProblemIOIConfig {
    match problem_id {
        // Problem 1: Sum scoring method - partial scores accumulate
        1 => ProblemIOIConfig {
            problem_id: 1,
            subtask_enabled: true,
            final_score_method: FinalScoreMethod::BestSubmission,
            subtasks: vec![
                SubtaskConfig {
                    id: 1,
                    name: "Subtask 1 - Small (N ≤ 100)".to_string(),
                    max_score: 30,
                    scoring_method: SubtaskScoringMethod::Sum,
                    test_case_ids: vec![1, 2, 3],
                },
                SubtaskConfig {
                    id: 2,
                    name: "Subtask 2 - Medium (N ≤ 10000)".to_string(),
                    max_score: 30,
                    scoring_method: SubtaskScoringMethod::GroupMin,
                    test_case_ids: vec![4, 5],
                },
                SubtaskConfig {
                    id: 3,
                    name: "Subtask 3 - Large (N ≤ 10^6)".to_string(),
                    max_score: 40,
                    scoring_method: SubtaskScoringMethod::GroupMin,
                    test_case_ids: vec![6, 7],
                },
            ],
        },
        // Problem 2: All GroupMin - typical IOI style (all-or-nothing per subtask)
        2 => ProblemIOIConfig {
            problem_id: 2,
            subtask_enabled: true,
            final_score_method: FinalScoreMethod::BestSubmission,
            subtasks: vec![
                SubtaskConfig {
                    id: 1,
                    name: "Subtask 1 - Examples".to_string(),
                    max_score: 10,
                    scoring_method: SubtaskScoringMethod::GroupMin,
                    test_case_ids: vec![1, 2],
                },
                SubtaskConfig {
                    id: 2,
                    name: "Subtask 2 - Small".to_string(),
                    max_score: 20,
                    scoring_method: SubtaskScoringMethod::GroupMin,
                    test_case_ids: vec![3, 4, 5],
                },
                SubtaskConfig {
                    id: 3,
                    name: "Subtask 3 - Full".to_string(),
                    max_score: 70,
                    scoring_method: SubtaskScoringMethod::GroupMin,
                    test_case_ids: vec![6, 7],
                },
            ],
        },
        // Problem 3: BestSubtaskSum mode - IOI 2010-2016 style
        3 => ProblemIOIConfig {
            problem_id: 3,
            subtask_enabled: true,
            final_score_method: FinalScoreMethod::BestSubtaskSum,
            subtasks: vec![
                SubtaskConfig {
                    id: 1,
                    name: "Subtask 1".to_string(),
                    max_score: 20,
                    scoring_method: SubtaskScoringMethod::GroupMin,
                    test_case_ids: vec![1, 2, 3],
                },
                SubtaskConfig {
                    id: 2,
                    name: "Subtask 2".to_string(),
                    max_score: 30,
                    scoring_method: SubtaskScoringMethod::GroupMin,
                    test_case_ids: vec![4, 5],
                },
                SubtaskConfig {
                    id: 3,
                    name: "Subtask 3".to_string(),
                    max_score: 50,
                    scoring_method: SubtaskScoringMethod::GroupMin,
                    test_case_ids: vec![6, 7],
                },
            ],
        },
        // Problem 4: GroupMul scoring - partial credit within subtask
        4 => ProblemIOIConfig {
            problem_id: 4,
            subtask_enabled: true,
            final_score_method: FinalScoreMethod::BestSubmission,
            subtasks: vec![
                SubtaskConfig {
                    id: 1,
                    name: "Subtask 1 - Basic".to_string(),
                    max_score: 40,
                    scoring_method: SubtaskScoringMethod::GroupMul,
                    test_case_ids: vec![1, 2, 3, 4],
                },
                SubtaskConfig {
                    id: 2,
                    name: "Subtask 2 - Advanced".to_string(),
                    max_score: 60,
                    scoring_method: SubtaskScoringMethod::GroupMul,
                    test_case_ids: vec![5, 6, 7],
                },
            ],
        },
        _ => ProblemIOIConfig {
            problem_id,
            subtask_enabled: false,
            final_score_method: FinalScoreMethod::BestSubmission,
            subtasks: vec![],
        },
    }
}

// ============================================================================
// Base Data (initial state before any updates)
// Score is 0 initially, simulating freshly judged submissions awaiting score calculation
// ============================================================================

fn get_base_submissions() -> Vec<SubmissionWithResult> {
    vec![
        // ================================================================
        // Submission 1: Alice's submission for Problem 1
        // - Subtask 1 (Sum): All AC -> 30 points
        // - Subtask 2 (GroupMin): All AC -> 30 points
        // - Subtask 3 (GroupMin): All AC -> 40 points
        // Expected total: 100
        // ================================================================
        SubmissionWithResult {
            submission: Submission {
                id: 1,
                code: String::new(),
                language: "cpp".to_string(),
                status: "Finished".to_string(),
                user_id: 1,
                problem_id: 1,
                created_at: "2024-01-01T10:00:00Z".to_string(),
            },
            result: Some(JudgeResult {
                id: 1,
                verdict: "Pending".to_string(),
                score: 0,
                time_used: 0,
                memory_used: 0,
                submission_id: 1,
                created_at: "2024-01-01T10:00:01Z".to_string(),
            }),
            test_case_results: vec![
                // Subtask 1 (Sum scoring)
                TestCaseResult { id: 1, judge_result_id: 1, test_case_id: 1, verdict: "Accepted".to_string(), score: 10, time_used: 5, memory_used: 512, created_at: "2024-01-01T10:00:01Z".to_string() },
                TestCaseResult { id: 2, judge_result_id: 1, test_case_id: 2, verdict: "Accepted".to_string(), score: 10, time_used: 5, memory_used: 512, created_at: "2024-01-01T10:00:01Z".to_string() },
                TestCaseResult { id: 3, judge_result_id: 1, test_case_id: 3, verdict: "Accepted".to_string(), score: 10, time_used: 5, memory_used: 512, created_at: "2024-01-01T10:00:01Z".to_string() },
                // Subtask 2 (GroupMin scoring)
                TestCaseResult { id: 4, judge_result_id: 1, test_case_id: 4, verdict: "Accepted".to_string(), score: 15, time_used: 8, memory_used: 768, created_at: "2024-01-01T10:00:01Z".to_string() },
                TestCaseResult { id: 5, judge_result_id: 1, test_case_id: 5, verdict: "Accepted".to_string(), score: 15, time_used: 10, memory_used: 768, created_at: "2024-01-01T10:00:01Z".to_string() },
                // Subtask 3 (GroupMin scoring)
                TestCaseResult { id: 6, judge_result_id: 1, test_case_id: 6, verdict: "Accepted".to_string(), score: 20, time_used: 12, memory_used: 1024, created_at: "2024-01-01T10:00:01Z".to_string() },
                TestCaseResult { id: 7, judge_result_id: 1, test_case_id: 7, verdict: "Accepted".to_string(), score: 20, time_used: 15, memory_used: 1024, created_at: "2024-01-01T10:00:01Z".to_string() },
            ],
        },

        // ================================================================
        // Submission 2: Alice's submission for Problem 2 (partial)
        // - Subtask 1 (GroupMin): All AC -> 10 points
        // - Subtask 2 (GroupMin): All AC -> 20 points
        // - Subtask 3 (GroupMin): One TLE -> 0 points
        // Expected total: 30
        // ================================================================
        SubmissionWithResult {
            submission: Submission {
                id: 2,
                code: String::new(),
                language: "cpp".to_string(),
                status: "Finished".to_string(),
                user_id: 1,
                problem_id: 2,
                created_at: "2024-01-01T10:30:00Z".to_string(),
            },
            result: Some(JudgeResult {
                id: 2,
                verdict: "Pending".to_string(),
                score: 0,
                time_used: 0,
                memory_used: 0,
                submission_id: 2,
                created_at: "2024-01-01T10:30:01Z".to_string(),
            }),
            test_case_results: vec![
                // Subtask 1 (GroupMin)
                TestCaseResult { id: 8, judge_result_id: 2, test_case_id: 1, verdict: "Accepted".to_string(), score: 5, time_used: 50, memory_used: 4096, created_at: "2024-01-01T10:30:01Z".to_string() },
                TestCaseResult { id: 9, judge_result_id: 2, test_case_id: 2, verdict: "Accepted".to_string(), score: 5, time_used: 50, memory_used: 4096, created_at: "2024-01-01T10:30:01Z".to_string() },
                // Subtask 2 (GroupMin)
                TestCaseResult { id: 10, judge_result_id: 2, test_case_id: 3, verdict: "Accepted".to_string(), score: 7, time_used: 100, memory_used: 8192, created_at: "2024-01-01T10:30:01Z".to_string() },
                TestCaseResult { id: 11, judge_result_id: 2, test_case_id: 4, verdict: "Accepted".to_string(), score: 7, time_used: 150, memory_used: 8192, created_at: "2024-01-01T10:30:01Z".to_string() },
                TestCaseResult { id: 12, judge_result_id: 2, test_case_id: 5, verdict: "Accepted".to_string(), score: 6, time_used: 200, memory_used: 8192, created_at: "2024-01-01T10:30:01Z".to_string() },
                // Subtask 3 (GroupMin) - one TLE, so entire subtask = 0
                TestCaseResult { id: 13, judge_result_id: 2, test_case_id: 6, verdict: "TimeLimitExceeded".to_string(), score: 0, time_used: 2000, memory_used: 32768, created_at: "2024-01-01T10:30:01Z".to_string() },
                TestCaseResult { id: 14, judge_result_id: 2, test_case_id: 7, verdict: "Accepted".to_string(), score: 35, time_used: 800, memory_used: 16384, created_at: "2024-01-01T10:30:01Z".to_string() },
            ],
        },

        // ================================================================
        // Submission 3: Bob's submission for Problem 1
        // - Subtask 1 (Sum): All AC -> 30 points
        // - Subtask 2 (GroupMin): All AC -> 30 points  
        // - Subtask 3 (GroupMin): One TLE -> 0 points
        // Expected total: 60
        // ================================================================
        SubmissionWithResult {
            submission: Submission {
                id: 3,
                code: String::new(),
                language: "python".to_string(),
                status: "Finished".to_string(),
                user_id: 2,
                problem_id: 1,
                created_at: "2024-01-01T10:15:00Z".to_string(),
            },
            result: Some(JudgeResult {
                id: 3,
                verdict: "Pending".to_string(),
                score: 0,
                time_used: 0,
                memory_used: 0,
                submission_id: 3,
                created_at: "2024-01-01T10:15:01Z".to_string(),
            }),
            test_case_results: vec![
                // Subtask 1 (Sum)
                TestCaseResult { id: 15, judge_result_id: 3, test_case_id: 1, verdict: "Accepted".to_string(), score: 10, time_used: 50, memory_used: 2048, created_at: "2024-01-01T10:15:01Z".to_string() },
                TestCaseResult { id: 16, judge_result_id: 3, test_case_id: 2, verdict: "Accepted".to_string(), score: 10, time_used: 50, memory_used: 2048, created_at: "2024-01-01T10:15:01Z".to_string() },
                TestCaseResult { id: 17, judge_result_id: 3, test_case_id: 3, verdict: "Accepted".to_string(), score: 10, time_used: 50, memory_used: 2048, created_at: "2024-01-01T10:15:01Z".to_string() },
                // Subtask 2 (GroupMin)
                TestCaseResult { id: 18, judge_result_id: 3, test_case_id: 4, verdict: "Accepted".to_string(), score: 15, time_used: 100, memory_used: 4096, created_at: "2024-01-01T10:15:01Z".to_string() },
                TestCaseResult { id: 19, judge_result_id: 3, test_case_id: 5, verdict: "Accepted".to_string(), score: 15, time_used: 100, memory_used: 4096, created_at: "2024-01-01T10:15:01Z".to_string() },
                // Subtask 3 (GroupMin) - one TLE, entire subtask = 0
                TestCaseResult { id: 20, judge_result_id: 3, test_case_id: 6, verdict: "TimeLimitExceeded".to_string(), score: 0, time_used: 2000, memory_used: 8192, created_at: "2024-01-01T10:15:01Z".to_string() },
                TestCaseResult { id: 21, judge_result_id: 3, test_case_id: 7, verdict: "Accepted".to_string(), score: 20, time_used: 500, memory_used: 4096, created_at: "2024-01-01T10:15:01Z".to_string() },
            ],
        },

        // ================================================================
        // Submission 4: Bob's submission for Problem 2 (full score)
        // - All subtasks AC -> 100 points
        // Expected total: 100
        // ================================================================
        SubmissionWithResult {
            submission: Submission {
                id: 4,
                code: String::new(),
                language: "cpp".to_string(),
                status: "Finished".to_string(),
                user_id: 2,
                problem_id: 2,
                created_at: "2024-01-01T10:45:00Z".to_string(),
            },
            result: Some(JudgeResult {
                id: 4,
                verdict: "Pending".to_string(),
                score: 0,
                time_used: 0,
                memory_used: 0,
                submission_id: 4,
                created_at: "2024-01-01T10:45:01Z".to_string(),
            }),
            test_case_results: vec![
                TestCaseResult { id: 22, judge_result_id: 4, test_case_id: 1, verdict: "Accepted".to_string(), score: 5, time_used: 100, memory_used: 4096, created_at: "2024-01-01T10:45:01Z".to_string() },
                TestCaseResult { id: 23, judge_result_id: 4, test_case_id: 2, verdict: "Accepted".to_string(), score: 5, time_used: 100, memory_used: 4096, created_at: "2024-01-01T10:45:01Z".to_string() },
                TestCaseResult { id: 24, judge_result_id: 4, test_case_id: 3, verdict: "Accepted".to_string(), score: 7, time_used: 100, memory_used: 4096, created_at: "2024-01-01T10:45:01Z".to_string() },
                TestCaseResult { id: 25, judge_result_id: 4, test_case_id: 4, verdict: "Accepted".to_string(), score: 7, time_used: 200, memory_used: 8192, created_at: "2024-01-01T10:45:01Z".to_string() },
                TestCaseResult { id: 26, judge_result_id: 4, test_case_id: 5, verdict: "Accepted".to_string(), score: 6, time_used: 200, memory_used: 8192, created_at: "2024-01-01T10:45:01Z".to_string() },
                TestCaseResult { id: 27, judge_result_id: 4, test_case_id: 6, verdict: "Accepted".to_string(), score: 35, time_used: 400, memory_used: 16384, created_at: "2024-01-01T10:45:01Z".to_string() },
                TestCaseResult { id: 28, judge_result_id: 4, test_case_id: 7, verdict: "Accepted".to_string(), score: 35, time_used: 500, memory_used: 16384, created_at: "2024-01-01T10:45:01Z".to_string() },
            ],
        },

        // ================================================================
        // Submission 5: Charlie's submission for Problem 1
        // - Subtask 1 (Sum): Partial (2 AC, 1 WA) -> 20 points
        // - Subtask 2 (GroupMin): One WA -> 0 points
        // - Subtask 3 (GroupMin): All WA -> 0 points
        // Expected total: 20
        // ================================================================
        SubmissionWithResult {
            submission: Submission {
                id: 5,
                code: String::new(),
                language: "java".to_string(),
                status: "Finished".to_string(),
                user_id: 3,
                problem_id: 1,
                created_at: "2024-01-01T11:00:00Z".to_string(),
            },
            result: Some(JudgeResult {
                id: 5,
                verdict: "Pending".to_string(),
                score: 0,
                time_used: 0,
                memory_used: 0,
                submission_id: 5,
                created_at: "2024-01-01T11:00:01Z".to_string(),
            }),
            test_case_results: vec![
                // Subtask 1 (Sum) - partial score
                TestCaseResult { id: 29, judge_result_id: 5, test_case_id: 1, verdict: "Accepted".to_string(), score: 10, time_used: 100, memory_used: 8192, created_at: "2024-01-01T11:00:01Z".to_string() },
                TestCaseResult { id: 30, judge_result_id: 5, test_case_id: 2, verdict: "Accepted".to_string(), score: 10, time_used: 100, memory_used: 8192, created_at: "2024-01-01T11:00:01Z".to_string() },
                TestCaseResult { id: 31, judge_result_id: 5, test_case_id: 3, verdict: "WrongAnswer".to_string(), score: 0, time_used: 100, memory_used: 8192, created_at: "2024-01-01T11:00:01Z".to_string() },
                // Subtask 2 (GroupMin) - one WA, entire subtask = 0
                TestCaseResult { id: 32, judge_result_id: 5, test_case_id: 4, verdict: "Accepted".to_string(), score: 15, time_used: 200, memory_used: 16384, created_at: "2024-01-01T11:00:01Z".to_string() },
                TestCaseResult { id: 33, judge_result_id: 5, test_case_id: 5, verdict: "WrongAnswer".to_string(), score: 0, time_used: 200, memory_used: 16384, created_at: "2024-01-01T11:00:01Z".to_string() },
                // Subtask 3 (GroupMin) - all WA
                TestCaseResult { id: 34, judge_result_id: 5, test_case_id: 6, verdict: "WrongAnswer".to_string(), score: 0, time_used: 300, memory_used: 32768, created_at: "2024-01-01T11:00:01Z".to_string() },
                TestCaseResult { id: 35, judge_result_id: 5, test_case_id: 7, verdict: "WrongAnswer".to_string(), score: 0, time_used: 300, memory_used: 32768, created_at: "2024-01-01T11:00:01Z".to_string() },
            ],
        },

        // ================================================================
        // Submission 6: Charlie's second submission for Problem 1
        // - Subtask 1 (Sum): All AC -> 30 points
        // - Subtask 2 (GroupMin): All AC -> 30 points
        // - Subtask 3 (GroupMin): One WA -> 0 points
        // Expected total: 60
        // For BestSubtaskSum: Best of subtask1=30, subtask2=30, subtask3=0 from both submissions
        // ================================================================
        SubmissionWithResult {
            submission: Submission {
                id: 6,
                code: String::new(),
                language: "java".to_string(),
                status: "Finished".to_string(),
                user_id: 3,
                problem_id: 1,
                created_at: "2024-01-01T11:30:00Z".to_string(),
            },
            result: Some(JudgeResult {
                id: 6,
                verdict: "Pending".to_string(),
                score: 0,
                time_used: 0,
                memory_used: 0,
                submission_id: 6,
                created_at: "2024-01-01T11:30:01Z".to_string(),
            }),
            test_case_results: vec![
                // Subtask 1 (Sum) - full score
                TestCaseResult { id: 36, judge_result_id: 6, test_case_id: 1, verdict: "Accepted".to_string(), score: 10, time_used: 80, memory_used: 8192, created_at: "2024-01-01T11:30:01Z".to_string() },
                TestCaseResult { id: 37, judge_result_id: 6, test_case_id: 2, verdict: "Accepted".to_string(), score: 10, time_used: 80, memory_used: 8192, created_at: "2024-01-01T11:30:01Z".to_string() },
                TestCaseResult { id: 38, judge_result_id: 6, test_case_id: 3, verdict: "Accepted".to_string(), score: 10, time_used: 80, memory_used: 8192, created_at: "2024-01-01T11:30:01Z".to_string() },
                // Subtask 2 (GroupMin) - full score
                TestCaseResult { id: 39, judge_result_id: 6, test_case_id: 4, verdict: "Accepted".to_string(), score: 15, time_used: 150, memory_used: 16384, created_at: "2024-01-01T11:30:01Z".to_string() },
                TestCaseResult { id: 40, judge_result_id: 6, test_case_id: 5, verdict: "Accepted".to_string(), score: 15, time_used: 150, memory_used: 16384, created_at: "2024-01-01T11:30:01Z".to_string() },
                // Subtask 3 (GroupMin) - one WA, entire subtask = 0
                TestCaseResult { id: 41, judge_result_id: 6, test_case_id: 6, verdict: "Accepted".to_string(), score: 20, time_used: 250, memory_used: 32768, created_at: "2024-01-01T11:30:01Z".to_string() },
                TestCaseResult { id: 42, judge_result_id: 6, test_case_id: 7, verdict: "WrongAnswer".to_string(), score: 0, time_used: 250, memory_used: 32768, created_at: "2024-01-01T11:30:01Z".to_string() },
            ],
        },
    ]
}
