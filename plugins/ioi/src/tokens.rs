use serde::{Deserialize, Serialize};

use crate::config::{TokenConfig, TokenMode};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenState {
    #[serde(default)]
    pub used: u32,
    #[serde(default)]
    pub tokened_submission_ids: Vec<i32>,
}

/// Compute the number of tokens currently available.
///
/// - `TokenMode::None` -> `u32::MAX` (tokens disabled, unlimited)
/// - `FixedBudget` -> `initial - used`
/// - `Regenerating` -> `min(initial + elapsed/interval, max) - used`
pub fn available_tokens(config: &TokenConfig, state: &TokenState, elapsed_minutes: u64) -> u32 {
    match config.mode {
        TokenMode::None => u32::MAX,
        TokenMode::FixedBudget => config.initial.saturating_sub(state.used),
        TokenMode::Regenerating => {
            let regen_interval = config.regen_interval_min.max(1) as u64;
            let regenerated = elapsed_minutes / regen_interval;
            let total = (config.initial as u64 + regenerated).min(config.max as u64) as u32;
            total.saturating_sub(state.used)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed_config(initial: u32) -> TokenConfig {
        TokenConfig {
            mode: TokenMode::FixedBudget,
            initial,
            max: initial,
            regen_interval_min: 30,
        }
    }

    fn regen_config(initial: u32, max: u32, interval: u32) -> TokenConfig {
        TokenConfig {
            mode: TokenMode::Regenerating,
            initial,
            max,
            regen_interval_min: interval,
        }
    }

    #[test]
    fn none_unlimited() {
        let config = TokenConfig { mode: TokenMode::None, ..Default::default() };
        let state = TokenState::default();
        assert_eq!(available_tokens(&config, &state, 0), u32::MAX);
    }

    #[test]
    fn fixed_initial() {
        let config = fixed_config(5);
        let state = TokenState { used: 2, ..Default::default() };
        assert_eq!(available_tokens(&config, &state, 0), 3);
    }

    #[test]
    fn fixed_exhausted() {
        let config = fixed_config(3);
        let state = TokenState { used: 3, ..Default::default() };
        assert_eq!(available_tokens(&config, &state, 0), 0);
    }

    #[test]
    fn regen_initial() {
        let config = regen_config(2, 10, 30);
        let state = TokenState::default();
        assert_eq!(available_tokens(&config, &state, 0), 2);
    }

    #[test]
    fn regen_after_interval() {
        let config = regen_config(2, 10, 30);
        let state = TokenState::default();
        // After 60 minutes: 2 + 60/30 = 4
        assert_eq!(available_tokens(&config, &state, 60), 4);
    }

    #[test]
    fn regen_capped() {
        let config = regen_config(2, 5, 30);
        let state = TokenState::default();
        // After 300 min: 2 + 10 = 12, capped at 5
        assert_eq!(available_tokens(&config, &state, 300), 5);
    }

    #[test]
    fn regen_with_used() {
        let config = regen_config(2, 10, 30);
        let state = TokenState { used: 3, ..Default::default() };
        // After 60 min: min(2 + 2, 10) - 3 = 1
        assert_eq!(available_tokens(&config, &state, 60), 1);
    }
}
