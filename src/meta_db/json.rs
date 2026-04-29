use color_eyre::eyre;

pub(super) fn insert_nested(
    root: &mut serde_json::Map<String, serde_json::Value>,
    path: &str,
    value: serde_json::Value,
) -> eyre::Result<()> {
    let mut parts = path.split("/").peekable();
    let mut current = root;

    while let Some(part) = parts.next() {
        if parts.peek().is_none() {
            if let Some(existing) = current.get(part) {
                if existing.is_object() && !value.is_object() {
                    return Err(eyre::eyre!(
                        "insert_nested: namespace conflict at '{}'. Trying to overwrite an object with a primitive value",
                        part
                    ));
                }
            }

            current.insert(part.to_string(), value);
            return Ok(());
        }

        current = current
            .entry(part.to_string())
            .or_insert_with(|| serde_json::Value::Object(Default::default()))
            .as_object_mut()
            .ok_or_else(|| eyre::eyre!("insert_nested: namespace conflict at '{}'. Expected an object, but found a primitive value. Did you mix a key and a prefix?", part))?;
    }

    Ok(())
}

pub(super) fn flatten_json(
    prefix: &str,
    value: serde_json::Value,
    out: &mut Vec<(String, serde_json::Value)>,
) {
    match value {
        serde_json::Value::Object(o) => {
            for (k, v) in o {
                let k_prefix = format!("{}{}/", prefix, k);
                flatten_json(&k_prefix, v, out);
            }
        }
        _ => {
            let key = prefix.trim_end_matches("/").to_string();
            out.push((key, value.clone()));
        }
    }
}
