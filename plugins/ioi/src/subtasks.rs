use std::collections::HashMap;

use broccoli_server_sdk::types::TestCaseRow;

use crate::config::{SubtaskDef, SubtaskScoringMethod, resolve_tc_label, round_score};

#[derive(Debug, Clone)]
pub struct SubtaskResult {
    pub name: String,
    pub score: f64,
    pub max_score: f64,
}

/// Score a single subtask using the configured method.
pub fn score_subtask(def: &SubtaskDef, tc_scores: &HashMap<String, f64>) -> SubtaskResult {
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
                let n = def.test_cases.len() as f64;
                let sum: f64 = def
                    .test_cases
                    .iter()
                    .map(|label| tc_scores.get(label).copied().unwrap_or(0.0))
                    .sum();
                def.max_score * (sum / n)
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
    tc_scores: &HashMap<String, f64>,
) -> Vec<SubtaskResult> {
    defs.iter()
        .map(|def| score_subtask(def, tc_scores))
        .collect()
}

/// Build a single default subtask containing all test cases with Sum scoring.
///
/// Used when no subtask definitions are configured.
pub fn build_default_subtasks(test_cases: &[TestCaseRow]) -> Vec<SubtaskDef> {
    if test_cases.is_empty() {
        return vec![];
    }
    let total_score: f64 = test_cases.iter().map(|tc| tc.score).sum();
    vec![SubtaskDef {
        name: "All Tests".into(),
        scoring_method: SubtaskScoringMethod::Sum,
        max_score: total_score,
        test_cases: test_cases.iter().map(resolve_tc_label).collect(),
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
        let result = score_subtask(&def, &s);
        assert_eq!(result.score, 30.0);
    }

    #[test]
    fn group_min_one_fail() {
        let def = make_def(SubtaskScoringMethod::GroupMin, 30.0, vec!["1", "2", "3"]);
        let s = scores(&[("1", 1.0), ("2", 0.5), ("3", 1.0)]);
        let result = score_subtask(&def, &s);
        assert_eq!(result.score, 0.0);
    }

    #[test]
    fn group_min_empty() {
        let def = make_def(SubtaskScoringMethod::GroupMin, 30.0, vec![]);
        let s = HashMap::new();
        let result = score_subtask(&def, &s);
        assert_eq!(result.score, 0.0);
    }

    #[test]
    fn sum_proportional() {
        let def = make_def(SubtaskScoringMethod::Sum, 100.0, vec!["1", "2"]);
        let s = scores(&[("1", 1.0), ("2", 1.0)]);
        let result = score_subtask(&def, &s);
        assert_eq!(result.score, 100.0);
    }

    #[test]
    fn sum_partial_scores() {
        let def = make_def(SubtaskScoringMethod::Sum, 100.0, vec!["1", "2"]);
        let s = scores(&[("1", 0.5), ("2", 0.5)]);
        let result = score_subtask(&def, &s);
        assert_eq!(result.score, 50.0);
    }

    #[test]
    fn sum_all_zero() {
        let def = make_def(SubtaskScoringMethod::Sum, 100.0, vec!["1", "2"]);
        let s = scores(&[("1", 0.0), ("2", 0.0)]);
        let result = score_subtask(&def, &s);
        assert_eq!(result.score, 0.0);
    }

    #[test]
    fn sum_empty() {
        let def = make_def(SubtaskScoringMethod::Sum, 100.0, vec![]);
        let s = HashMap::new();
        let result = score_subtask(&def, &s);
        assert_eq!(result.score, 0.0);
    }

    #[test]
    fn group_mul_all_perfect() {
        let def = make_def(SubtaskScoringMethod::GroupMul, 50.0, vec!["1", "2"]);
        let s = scores(&[("1", 1.0), ("2", 1.0)]);
        let result = score_subtask(&def, &s);
        assert_eq!(result.score, 50.0);
    }

    #[test]
    fn group_mul_one_half() {
        let def = make_def(SubtaskScoringMethod::GroupMul, 50.0, vec!["1", "2"]);
        let s = scores(&[("1", 1.0), ("2", 0.5)]);
        let result = score_subtask(&def, &s);
        assert_eq!(result.score, 25.0);
    }

    #[test]
    fn group_mul_one_zero() {
        let def = make_def(SubtaskScoringMethod::GroupMul, 50.0, vec!["1", "2"]);
        let s = scores(&[("1", 1.0), ("2", 0.0)]);
        let result = score_subtask(&def, &s);
        assert_eq!(result.score, 0.0);
    }

    #[test]
    fn missing_tc_treated_as_zero() {
        let def = make_def(SubtaskScoringMethod::Sum, 100.0, vec!["1", "2", "3"]);
        let s = scores(&[("1", 1.0)]); // 2 and 3 missing
        let result = score_subtask(&def, &s);
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
            },
            TestCaseRow {
                id: 2,
                score: 70.0,
                is_sample: false,
                position: 1,
                description: None,
                label: Some("tc_2".into()),
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
            },
            TestCaseRow {
                id: 2,
                score: 50.0,
                is_sample: false,
                position: 1,
                description: None,
                label: None,
            },
        ];
        let defs = build_default_subtasks(&test_cases);
        assert_eq!(defs[0].test_cases, vec!["1", "2"]);
    }

    #[test]
    fn build_default_subtasks_empty() {
        let defs = build_default_subtasks(&[]);
        assert!(defs.is_empty());
    }
}
