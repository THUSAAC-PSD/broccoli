use std::net::IpAddr;

use serde::{Deserialize, Serialize};
use tracing::warn;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub hostname: Option<String>,
    pub ip_addresses: Vec<String>,
    pub os: String,
    pub arch: String,
    pub cpu_count: u32,
    pub pid: u32,
}

impl SystemInfo {
    pub fn detect() -> Self {
        let hostname = match hostname::get() {
            Ok(h) => h.to_string_lossy().into_owned().into(),
            Err(e) => {
                warn!(error = %e, "Failed to read hostname");
                None
            }
        };

        let ip_addresses = list_ip_addresses();

        let cpu_count = std::thread::available_parallelism()
            .map(|n| n.get() as u32)
            .unwrap_or(0);

        Self {
            hostname,
            ip_addresses,
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            cpu_count,
            pid: std::process::id(),
        }
    }
}

pub fn is_wsl() -> bool {
    let proc_version = std::fs::read_to_string("/proc/version").unwrap_or_default();
    let wsl_env_present =
        std::env::var_os("WSL_DISTRO_NAME").is_some() || std::env::var_os("WSL_INTEROP").is_some();
    is_wsl_linux(std::env::consts::OS, &proc_version, wsl_env_present)
}

pub(crate) fn is_wsl_linux(os: &str, proc_version: &str, wsl_env_present: bool) -> bool {
    os == "linux" && (wsl_env_present || proc_version.to_ascii_lowercase().contains("microsoft"))
}

fn list_ip_addresses() -> Vec<String> {
    let ifaces = match local_ip_address::list_afinet_netifas() {
        Ok(v) => v,
        Err(e) => {
            warn!(error = %e, "Failed to enumerate network interfaces");
            return Vec::new();
        }
    };

    let mut out: Vec<String> = ifaces
        .into_iter()
        .filter_map(|(_name, ip)| {
            if is_useful_ip(&ip) {
                Some(ip.to_string())
            } else {
                None
            }
        })
        .collect();

    out.sort_by_key(|s| ip_sort_key(s));
    out.dedup();
    out
}

fn is_useful_ip(ip: &IpAddr) -> bool {
    if ip.is_loopback() || ip.is_unspecified() {
        return false;
    }
    match ip {
        IpAddr::V4(_) => true,
        IpAddr::V6(v6) => !v6.is_unicast_link_local() && !is_v6_link_local_or_unique(v6),
    }
}

fn is_v6_link_local_or_unique(v6: &std::net::Ipv6Addr) -> bool {
    let segs = v6.segments();
    let first = segs[0];
    (first & 0xffc0) == 0xfe80
}

fn ip_sort_key(s: &str) -> (u8, String) {
    if s.contains(':') {
        (1, s.to_string())
    } else {
        (0, s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn loopback_and_unspecified_are_filtered() {
        assert!(!is_useful_ip(&IpAddr::V4(Ipv4Addr::LOCALHOST)));
        assert!(!is_useful_ip(&IpAddr::V6(Ipv6Addr::LOCALHOST)));
        assert!(!is_useful_ip(&IpAddr::V4(Ipv4Addr::UNSPECIFIED)));
    }

    #[test]
    fn v6_link_local_is_filtered() {
        let ll: Ipv6Addr = "fe80::1".parse().unwrap();
        assert!(!is_useful_ip(&IpAddr::V6(ll)));
    }

    #[test]
    fn v4_lan_ip_is_kept() {
        assert!(is_useful_ip(&IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10))));
    }

    #[test]
    fn detect_returns_stable_fields() {
        let info = SystemInfo::detect();
        assert!(!info.os.is_empty());
        assert!(!info.arch.is_empty());
        assert!(info.pid > 0);
    }

    #[test]
    fn ip_sort_puts_v4_before_v6() {
        let mut ips = ["fe80::1".to_string(), "192.168.0.1".to_string()];
        ips.sort_by_key(|s| ip_sort_key(s));
        assert_eq!(ips[0], "192.168.0.1");
    }

    #[test]
    fn wsl_detection_is_linux_only_and_matches_proc_or_env() {
        assert!(is_wsl_linux(
            "linux",
            "Linux version 5.15.90.1-microsoft-standard-WSL2",
            false
        ));
        assert!(is_wsl_linux("linux", "Linux version 6.6.87", true));
        assert!(!is_wsl_linux("linux", "Linux version 6.6.87", false));
        assert!(!is_wsl_linux(
            "macos",
            "Linux version 5.15.90.1-microsoft-standard-WSL2",
            true
        ));
    }
}
