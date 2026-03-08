use crate::error::SdkError;
use crate::host;
use crate::types::{ProblemCheckerInfo, TestCaseData, TestCaseRow};

/// Query test cases for a problem, ordered by position.
pub fn query_test_cases(problem_id: i32) -> Result<Vec<TestCaseRow>, SdkError> {
    let sql = format!(
        "SELECT id, score, is_sample, position FROM test_case WHERE problem_id = {} ORDER BY position ASC",
        problem_id
    );
    host::db::db_query(&sql)
}

/// Query a single test case's input and expected output.
pub fn query_test_case_data(test_case_id: i32) -> Result<TestCaseData, SdkError> {
    let sql = format!(
        "SELECT input, expected_output FROM test_case WHERE id = {}",
        test_case_id
    );
    let rows: Vec<TestCaseData> = host::db::db_query(&sql)?;
    rows.into_iter()
        .next()
        .ok_or_else(|| SdkError::Database(format!("Test case {} not found", test_case_id)))
}

/// Query a problem's checker configuration, including any checker config from plugin_config.
pub fn query_problem_checker(problem_id: i32) -> Result<ProblemCheckerInfo, SdkError> {
    let sql = format!(
        "SELECT p.id, p.checker_source, p.checker_format, pc.config AS checker_config \
         FROM problem p \
         LEFT JOIN plugin_config pc ON pc.scope = 'problem' AND pc.ref_id = '{}' AND pc.namespace = 'checker' \
         WHERE p.id = {}",
        problem_id, problem_id
    );
    let rows: Vec<ProblemCheckerInfo> = host::db::db_query(&sql)?;
    rows.into_iter()
        .next()
        .ok_or_else(|| SdkError::Database(format!("Problem {} not found", problem_id)))
}
