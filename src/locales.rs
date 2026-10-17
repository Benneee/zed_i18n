use anyhow::Result;
use serde_json::Value;
use std::collections::BTreeMap;

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
