pub struct Client;
impl Client {
    pub fn new(_url: &str) -> Self { Client }
    pub fn store(&self, _path: &str, _line: usize, _lang: &str, _content: &str) {}
    pub fn knn_search(&self, _q: &str, _top_k: usize) -> Vec<(String, String, f32, usize)> { 
        vec![("testdata/sample.rs".to_string(), "fn main() {}".to_string(), 0.9, 1)] 
    }
}
