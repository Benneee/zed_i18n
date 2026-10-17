use anyhow::Result;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub fn load_locale_file(path: &str) -> Result<BTreeMap<String, String>> {
    let text = fs::read_to_string(path)?;
    process_json(&text)
}

pub fn load_file_name(path: &str) -> Option<String> {
    let file_path = Path::new(path);
    match file_path.file_stem() {
        Some(file_name_text) => {
            eprintln!("{:?}", file_name_text);
            Some(file_name_text.to_string_lossy().to_string())
        }
        None => {
            eprintln!("Nothing to see here");
            None
        }
    }
}

pub fn process_json(copies_json: &str) -> Result<BTreeMap<String, String>> {
    // Parse the JSON string
    let copy_text: Value = serde_json::from_str(copies_json)?;

    // Now, we flatten the JSON structure into the BTreeMap with the outer key hardcoded for now
    if copy_text.is_object() {
        // Empty table to be filled with flat key -> text pairs
        let mut table: BTreeMap<String, String> = BTreeMap::new();
        flatten_recursive(&copy_text, String::new(), &mut table);
        Ok(table)
    } else {
        anyhow::bail!("locale file isn't a JSON object, skipping it");
    }
}

fn flatten_recursive(current: &Value, prefix: String, table: &mut BTreeMap<String, String>) {
    match current {
        Value::Object(obj) => {
            for (key, value) in obj {
                let new_prefix = if prefix.is_empty() {
                    key.clone()
                } else {
                    // key dotted together -> value
                    format!("{}.{}", prefix, key)
                };
                // In case the object is nested
                flatten_recursive(value, new_prefix, table);
            }
        }
        Value::String(text) => {
            table.insert(prefix, text.clone());
        }
        _ => {}
    }
}
