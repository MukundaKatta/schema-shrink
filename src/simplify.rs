use serde_json::{Map, Value};

/// Options controlling [`simplify`].
#[derive(Debug, Clone)]
pub struct SimplifyOptions {
    /// Replace `type: ["X", "null"]` with `type: "X"`. Defaults to `true`.
    /// Caller must then post-process nulls into their own application logic.
    pub strip_null_unions: bool,
    /// Cap enums of strings at this many values. Values beyond are dropped.
    /// `None` disables the cap. Defaults to `Some(20)`.
    pub max_enum_size: Option<usize>,
    /// Drop `additionalProperties: false` constraint, which is also expensive
    /// in strict mode. Defaults to `false` (preserves constraint).
    pub drop_additional_properties_false: bool,
}

impl Default for SimplifyOptions {
    fn default() -> Self {
        Self {
            strip_null_unions: true,
            max_enum_size: Some(20),
            drop_additional_properties_false: false,
        }
    }
}

/// Apply documented Anthropic strict-mode workarounds to a JSON Schema.
///
/// Returns a new `Value`. The original is not mutated.
pub fn simplify(schema: Value, opts: SimplifyOptions) -> Value {
    walk(schema, &opts)
}

fn walk(node: Value, opts: &SimplifyOptions) -> Value {
    match node {
        Value::Object(map) => Value::Object(walk_obj(map, opts)),
        Value::Array(arr) => Value::Array(arr.into_iter().map(|v| walk(v, opts)).collect()),
        other => other,
    }
}

fn walk_obj(mut map: Map<String, Value>, opts: &SimplifyOptions) -> Map<String, Value> {
    // type: ["X", "null"] -> "X"
    if opts.strip_null_unions {
        if let Some(type_val) = map.get_mut("type") {
            if let Some(arr) = type_val.as_array() {
                let non_null: Vec<&str> = arr
                    .iter()
                    .filter_map(|v| v.as_str())
                    .filter(|s| *s != "null")
                    .collect();
                if non_null.len() == 1 && arr.iter().any(|v| v.as_str() == Some("null")) {
                    *type_val = Value::String(non_null[0].to_string());
                }
            }
        }
    }

    // enum cap
    if let Some(cap) = opts.max_enum_size {
        if let Some(enum_val) = map.get_mut("enum") {
            if let Some(arr) = enum_val.as_array_mut() {
                if arr.len() > cap {
                    arr.truncate(cap);
                }
            }
        }
    }

    // additionalProperties: false drop
    if opts.drop_additional_properties_false {
        if let Some(addl) = map.get("additionalProperties") {
            if addl == &Value::Bool(false) {
                map.remove("additionalProperties");
            }
        }
    }

    // Recurse into nested schema-bearing keys.
    for key in ["properties", "patternProperties", "$defs", "definitions"] {
        if let Some(Value::Object(props)) = map.remove(key) {
            let new_props: Map<String, Value> =
                props.into_iter().map(|(k, v)| (k, walk(v, opts))).collect();
            map.insert(key.to_string(), Value::Object(new_props));
        }
    }
    if let Some(items) = map.remove("items") {
        map.insert("items".to_string(), walk(items, opts));
    }
    // `additionalProperties` is itself a subschema, so recurse into it (when it
    // is an object). A boolean value is left untouched here; the `false`-drop
    // handling above already dealt with the only boolean case we rewrite.
    if let Some(addl) = map.remove("additionalProperties") {
        let rewritten = if addl.is_object() {
            walk(addl, opts)
        } else {
            addl
        };
        map.insert("additionalProperties".to_string(), rewritten);
    }
    for keyword in ["anyOf", "oneOf", "allOf"] {
        if let Some(Value::Array(arr)) = map.remove(keyword) {
            let new_arr: Vec<Value> = arr.into_iter().map(|v| walk(v, opts)).collect();
            map.insert(keyword.to_string(), Value::Array(new_arr));
        }
    }

    map
}
