use std::process::Command;

fn require_env(key: &str) -> String {
    std::env::var(key)
        .unwrap_or_else(|_| panic!("{key} must be set; see packages/stress-test/README.md"))
}

#[test]
#[ignore = "requires a running broccoli stack; opt in with --ignored"]
fn stress_test_passes_against_real_server() {
    let url = require_env("STRESS_TEST_E2E_URL");
    let username =
        std::env::var("STRESS_TEST_E2E_USERNAME").unwrap_or_else(|_| "admin".to_string());
    let password = require_env("STRESS_TEST_E2E_PASSWORD");

    let bin = env!("CARGO_BIN_EXE_broccoli-stress-test");
    let output = Command::new(bin)
        .args([
            "--url",
            &url,
            "--admin-username",
            &username,
            "--admin-password",
            &password,
            "--skip-load",
            "--json",
        ])
        .output()
        .expect("failed to spawn broccoli-stress-test");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "stress-test exited {:?}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        output.status.code()
    );

    let payload: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("non-JSON stdout: {e}\nstdout:\n{stdout}"));
    assert_eq!(payload["result"], "pass", "payload:\n{stdout}");
    assert_eq!(payload["exit_code"], 0, "payload:\n{stdout}");
}
