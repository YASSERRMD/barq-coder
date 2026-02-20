use super::compare::BenchResult;

pub fn detect_regression(results: &[BenchResult]) -> Vec<&BenchResult> {
    results.iter().filter(|r| r.pct_change > 10.0).collect()
}
