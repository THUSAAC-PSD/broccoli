use std::time::Duration;

use sha2::{Digest, Sha256};
use tokio::process::Command;
use tokio::time::timeout;
use tracing::{info, warn};

#[derive(Clone, Debug)]
pub struct Probe {
    pub name: &'static str,
    pub command: &'static str,
    pub args: &'static [&'static str],
}

pub fn default_probes() -> &'static [Probe] {
    &[
        Probe {
            name: "gcc",
            command: "gcc",
            args: &["--version"],
        },
        Probe {
            name: "g++",
            command: "g++",
            args: &["--version"],
        },
        Probe {
            name: "clang",
            command: "clang",
            args: &["--version"],
        },
        Probe {
            name: "clang++",
            command: "clang++",
            args: &["--version"],
        },
        Probe {
            name: "python3",
            command: "python3",
            args: &["--version"],
        },
        Probe {
            name: "java",
            command: "java",
            args: &["-version"],
        },
        Probe {
            name: "javac",
            command: "javac",
            args: &["-version"],
        },
    ]
}

const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

pub async fn compute(probes: &[Probe]) -> String {
    let mut entries: Vec<(&'static str, String)> = Vec::with_capacity(probes.len());
    for probe in probes {
        let outcome = probe_one(probe).await;
        entries.push((probe.name, outcome));
    }
    entries.sort_by_key(|(name, _)| *name);

    let mut hasher = Sha256::new();
    for (name, output) in &entries {
        hasher.update(name.as_bytes());
        hasher.update(b"\0");
        hasher.update(output.as_bytes());
        hasher.update(b"\0");
    }
    hex::encode(hasher.finalize())
}

async fn probe_one(probe: &Probe) -> String {
    let mut cmd = Command::new(probe.command);
    cmd.args(probe.args);
    cmd.kill_on_drop(true);

    match timeout(PROBE_TIMEOUT, cmd.output()).await {
        Ok(Ok(output)) if output.status.success() => {
            let mut combined = Vec::with_capacity(output.stdout.len() + output.stderr.len());
            combined.extend_from_slice(&output.stdout);
            combined.extend_from_slice(&output.stderr);
            let text = String::from_utf8_lossy(&combined).trim().to_string();
            info!(probe = probe.name, version = %text, "toolchain probe succeeded");
            text
        }
        // KNOWN LIMITATION (low severity): a TRANSIENT probe failure (non-zero
        // exit, spawn error, timeout below) collapses to the same stable
        // "unavailable" token as a genuinely-absent toolchain. Because this token
        // feeds the compile-cache key, a binary compiled on a worker whose probe
        // transiently failed can be reused on a worker with a DIFFERENT actual
        // toolchain whose probe also failed - a wrong-toolchain cache hit -> wrong
        // compile output -> wrong verdict. The correct fix is to treat a probe
        // failure as an UNRELIABLE fingerprint and have the caller skip the
        // compile cache for that operation (a cache-contract change), not to emit
        // a stable token here.
        Ok(Ok(output)) => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            warn!(
                probe = probe.name,
                exit_code = output.status.code(),
                stderr = %stderr,
                "toolchain probe exited non-zero"
            );
            "unavailable".to_string()
        }
        Ok(Err(e)) => {
            warn!(probe = probe.name, error = %e, "toolchain probe failed to spawn");
            "unavailable".to_string()
        }
        Err(_) => {
            warn!(probe = probe.name, "toolchain probe timed out");
            "unavailable".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn compute_returns_nonempty_for_empty_probe_list() {
        let fp = compute(&[]).await;
        assert!(!fp.is_empty());
        assert_eq!(fp.len(), 64);
    }

    #[tokio::test]
    async fn compute_is_deterministic_across_calls() {
        let probes = [
            Probe {
                name: "echo-a",
                command: "/bin/echo",
                args: &["a"],
            },
            Probe {
                name: "echo-b",
                command: "/bin/echo",
                args: &["b"],
            },
        ];
        let fp1 = compute(&probes).await;
        let fp2 = compute(&probes).await;
        assert_eq!(fp1, fp2);
    }

    #[tokio::test]
    async fn compute_is_order_independent() {
        let p1 = [
            Probe {
                name: "a",
                command: "/bin/echo",
                args: &["1"],
            },
            Probe {
                name: "b",
                command: "/bin/echo",
                args: &["2"],
            },
        ];
        let p2 = [
            Probe {
                name: "b",
                command: "/bin/echo",
                args: &["2"],
            },
            Probe {
                name: "a",
                command: "/bin/echo",
                args: &["1"],
            },
        ];
        assert_eq!(compute(&p1).await, compute(&p2).await);
    }

    #[tokio::test]
    async fn compute_differs_for_different_outputs() {
        let p1 = [Probe {
            name: "x",
            command: "/bin/echo",
            args: &["v1"],
        }];
        let p2 = [Probe {
            name: "x",
            command: "/bin/echo",
            args: &["v2"],
        }];
        assert_ne!(compute(&p1).await, compute(&p2).await);
    }

    #[tokio::test]
    async fn compute_handles_missing_binary() {
        let probes = [Probe {
            name: "nonexistent",
            command: "/path/that/does/not/exist/never-ever-binary",
            args: &[],
        }];
        let fp = compute(&probes).await;
        assert!(!fp.is_empty());
    }

    #[tokio::test]
    async fn compute_treats_missing_binary_as_stable_unavailable() {
        let missing = [Probe {
            name: "x",
            command: "/no-such-binary-a",
            args: &[],
        }];
        let also_missing = [Probe {
            name: "x",
            command: "/no-such-binary-b",
            args: &[],
        }];
        assert_eq!(compute(&missing).await, compute(&also_missing).await);
    }
}
