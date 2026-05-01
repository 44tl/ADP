use serde::{Deserialize, Serialize};
use std::io::{self, Read};

#[derive(Debug, Deserialize)]
pub struct ArchitectInput {
    pub requirements: String,
    #[serde(default)]
    pub constraints: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Adr {
    pub title: String,
    pub status: String,
    pub context: String,
    pub decision: String,
    pub consequences: String,
}

#[derive(Debug, Serialize)]
pub struct Module {
    pub name: String,
    pub responsibilities: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ArchitectOutput {
    pub adr: Adr,
    pub modules: Vec<Module>,
}

#[no_mangle]
pub extern "C" fn _start() {
    // Read input from the host runtime via stdin
    let mut input_str = String::new();
    if io::stdin().read_to_string(&mut input_str).is_err() {
        return;
    }

    let input: ArchitectInput = match serde_json::from_str(&input_str) {
        Ok(i) => i,
        Err(_) => return,
    };

    // Construct the output
    let output = ArchitectOutput {
        adr: Adr {
            title: format!("ADR-0042: {}", input.requirements),
            status: "proposed".to_string(),
            context: "Initial system design based on input requirements".to_string(),
            decision: "Select a local-first, zero-cloud architecture".to_string(),
            consequences: "Eliminates network latency, enhances privacy".to_string(),
        },
        modules: vec![
            Module {
                name: "Core".to_string(),
                responsibilities: vec!["State machine initialization".to_string()],
            },
            Module {
                name: "Router".to_string(),
                responsibilities: vec!["Local LLM Inference orchestration".to_string()],
            },
        ],
    };

    if let Ok(json) = serde_json::to_string(&output) {
        println!("{}", json);
    }
}
