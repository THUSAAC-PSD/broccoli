
use crate::dto::{SubmissionStatus, Verdict};
use crate::fixtures;

#[derive(Debug, Clone)]
pub struct Scenario {
    pub id: &'static str,

    pub language: &'static str,

    pub files: &'static [(&'static str, &'static str)],

    pub time_limit_ms: i32,

    pub memory_limit_kb: i32,

    pub checker_format: &'static str,

    pub test_input: &'static str,

    pub test_expected_output: &'static str,

    pub expected_status: SubmissionStatus,
    pub expected_verdict: Option<Verdict>,
}

pub const DEFAULT_PROBLEM_TIME_LIMIT_MS: i32 = 1000;
pub const DEFAULT_PROBLEM_MEMORY_LIMIT_KB: i32 = 65_536;

pub const SCENARIOS: &[Scenario] = &[
    Scenario {
        id: "ab-cpp-ac",
        language: "cpp",
        files: &[("solution.cpp", fixtures::SOLUTION_AB_CPP_AC)],
        time_limit_ms: DEFAULT_PROBLEM_TIME_LIMIT_MS,
        memory_limit_kb: DEFAULT_PROBLEM_MEMORY_LIMIT_KB,
        checker_format: "exact",
        test_input: "1 2\n",
        test_expected_output: "3\n",
        expected_status: SubmissionStatus::Judged,
        expected_verdict: Some(Verdict::Accepted),
    },
    Scenario {
        id: "ab-py-ac",
        language: "python3",
        files: &[("solution.py", fixtures::SOLUTION_AB_PY_AC)],
        time_limit_ms: DEFAULT_PROBLEM_TIME_LIMIT_MS,
        memory_limit_kb: DEFAULT_PROBLEM_MEMORY_LIMIT_KB,
        checker_format: "exact",
        test_input: "1 2\n",
        test_expected_output: "3\n",
        expected_status: SubmissionStatus::Judged,
        expected_verdict: Some(Verdict::Accepted),
    },
    Scenario {
        id: "ab-cpp-wa",
        language: "cpp",
        files: &[("solution.cpp", fixtures::SOLUTION_AB_CPP_WA)],
        time_limit_ms: DEFAULT_PROBLEM_TIME_LIMIT_MS,
        memory_limit_kb: DEFAULT_PROBLEM_MEMORY_LIMIT_KB,
        checker_format: "exact",
        test_input: "1 2\n",
        test_expected_output: "3\n",
        expected_status: SubmissionStatus::Judged,
        expected_verdict: Some(Verdict::WrongAnswer),
    },
    Scenario {
        id: "ab-cpp-tle",
        language: "cpp",
        files: &[("solution.cpp", fixtures::SOLUTION_AB_CPP_TLE)],
        time_limit_ms: 1000,
        memory_limit_kb: DEFAULT_PROBLEM_MEMORY_LIMIT_KB,
        checker_format: "exact",
        test_input: "1 2\n",
        test_expected_output: "3\n",
        expected_status: SubmissionStatus::Judged,
        expected_verdict: Some(Verdict::TimeLimitExceeded),
    },
    Scenario {
        id: "ab-cpp-mle",
        language: "cpp",
        files: &[("solution.cpp", fixtures::SOLUTION_AB_CPP_MLE)],
        time_limit_ms: 5_000,
        memory_limit_kb: 64 * 1024,
        checker_format: "exact",
        test_input: "1 2\n",
        test_expected_output: "3\n",
        expected_status: SubmissionStatus::Judged,
        expected_verdict: Some(Verdict::MemoryLimitExceeded),
    },
    Scenario {
        id: "ab-cpp-re",
        language: "cpp",
        files: &[("solution.cpp", fixtures::SOLUTION_AB_CPP_RE)],
        time_limit_ms: DEFAULT_PROBLEM_TIME_LIMIT_MS,
        memory_limit_kb: DEFAULT_PROBLEM_MEMORY_LIMIT_KB,
        checker_format: "exact",
        test_input: "1 2\n",
        test_expected_output: "3\n",
        expected_status: SubmissionStatus::Judged,
        expected_verdict: Some(Verdict::RuntimeError),
    },
    Scenario {
        id: "ab-cpp-ce",
        language: "cpp",
        files: &[("solution.cpp", fixtures::SOLUTION_AB_CPP_CE)],
        time_limit_ms: DEFAULT_PROBLEM_TIME_LIMIT_MS,
        memory_limit_kb: DEFAULT_PROBLEM_MEMORY_LIMIT_KB,
        checker_format: "exact",
        test_input: "1 2\n",
        test_expected_output: "3\n",
        expected_status: SubmissionStatus::CompilationError,
        expected_verdict: None,
    },
    Scenario {
        id: "ab-cpp-igncase",
        language: "cpp",
        files: &[("solution.cpp", fixtures::SOLUTION_AB_CPP_IGNCASE)],
        time_limit_ms: DEFAULT_PROBLEM_TIME_LIMIT_MS,
        memory_limit_kb: DEFAULT_PROBLEM_MEMORY_LIMIT_KB,
        checker_format: "ignore_case",
        test_input: "1 2\n",
        test_expected_output: "YES\n",
        expected_status: SubmissionStatus::Judged,
        expected_verdict: Some(Verdict::Accepted),
    },
    Scenario {
        id: "ab-cpp-multi",
        language: "cpp",
        files: &[
            ("solution.cpp", fixtures::MULTI_FILE_SOLUTION_CPP),
            ("helper.hpp", fixtures::MULTI_FILE_HELPER_HPP),
        ],
        time_limit_ms: DEFAULT_PROBLEM_TIME_LIMIT_MS,
        memory_limit_kb: DEFAULT_PROBLEM_MEMORY_LIMIT_KB,
        checker_format: "exact",
        test_input: "1 2\n",
        test_expected_output: "3\n",
        expected_status: SubmissionStatus::Judged,
        expected_verdict: Some(Verdict::Accepted),
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn scenario_count_matches_design_doc() {
        assert_eq!(SCENARIOS.len(), 9);
    }

    #[test]
    fn scenario_ids_are_unique() {
        let mut seen = HashSet::new();
        for s in SCENARIOS {
            assert!(seen.insert(s.id), "duplicate id: {}", s.id);
        }
    }

    #[test]
    fn scenarios_cover_design_doc_table() {
        let ids: Vec<_> = SCENARIOS.iter().map(|s| s.id).collect();
        for expected in [
            "ab-cpp-ac",
            "ab-py-ac",
            "ab-cpp-wa",
            "ab-cpp-tle",
            "ab-cpp-mle",
            "ab-cpp-re",
            "ab-cpp-ce",
            "ab-cpp-igncase",
            "ab-cpp-multi",
        ] {
            assert!(
                ids.contains(&expected),
                "missing scenario: {expected}; have {ids:?}",
            );
        }
    }

    #[test]
    fn ce_scenario_has_no_verdict() {
        let ce = SCENARIOS
            .iter()
            .find(|s| s.id == "ab-cpp-ce")
            .expect("ce scenario exists");
        assert_eq!(ce.expected_status, SubmissionStatus::CompilationError);
        assert!(ce.expected_verdict.is_none());
    }

    #[test]
    fn judged_scenarios_have_verdict() {
        for s in SCENARIOS {
            if s.expected_status == SubmissionStatus::Judged {
                assert!(
                    s.expected_verdict.is_some(),
                    "scenario {} reaches Judged but has no expected verdict",
                    s.id,
                );
            }
        }
    }

    #[test]
    fn no_scenario_expects_system_error() {
        for s in SCENARIOS {
            assert_ne!(
                s.expected_status,
                SubmissionStatus::SystemError,
                "scenario {} expects SystemError",
                s.id,
            );
        }
    }

    #[test]
    fn every_scenario_has_at_least_one_file() {
        for s in SCENARIOS {
            assert!(!s.files.is_empty(), "scenario {} has no files", s.id);
            for (filename, content) in s.files {
                assert!(!filename.is_empty(), "empty filename in {}", s.id);
                assert!(
                    !content.trim().is_empty(),
                    "empty content for {} in scenario {}",
                    filename,
                    s.id,
                );
            }
        }
    }

    #[test]
    fn time_and_memory_limits_are_positive() {
        for s in SCENARIOS {
            assert!(s.time_limit_ms > 0, "non-positive time limit in {}", s.id);
            assert!(
                s.memory_limit_kb > 0,
                "non-positive memory limit in {}",
                s.id
            );
        }
    }

    #[test]
    fn igncase_scenario_uses_ignore_case_checker() {
        let s = SCENARIOS
            .iter()
            .find(|s| s.id == "ab-cpp-igncase")
            .expect("igncase scenario exists");
        assert_eq!(s.checker_format, "ignore_case");
        assert_ne!(
            s.test_expected_output.trim(),
            "yes",
            "expected output for igncase must be a different case to actually test the checker",
        );
    }

    #[test]
    fn multi_file_scenario_has_two_files() {
        let s = SCENARIOS
            .iter()
            .find(|s| s.id == "ab-cpp-multi")
            .expect("multi scenario exists");
        assert_eq!(s.files.len(), 2);
        assert_eq!(s.files[0].0, "solution.cpp");
        assert_eq!(s.files[1].0, "helper.hpp");
    }
}
