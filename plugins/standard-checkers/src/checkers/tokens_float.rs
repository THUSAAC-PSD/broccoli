use broccoli_server_sdk::types::*;

use serde::Deserialize;

use crate::util::{token_count_msg, token_mismatch_msg, tokenize, truncate};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FloatConfig {
    #[serde(default = "default_abs_tol")]
    abs_tol: f64,
    #[serde(default = "default_rel_tol")]
    rel_tol: f64,
}

impl Default for FloatConfig {
    fn default() -> Self {
        Self {
            abs_tol: default_abs_tol(),
            rel_tol: default_rel_tol(),
        }
    }
}

fn default_abs_tol() -> f64 {
    1e-9
}
fn default_rel_tol() -> f64 {
    1e-6
}

/// Combined absolute + relative float tolerance comparison.
/// For each token pair:
/// - Both parse as finite f64: accept if |a-b| <= max(abs_tol, rel_tol * max(|a|, |b|))
/// - Both are the same non-finite value: accept
/// - Neither parses as f64: string-compare (supports mixed output)
/// Defaults: abs_tol = 1e-9, rel_tol = 1e-6.
/// Reads overrides from config: {"abs_tol": 1e-4, "rel_tol": 1e-3}
pub fn check(req: &CheckerParseInput) -> Result<CheckerVerdict, String> {
    let cfg: FloatConfig = req
        .config
        .as_ref()
        .map(|v| serde_json::from_value(v.clone()))
        .transpose()
        .map_err(|e| format!("Invalid checker config: {e}"))?
        .unwrap_or_default();

    let expected = tokenize(&req.expected_output);
    let actual = tokenize(&req.stdout);

    if expected.len() != actual.len() {
        return Ok(CheckerVerdict {
            verdict: Verdict::WrongAnswer,
            score: 0.0,
            message: Some(token_count_msg(expected.len(), actual.len())),
        });
    }

    for (i, (exp, act)) in expected.iter().zip(actual.iter()).enumerate() {
        match (exp.parse::<f64>(), act.parse::<f64>()) {
            (Ok(exp_f), Ok(act_f)) => {
                if !exp_f.is_finite() || !act_f.is_finite() {
                    // Both must be the same non-finite value
                    if exp_f.is_nan() && act_f.is_nan() {
                        continue;
                    }
                    if exp_f == act_f {
                        continue;
                    }
                    return Ok(CheckerVerdict {
                        verdict: Verdict::WrongAnswer,
                        score: 0.0,
                        message: Some(format!(
                            "Non-finite mismatch at token {}: expected {}, got {}",
                            i + 1,
                            truncate(exp, 50),
                            truncate(act, 50)
                        )),
                    });
                }

                let diff = (exp_f - act_f).abs();
                let tolerance = cfg.abs_tol.max(cfg.rel_tol * exp_f.abs().max(act_f.abs()));

                if diff > tolerance {
                    return Ok(CheckerVerdict {
                        verdict: Verdict::WrongAnswer,
                        score: 0.0,
                        message: Some(format!(
                            "Float mismatch at token {}: expected {}, got {}, diff {} > tolerance {}",
                            i + 1,
                            exp,
                            act,
                            diff,
                            tolerance
                        )),
                    });
                }
            }
            _ => {
                // At least one doesn't parse as float — compare as strings
                if exp != act {
                    return Ok(CheckerVerdict {
                        verdict: Verdict::WrongAnswer,
                        score: 0.0,
                        message: Some(token_mismatch_msg(i + 1, exp, act)),
                    });
                }
            }
        }
    }

    Ok(CheckerVerdict {
        verdict: Verdict::Accepted,
        score: 1.0,
        message: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkers::{input, input_with_config};

    #[test]
    fn exact_match() {
        let req = input("3.14159", "3.14159");
        let v = check(&req).unwrap();
        assert_eq!(v.verdict, Verdict::Accepted);
    }

    #[test]
    fn within_abs_tolerance() {
        // Default abs_tol = 1e-9
        let req = input("1.0000000001", "1.0000000000");
        let v = check(&req).unwrap();
        assert_eq!(v.verdict, Verdict::Accepted);
    }

    #[test]
    fn within_rel_tolerance() {
        // Default rel_tol = 1e-6, value ~1000 → tolerance ~0.001
        let req = input("1000.0005", "1000.0000");
        let v = check(&req).unwrap();
        assert_eq!(v.verdict, Verdict::Accepted);
    }

    #[test]
    fn outside_both_tolerances() {
        let req = input("1.0", "2.0");
        let v = check(&req).unwrap();
        assert_eq!(v.verdict, Verdict::WrongAnswer);
    }

    #[test]
    fn nan_vs_nan() {
        let req = input("NaN", "NaN");
        let v = check(&req).unwrap();
        assert_eq!(v.verdict, Verdict::Accepted);
    }

    #[test]
    fn inf_vs_inf() {
        let req = input("inf", "inf");
        let v = check(&req).unwrap();
        assert_eq!(v.verdict, Verdict::Accepted);
    }

    #[test]
    fn mixed_float_string_tokens() {
        let req = input("YES 3.14", "YES 3.14");
        let v = check(&req).unwrap();
        assert_eq!(v.verdict, Verdict::Accepted);
    }

    #[test]
    fn custom_config_overrides_defaults() {
        // With abs_tol = 1.0, a diff of 0.5 should be accepted
        let req = input_with_config("1.5", "1.0", serde_json::json!({"abs_tol": 1.0}));
        let v = check(&req).unwrap();
        assert_eq!(v.verdict, Verdict::Accepted);
    }
}
