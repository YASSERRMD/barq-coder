pub struct BenchResult {
    pub name: String,
    pub before_ns: u64,
    pub after_ns: u64,
    pub pct_change: f64,
}

pub fn compare_benches(_current: &str, _baseline: &str) -> Vec<BenchResult> {
    // Mock diff
    vec![]
}
