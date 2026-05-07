use std::collections::HashMap;

#[cfg(test)]
use broccoli_server_sdk::types::TestCaseBodyRef;
use broccoli_server_sdk::types::TestCaseRow;

use crate::config::{SubtaskDef, SubtaskScoringMethod, resolve_tc_label, round_score};

#[derive(Debug, Clone)]
pub struct SubtaskResult {
    pub name: String,
    pub score: f64,
    pub max_score: f64,
}

fn test_case_weights(test_cases: &[TestCaseRow]) -> HashMap<String, f64> {
    test_cases
        .iter()
        .map(|tc| (resolve_tc_label(tc), tc.score))
        .collect()
}

/// Score a single subtask using the configured method.
pub fn score_subtask(
    def: &SubtaskDef,
    test_cases: &[TestCaseRow],
    tc_scores: &HashMap<String, f64>,
) -> SubtaskResult {
    let weights = test_case_weights(test_cases);
    score_subtask_with_weights(def, &weights, tc_scores)
}

fn score_subtask_with_weights(
    def: &SubtaskDef,
    test_case_weights: &HashMap<String, f64>,
    tc_scores: &HashMap<String, f64>,
) -> SubtaskResult {
    let score = if def.test_cases.is_empty() {
        0.0
    } else {
        match def.scoring_method {
            SubtaskScoringMethod::GroupMin => {
                let all_pass = def
                    .test_cases
                    .iter()
                    .all(|label| tc_scores.get(label).copied().unwrap_or(0.0) >= 1.0);
                if all_pass { def.max_score } else { 0.0 }
            }
            SubtaskScoringMethod::Sum => {
                let mut total_weight = 0.0;
                let mut weighted_sum = 0.0;
                for label in &def.test_cases {
                    let weight = test_case_weights.get(label).copied().unwrap_or(1.0);
                    if weight <= 0.0 {
                        continue;
                    }
                    total_weight += weight;
                    weighted_sum += tc_scores.get(label).copied().unwrap_or(0.0) * weight;
                }
                if total_weight > 0.0 {
                    def.max_score * (weighted_sum / total_weight)
                } else {
                    0.0
                }
            }
            SubtaskScoringMethod::GroupMul => {
                let product: f64 = def
                    .test_cases
                    .iter()
                    .map(|label| tc_scores.get(label).copied().unwrap_or(0.0))
                    .product();
                def.max_score * product
            }
        }
    };

    SubtaskResult {
        name: def.name.clone(),
        score: round_score(score),
        max_score: def.max_score,
    }
}

/// Score all subtasks and return results in definition order.
pub fn score_all_subtasks(
    defs: &[SubtaskDef],
    test_cases: &[TestCaseRow],
    tc_scores: &HashMap<String, f64>,
) -> Vec<SubtaskResult> {
    let weights = test_case_weights(test_cases);
    defs.iter()
        .map(|def| score_subtask_with_weights(def, &weights, tc_scores))
        .collect()
}

/// Build a single default subtask containing all test cases with Sum scoring.
///
/// Used when no subtask definitions are configured.
pub fn build_default_subtasks(test_cases: &[TestCaseRow]) -> Vec<SubtaskDef> {
    let scoring_test_cases: Vec<&TestCaseRow> = test_cases
        .iter()
        .filter(|tc| !tc.is_sample && tc.score > 0.0)
        .collect();
    if scoring_test_cases.is_empty() {
        return vec![];
    }
    let total_score: f64 = scoring_test_cases.iter().map(|tc| tc.score).sum();
    vec![SubtaskDef {
        name: "All Tests".into(),
        scoring_method: SubtaskScoringMethod::Sum,
        max_score: total_score,
        test_cases: scoring_test_cases
            .iter()
            .map(|tc| resolve_tc_label(tc))
            .collect(),
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_def(method: SubtaskScoringMethod, max_score: f64, labels: Vec<&str>) -> SubtaskDef {
        SubtaskDef {
            name: "Test".into(),
            scoring_method: method,
            max_score,
            test_cases: labels.into_iter().map(String::from).collect(),
        }
    }

    fn scores(pairs: &[(&str, f64)]) -> HashMap<String, f64> {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    #[test]
    fn group_min_all_pass() {
        let def = make_def(SubtaskScoringMethod::GroupMin, 30.0, vec!["1", "2", "3"]);
        let s = scores(&[("1", 1.0), ("2", 1.0), ("3", 1.0)]);
        let result = score_subtask(&def, &[], &s);
        assert_eq!(result.score, 30.0);
    }

    #[test]
    fn group_min_one_fail() {
        let def = make_def(SubtaskScoringMethod::GroupMin, 30.0, vec!["1", "2", "3"]);
        let s = scores(&[("1", 1.0), ("2", 0.5), ("3", 1.0)]);
        let result = score_subtask(&def, &[], &s);
        assert_eq!(result.score, 0.0);
    }

    #[test]
    fn group_min_empty() {
        let def = make_def(SubtaskScoringMethod::GroupMin, 30.0, vec![]);
        let s = HashMap::new();
        let result = score_subtask(&def, &[], &s);
        assert_eq!(result.score, 0.0);
    }

    #[test]
    fn sum_proportional() {
        let def = make_def(SubtaskScoringMethod::Sum, 100.0, vec!["1", "2"]);
        let s = scores(&[("1", 1.0), ("2", 1.0)]);
        let result = score_subtask(&def, &[], &s);
        assert_eq!(result.score, 100.0);
    }

    #[test]
    fn sum_partial_scores() {
        let def = make_def(SubtaskScoringMethod::Sum, 100.0, vec!["1", "2"]);
        let s = scores(&[("1", 0.5), ("2", 0.5)]);
        let result = score_subtask(&def, &[], &s);
        assert_eq!(result.score, 50.0);
    }

    #[test]
    fn sum_all_zero() {
        let def = make_def(SubtaskScoringMethod::Sum, 100.0, vec!["1", "2"]);
        let s = scores(&[("1", 0.0), ("2", 0.0)]);
        let result = score_subtask(&def, &[], &s);
        assert_eq!(result.score, 0.0);
    }

    #[test]
    fn sum_empty() {
        let def = make_def(SubtaskScoringMethod::Sum, 100.0, vec![]);
        let s = HashMap::new();
        let result = score_subtask(&def, &[], &s);
        assert_eq!(result.score, 0.0);
    }

    #[test]
    fn group_mul_all_perfect() {
        let def = make_def(SubtaskScoringMethod::GroupMul, 50.0, vec!["1", "2"]);
        let s = scores(&[("1", 1.0), ("2", 1.0)]);
        let result = score_subtask(&def, &[], &s);
        assert_eq!(result.score, 50.0);
    }

    #[test]
    fn group_mul_one_half() {
        let def = make_def(SubtaskScoringMethod::GroupMul, 50.0, vec!["1", "2"]);
        let s = scores(&[("1", 1.0), ("2", 0.5)]);
        let result = score_subtask(&def, &[], &s);
        assert_eq!(result.score, 25.0);
    }

    #[test]
    fn group_mul_one_zero() {
        let def = make_def(SubtaskScoringMethod::GroupMul, 50.0, vec!["1", "2"]);
        let s = scores(&[("1", 1.0), ("2", 0.0)]);
        let result = score_subtask(&def, &[], &s);
        assert_eq!(result.score, 0.0);
    }

    #[test]
    fn missing_tc_treated_as_zero() {
        let def = make_def(SubtaskScoringMethod::Sum, 100.0, vec!["1", "2", "3"]);
        let s = scores(&[("1", 1.0)]); // 2 and 3 missing
        let result = score_subtask(&def, &[], &s);
        // 100 * (1.0 + 0.0 + 0.0) / 3 = 33.33
        assert_eq!(result.score, 33.33);
    }

    #[test]
    fn build_default_subtasks_creates_single_sum_group() {
        let test_cases = vec![
            TestCaseRow {
                id: 1,
                score: 30.0,
                is_sample: false,
                position: 0,
                description: None,
                label: Some("tc_1".into()),
                input: TestCaseBodyRef::Missing,
                expected_output: TestCaseBodyRef::Missing,
                is_custom: false,
            },
            TestCaseRow {
                id: 2,
                score: 70.0,
                is_sample: false,
                position: 1,
                description: None,
                label: Some("tc_2".into()),
                input: TestCaseBodyRef::Missing,
                expected_output: TestCaseBodyRef::Missing,
                is_custom: false,
            },
        ];
        let defs = build_default_subtasks(&test_cases);
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].max_score, 100.0);
        assert_eq!(defs[0].scoring_method, SubtaskScoringMethod::Sum);
        assert_eq!(defs[0].test_cases, vec!["tc_1", "tc_2"]);
    }

    #[test]
    fn build_default_subtasks_fallback_to_id() {
        let test_cases = vec![
            TestCaseRow {
                id: 1,
                score: 50.0,
                is_sample: false,
                position: 0,
                description: None,
                label: None,
                input: TestCaseBodyRef::Missing,
                expected_output: TestCaseBodyRef::Missing,
                is_custom: false,
            },
            TestCaseRow {
                id: 2,
                score: 50.0,
                is_sample: false,
                position: 1,
                description: None,
                label: None,
                input: TestCaseBodyRef::Missing,
                expected_output: TestCaseBodyRef::Missing,
                is_custom: false,
            },
        ];
        let defs = build_default_subtasks(&test_cases);
        assert_eq!(defs[0].test_cases, vec!["1", "2"]);
    }

    #[test]
    fn build_default_subtasks_ignores_samples_and_zero_point_cases() {
        let test_cases = vec![
            TestCaseRow {
                id: 1,
                score: 0.0,
                is_sample: true,
                position: 0,
                description: None,
                label: Some("sample_01".into()),
                input: TestCaseBodyRef::Missing,
                expected_output: TestCaseBodyRef::Missing,
                is_custom: false,
            },
            TestCaseRow {
                id: 2,
                score: 0.0,
                is_sample: false,
                position: 1,
                description: None,
                label: Some("zero_01".into()),
                input: TestCaseBodyRef::Missing,
                expected_output: TestCaseBodyRef::Missing,
                is_custom: false,
            },
            TestCaseRow {
                id: 3,
                score: 100.0,
                is_sample: false,
                position: 2,
                description: None,
                label: Some("tc_01".into()),
                input: TestCaseBodyRef::Missing,
                expected_output: TestCaseBodyRef::Missing,
                is_custom: false,
            },
        ];

        let defs = build_default_subtasks(&test_cases);
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].max_score, 100.0);
        assert_eq!(defs[0].test_cases, vec!["tc_01"]);
    }

    #[test]
    fn build_default_subtasks_empty() {
        let defs = build_default_subtasks(&[]);
        assert!(defs.is_empty());
    }
}
