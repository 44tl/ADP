use serde::{Deserialize, Serialize};
use std::io::{self, Read};

#[derive(Debug, Deserialize)]
pub struct ResearcherInput {
    pub query: String,
    #[serde(default)]
    pub sources: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ResearcherOutput {
    pub summary: String,
    pub sources: Vec<String>,
    pub confidence: f32,
}

#[no_mangle]
pub extern "C" fn _start() {
    let mut input_str = String::new();
    if io::stdin().read_to_string(&mut input_str).is_err() {
        return;
    }

    let input: ResearcherInput = match serde_json::from_str(&input_str) {
        Ok(i) => i,
        Err(_) => return,
    };

    let summary = format!("Analyzed query for: {}", input.query);
    let output = ResearcherOutput {
        summary,
        sources: input.sources,
        confidence: 0.95,
    };

    if let Ok(json) = serde_json::to_string(&output) {
        println!("{}", json);
    }
}
