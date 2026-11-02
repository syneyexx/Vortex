//! JSON-schema-subset: validatie + milde coercie (W08.02). Ondersteunt type (ook lijst),
//! properties, required, additionalProperties, enum, const, items, minItems/maxItems,
//! minimum/maximum, minLength/maxLength, oneOf/anyOf, default.
//! Coercie: "5" → 5 bij integer, "true" → true bij boolean, los item → [item] bij array.

use serde_json::Value;

fn type_ok(t: &str, v: &Value) -> bool {
    match t {
        "object" => v.is_object(),
        "array" => v.is_array(),
        "string" => v.is_string(),
        "integer" => v.as_i64().is_some() || v.as_u64().is_some() || v.as_f64().is_some_and(|f| f.fract() == 0.0),
        "number" => v.is_number(),
        "boolean" => v.is_boolean(),
        "null" => v.is_null(),
        _ => true,
    }
}

fn types(schema: &Value) -> Vec<String> {
    match schema.get("type") {
        Some(Value::String(s)) => vec![s.clone()],
        Some(Value::Array(a)) => a.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
        _ => vec![],
    }
}

/// Valideer `v` tegen `schema`. Geeft alle fouten met pad.
pub fn validate(schema: &Value, v: &Value) -> Result<(), Vec<String>> {
    let mut errs = Vec::new();
    walk(schema, v, "$", &mut errs);
    if errs.is_empty() { Ok(()) } else { Err(errs) }
}

fn walk(s: &Value, v: &Value, path: &str, errs: &mut Vec<String>) {
    if !s.is_object() {
        return;
    }
    let ts = types(s);
    if !ts.is_empty() && !ts.iter().any(|t| type_ok(t, v)) {
        errs.push(format!("{path}: verwacht {}, kreeg {}", ts.join("|"), kind(v)));
        return;
    }
    if let Some(Value::Array(opts)) = s.get("enum")
        && !opts.contains(v)
    {
        errs.push(format!("{path}: moet een van {} zijn", Value::Array(opts.clone())));
    }
    if let Some(c) = s.get("const")
        && c != v
    {
        errs.push(format!("{path}: moet {c} zijn"));
    }
    for key in ["anyOf", "oneOf"] {
        if let Some(Value::Array(alts)) = s.get(key) {
            let n = alts.iter().filter(|a| validate(a, v).is_ok()).count();
            if n == 0 || (key == "oneOf" && n > 1) {
                errs.push(format!("{path}: past niet op {key}"));
            }
        }
    }
    match v {
        Value::Object(m) => {
            let props = s.get("properties").and_then(|p| p.as_object());
            if let Some(Value::Array(req)) = s.get("required") {
                for r in req.iter().filter_map(|r| r.as_str()) {
                    if !m.contains_key(r) {
                        errs.push(format!("{path}.{r}: verplicht veld ontbreekt"));
                    }
                }
            }
            for (k, val) in m {
                match props.and_then(|p| p.get(k)) {
                    Some(ps) => walk(ps, val, &format!("{path}.{k}"), errs),
                    None => {
                        if s.get("additionalProperties") == Some(&Value::Bool(false)) {
                            errs.push(format!("{path}.{k}: onbekend veld"));
                        } else if let Some(ap @ Value::Object(_)) = s.get("additionalProperties") {
                            walk(ap, val, &format!("{path}.{k}"), errs);
                        }
                    }
                }
            }
        }
        Value::Array(a) => {
            if let Some(n) = s.get("minItems").and_then(|x| x.as_u64())
                && (a.len() as u64) < n
            {
                errs.push(format!("{path}: minstens {n} items"));
            }
            if let Some(n) = s.get("maxItems").and_then(|x| x.as_u64())
                && (a.len() as u64) > n
            {
                errs.push(format!("{path}: hoogstens {n} items"));
            }
            if let Some(is) = s.get("items") {
                for (i, x) in a.iter().enumerate() {
                    walk(is, x, &format!("{path}[{i}]"), errs);
                }
            }
        }
        Value::String(st) => {
            let n = st.chars().count() as u64;
            if let Some(m) = s.get("minLength").and_then(|x| x.as_u64())
                && n < m
            {
                errs.push(format!("{path}: minstens {m} tekens"));
            }
            if let Some(m) = s.get("maxLength").and_then(|x| x.as_u64())
                && n > m
            {
                errs.push(format!("{path}: hoogstens {m} tekens"));
            }
        }
        Value::Number(num) => {
            let f = num.as_f64().unwrap_or(0.0);
            if let Some(m) = s.get("minimum").and_then(|x| x.as_f64())
                && f < m
            {
                errs.push(format!("{path}: minimaal {m}"));
            }
            if let Some(m) = s.get("maximum").and_then(|x| x.as_f64())
                && f > m
            {
                errs.push(format!("{path}: maximaal {m}"));
            }
        }
        _ => {}
    }
}

fn kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Milde coercie + defaults invullen. Daarna altijd nog [`validate`].
pub fn coerce(s: &Value, v: Value) -> Value {
    let ts = types(s);
    let want = |t: &str| ts.iter().any(|x| x == t);
    let v = match v {
        Value::String(st) if want("integer") && !want("string") => {
            st.trim().parse::<i64>().map(Value::from).unwrap_or(Value::String(st))
        }
        Value::String(st) if want("number") && !want("string") => st
            .trim()
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number)
            .unwrap_or(Value::String(st)),
        Value::String(st) if want("boolean") && !want("string") => match st.trim().to_lowercase().as_str() {
            "true" | "ja" | "yes" | "1" => Value::Bool(true),
            "false" | "nee" | "no" | "0" => Value::Bool(false),
            _ => Value::String(st),
        },
        // Model stuurt object als JSON-string.
        Value::String(st) if (want("object") || want("array")) && !want("string") => {
            match serde_json::from_str::<Value>(&st) {
                Ok(v) if v.is_object() || v.is_array() => v,
                _ if want("array") => Value::Array(vec![Value::String(st)]),
                _ => Value::String(st),
            }
        }
        other if want("array") && !other.is_array() && !other.is_null() => Value::Array(vec![other]),
        other => other,
    };
    match v {
        Value::Object(mut m) => {
            if let Some(props) = s.get("properties").and_then(|p| p.as_object()) {
                for (k, ps) in props {
                    match m.remove(k) {
                        Some(x) => {
                            m.insert(k.clone(), coerce(ps, x));
                        }
                        None => {
                            if let Some(d) = ps.get("default") {
                                m.insert(k.clone(), d.clone());
                            }
                        }
                    }
                }
            }
            Value::Object(m)
        }
        Value::Array(a) => match s.get("items") {
            Some(is) => Value::Array(a.into_iter().map(|x| coerce(is, x)).collect()),
            None => Value::Array(a),
        },
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn schema() -> Value {
        json!({"type": "object", "required": ["path"], "additionalProperties": false, "properties": {
            "path": {"type": "string", "minLength": 1},
            "limit": {"type": "integer", "minimum": 1, "default": 100},
            "mode": {"type": "string", "enum": ["a", "b"]},
            "tags": {"type": "array", "items": {"type": "string"}}
        }})
    }

    #[test]
    fn validates_with_paths() {
        assert!(validate(&schema(), &json!({"path": "x"})).is_ok());
        let e = validate(&schema(), &json!({"limit": 0, "mode": "c", "extra": 1})).unwrap_err();
        let all = e.join("\n");
        assert!(all.contains("$.path: verplicht"));
        assert!(all.contains("$.limit: minimaal 1"));
        assert!(all.contains("$.mode: moet een van"));
        assert!(all.contains("$.extra: onbekend veld"));
    }

    #[test]
    fn coerces_common_model_mistakes() {
        let v = coerce(&schema(), json!({"path": "x", "limit": "5", "tags": "één"}));
        assert_eq!(v, json!({"path": "x", "limit": 5, "tags": ["één"]}));
        let v = coerce(&schema(), json!({"path": "x"}));
        assert_eq!(v["limit"], 100);
        assert!(validate(&schema(), &v).is_ok());
    }
}
