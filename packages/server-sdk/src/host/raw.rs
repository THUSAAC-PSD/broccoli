use extism_pdk::host_fn;

#[host_fn]
extern "ExtismHost" {
    pub fn log_info(msg: String);
    pub fn register_contest_type(input: String);
    pub fn register_evaluator(input: String);
    pub fn register_checker_format(input: String);
    pub fn db_query(sql: String, args: String) -> String;
    pub fn db_execute(sql: String, args: String) -> String;
    pub fn db_transaction(queries_json: String) -> String;
    pub fn db_begin(input: String) -> String;
    pub fn db_query_in(txn_id: String, sql: String, args: String) -> String;
    pub fn db_execute_in(txn_id: String, sql: String, args: String) -> String;
    pub fn db_commit(txn_id: String) -> String;
    pub fn db_rollback(txn_id: String) -> String;
    pub fn start_evaluate_batch(input: String) -> String;
    pub fn get_next_evaluate_result(input: String) -> String;
    pub fn cancel_evaluate_batch(input: String);
    pub fn start_operation_batch(input: String) -> String;
    pub fn get_next_operation_result(input: String) -> String;
    pub fn cancel_operation_batch(input: String);
    pub fn run_checker(input: String) -> String;
    pub fn get_language_config(input: String) -> String;
    pub fn store_get(input: String) -> String;
    pub fn store_set(input: String);
    pub fn config_get(input: String) -> String;
    pub fn config_set(input: String);
}
