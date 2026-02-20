use barqcoder::barq::BarqIndex;
use barqcoder::config::Config;

#[test]
fn test_barq_index_empty_dir() {
    let config = Config::default();
    let index = BarqIndex::new(&config).unwrap();
    let res = index.index_repo("testdata/empty");
    assert!(res.is_ok());
}

#[test]
fn test_barq_query_returns_results() {
    let config = Config::default();
    let index = BarqIndex::new(&config).unwrap();
    index.index_repo("testdata").unwrap();
    let results = index.query("fn main", 10);
    assert!(!results.is_empty());
}

#[test]
fn test_barq_graph_deps() {
    let config = Config::default();
    let index = BarqIndex::new(&config).unwrap();
    index.index_repo("testdata").unwrap();
    let deps = index.graph_deps("main");
    assert!(!deps.is_empty());
}
