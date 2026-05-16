use serde::Serialize;
use serde_json::Value;

/// One expensive feature found in a schema.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExpensiveFeature {
    /// A `type` field with `null` in it (e.g. `["string", "null"]`).
    /// Anthropic docs cap these at 16 total per strict schema.
    NullableUnion {
        /// JSON pointer to the offending field.
        pointer: String,
    },
    /// An `enum` of strings with `count` values; large enums inflate the grammar.
    LargeEnum {
        /// JSON pointer to the offending field.
        pointer: String,
        /// Number of enum values.
        count: usize,
    },
    /// Nesting depth exceeded the analyzer's threshold.
    DeepNesting {
        /// JSON pointer to the deepest path found.
        pointer: String,
        /// Depth, counted from the root.
        depth: usize,
    },
    /// `anyOf`/`oneOf`/`allOf` with many branches; each branch multiplies grammar paths.
    AnyOfWithMany {
        /// JSON pointer to the offending field.
        pointer: String,
        /// Number of branches.
        branches: usize,
    },
}

/// Analysis result for one schema.
#[derive(Debug, Clone, Serialize)]
pub struct Analysis {
    /// Rough estimate of compiled-grammar size in bytes (heuristic).
    pub estimated_grammar_size: usize,
    /// `true` if the heuristic suggests this schema is likely to hit the
    /// Anthropic strict-mode grammar limit.
    pub likely_too_large: bool,
    /// Total count of `["string","null"]`-style nullable unions.
    pub nullable_union_count: usize,
    /// All discovered expensive features in document order.
    pub expensive_features: Vec<ExpensiveFeature>,
}

/// Heuristic threshold above which we flag `likely_too_large = true`.
/// Tuned to roughly correspond to the threshold at which the Anthropic
/// compiled-grammar error starts firing in practice. Adjust upstream.
const TOO_LARGE_THRESHOLD: usize = 10_000;
const DEEP_NESTING_THRESHOLD: usize = 5;
const LARGE_ENUM_THRESHOLD: usize = 20;
const ANYOF_MANY_THRESHOLD: usize = 8;
const NULLABLE_UNION_HARD_LIMIT: usize = 16;

/// Walk a schema and return an [`Analysis`].
pub fn analyze(schema: &Value) -> Analysis {
    let mut a = Analysis {
        estimated_grammar_size: estimate_grammar_size(schema),
        likely_too_large: false,
        nullable_union_count: 0,
        expensive_features: Vec::new(),
    };
    walk(schema, "", 0, &mut a);
    a.likely_too_large = a.estimated_grammar_size > TOO_LARGE_THRESHOLD
        || a.nullable_union_count > NULLABLE_UNION_HARD_LIMIT;
    a
}

fn walk(node: &Value, pointer: &str, depth: usize, a: &mut Analysis) {
    if depth >= DEEP_NESTING_THRESHOLD {
        a.expensive_features.push(ExpensiveFeature::DeepNesting {
            pointer: pointer.to_string(),
            depth,
        });
    }

    if let Some(obj) = node.as_object() {
        // type: ["string","null"] etc.
        if let Some(type_val) = obj.get("type") {
            if let Some(arr) = type_val.as_array() {
                if arr.iter().any(|v| v.as_str() == Some("null")) && arr.len() > 1 {
                    a.nullable_union_count += 1;
                    a.expensive_features.push(ExpensiveFeature::NullableUnion {
                        pointer: pointer.to_string(),
                    });
                }
            }
        }

        // enum
        if let Some(enum_val) = obj.get("enum") {
            if let Some(arr) = enum_val.as_array() {
                if arr.len() >= LARGE_ENUM_THRESHOLD {
                    a.expensive_features.push(ExpensiveFeature::LargeEnum {
                        pointer: pointer.to_string(),
                        count: arr.len(),
                    });
                }
            }
        }

        for keyword in ["anyOf", "oneOf", "allOf"] {
            if let Some(arr) = obj.get(keyword).and_then(|v| v.as_array()) {
                if arr.len() >= ANYOF_MANY_THRESHOLD {
                    a.expensive_features.push(ExpensiveFeature::AnyOfWithMany {
                        pointer: pointer.to_string(),
                        branches: arr.len(),
                    });
                }
                for (i, branch) in arr.iter().enumerate() {
                    walk(branch, &format!("{pointer}/{keyword}/{i}"), depth + 1, a);
                }
            }
        }

        // Recurse into known schema-bearing keys.
        for key in ["properties", "patternProperties", "$defs", "definitions"] {
            if let Some(props) = obj.get(key).and_then(|v| v.as_object()) {
                for (k, v) in props {
                    walk(v, &format!("{pointer}/{key}/{k}"), depth + 1, a);
                }
            }
        }
        if let Some(items) = obj.get("items") {
            walk(items, &format!("{pointer}/items"), depth + 1, a);
        }
        if let Some(addl) = obj.get("additionalProperties").and_then(|v| v.as_object()) {
            for (k, v) in addl {
                walk(v, &format!("{pointer}/additionalProperties/{k}"), depth + 1, a);
            }
        }
    }
}

/// Rough byte-size estimate of the schema as a proxy for compiled-grammar size.
fn estimate_grammar_size(schema: &Value) -> usize {
    serde_json::to_string(schema).map(|s| s.len()).unwrap_or(0)
}
