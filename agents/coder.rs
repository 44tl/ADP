use serde::{Deserialize, Serialize};
use std::io::{self, Read};

#[derive(Debug, Deserialize)]
pub struct CoderInput {
    pub files: Vec<String>,
    pub requirements: String,
}

#[derive(Debug, Serialize)]
pub struct CoderOutput {
    pub patches: Vec<Patch>,
}

#[derive(Debug, Serialize)]
pub struct Patch {
    pub file: String,
    pub diff: String,
}

#[no_mangle]
pub extern "C" fn _start() {
    let mut input_str = String::new();
    if io::stdin().read_to_string(&mut input_str).is_err() {
        return;
    }

    let input: CoderInput = match serde_json::from_str(&input_str) {
        Ok(i) => i,
        Err(_) => return,
    };

    let mut patches = Vec::new();
    for file in input.files {
        let diff = format!(
            "--- a/{0}\n+++ b/{0}\n@@ -1,3 +1,4 @@\n+// Added requirements: {1}",
            file, input.requirements
        );
        patches.push(Patch { file, diff });
    }

    let output = CoderOutput { patches };

    if let Ok(json) = serde_json::to_string(&output) {
        println!("{}", json);
    }
}
