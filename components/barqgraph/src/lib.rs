pub struct Client;
impl Client {
    pub fn new(_url: &str) -> Self { Client }
    pub fn store_relationship(&self, _path: &str, _imports: Vec<String>) {}
    pub fn neighbors(&self, _symbol: &str) -> Vec<String> { 
        vec!["main".to_string()] 
    }
}
