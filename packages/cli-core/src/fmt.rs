pub fn memory_kb(kb: i64) -> String {
    const MB: f64 = 1024.0;
    const GB: f64 = 1024.0 * 1024.0;
    let kb_f = kb as f64;
    if kb_f >= GB {
        format!("{:.1} GB", kb_f / GB)
    } else if kb_f >= MB {
        format!("{:.1} MB", kb_f / MB)
    } else {
        format!("{} KB", kb)
    }
}

pub fn time_ms(ms: i64) -> String {
    if ms < 1000 {
        format!("{} ms", ms)
    } else if ms < 60_000 {
        format!("{:.2} s", ms as f64 / 1000.0)
    } else {
        let secs = ms / 1000;
        format!("{}m {:02}s", secs / 60, secs % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_scales_units() {
        assert_eq!(memory_kb(0), "0 KB");
        assert_eq!(memory_kb(512), "512 KB");
        assert_eq!(memory_kb(1024), "1.0 MB");
        assert_eq!(memory_kb(2048), "2.0 MB");
        assert_eq!(memory_kb(1024 * 1024), "1.0 GB");
        assert_eq!(memory_kb(1024 * 1024 + 512 * 1024), "1.5 GB");
    }

    #[test]
    fn time_scales_units() {
        assert_eq!(time_ms(0), "0 ms");
        assert_eq!(time_ms(41), "41 ms");
        assert_eq!(time_ms(999), "999 ms");
        assert_eq!(time_ms(1000), "1.00 s");
        assert_eq!(time_ms(1234), "1.23 s");
        assert_eq!(time_ms(75_000), "1m 15s");
    }
}
