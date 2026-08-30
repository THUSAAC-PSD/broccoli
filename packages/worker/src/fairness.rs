#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FairnessMode {
    Strict,
    Pinned,
    Clamped,
    Unsafe,
}

impl FairnessMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Strict | Self::Clamped => "strict",
            Self::Pinned => "pinned",
            Self::Unsafe => "unsafe",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkerCapacity {
    pub max_concurrency: usize,
    pub fairness_mode: FairnessMode,
    pub was_clamped: bool,
}

pub fn effective_worker_capacity(
    requested_max_concurrency: u32,
    fairness_unsafe_allow: bool,
    os: &str,
    is_wsl: bool,
) -> WorkerCapacity {
    let requested = requested_max_concurrency.max(1);
    let platform_needs_single_slot = os != "linux" || is_wsl;

    if requested > 1 && platform_needs_single_slot && !fairness_unsafe_allow {
        return WorkerCapacity {
            max_concurrency: 1,
            fairness_mode: FairnessMode::Clamped,
            was_clamped: true,
        };
    }

    let fairness_mode = if requested > 1 && platform_needs_single_slot {
        FairnessMode::Unsafe
    } else if requested > 1 {
        FairnessMode::Pinned
    } else {
        FairnessMode::Strict
    };

    WorkerCapacity {
        max_concurrency: requested as usize,
        fairness_mode,
        was_clamped: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_capacity_keeps_linux_concurrency_native() {
        let capacity = effective_worker_capacity(4, false, "linux", false);

        assert_eq!(capacity.max_concurrency, 4);
        assert_eq!(capacity.fairness_mode.as_str(), "pinned");
        assert!(!capacity.was_clamped);
    }

    #[test]
    fn effective_capacity_clamps_non_linux_without_override() {
        let capacity = effective_worker_capacity(4, false, "macos", false);

        assert_eq!(capacity.max_concurrency, 1);
        assert_eq!(capacity.fairness_mode.as_str(), "strict");
        assert!(capacity.was_clamped);
    }

    #[test]
    fn effective_capacity_clamps_wsl_without_override() {
        let capacity = effective_worker_capacity(3, false, "linux", true);

        assert_eq!(capacity.max_concurrency, 1);
        assert_eq!(capacity.fairness_mode.as_str(), "strict");
        assert!(capacity.was_clamped);
    }

    #[test]
    fn effective_capacity_allows_explicit_unsafe_override() {
        let capacity = effective_worker_capacity(3, true, "macos", false);

        assert_eq!(capacity.max_concurrency, 3);
        assert_eq!(capacity.fairness_mode.as_str(), "unsafe");
        assert!(!capacity.was_clamped);
    }

    #[test]
    fn requested_zero_is_treated_as_one() {
        let capacity = effective_worker_capacity(0, false, "linux", false);

        assert_eq!(capacity.max_concurrency, 1);
        assert_eq!(capacity.fairness_mode.as_str(), "strict");
        assert!(!capacity.was_clamped);
    }
}
