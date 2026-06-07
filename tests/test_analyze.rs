use schema_shrink::{analyze, simplify, ExpensiveFeature, SimplifyOptions};
use serde_json::json;

#[test]
fn finds_nullable_unions() {
    let s = json!({
        "type": "object",
        "properties": {
            "name": {"type": ["string", "null"]},
            "age":  {"type": ["integer", "null"]}
        }
    });
    let a = analyze(&s);
    assert_eq!(a.nullable_union_count, 2);
    assert_eq!(
        a.expensive_features
            .iter()
            .filter(|f| matches!(f, ExpensiveFeature::NullableUnion { .. }))
            .count(),
        2
    );
}

#[test]
fn flags_large_enums() {
    let huge: Vec<_> = (0..30).map(|i| format!("v{i}")).collect();
    let s = json!({"type": "string", "enum": huge});
    let a = analyze(&s);
    assert!(a
        .expensive_features
        .iter()
        .any(|f| matches!(f, ExpensiveFeature::LargeEnum { count: 30, .. })));
}

#[test]
fn flags_many_branches_anyof() {
    let branches: Vec<_> = (0..10).map(|i| json!({"const": format!("v{i}")})).collect();
    let s = json!({"anyOf": branches});
    let a = analyze(&s);
    assert!(a
        .expensive_features
        .iter()
        .any(|f| matches!(f, ExpensiveFeature::AnyOfWithMany { branches: 10, .. })));
}

#[test]
fn flags_deep_nesting() {
    // 6 levels of nesting via repeated `items`.
    let s = json!({
        "type": "array",
        "items": {"type": "array", "items": {"type": "array", "items": {"type": "array", "items": {"type": "array", "items": {"type": "string"}}}}}
    });
    let a = analyze(&s);
    assert!(a
        .expensive_features
        .iter()
        .any(|f| matches!(f, ExpensiveFeature::DeepNesting { .. })));
}

#[test]
fn likely_too_large_above_threshold() {
    // Build a schema with 20 nullable unions to cross the hard limit (16).
    let mut props = serde_json::Map::new();
    for i in 0..20 {
        props.insert(format!("f{i}"), json!({"type": ["string", "null"]}));
    }
    let s = json!({"type": "object", "properties": props});
    let a = analyze(&s);
    assert_eq!(a.nullable_union_count, 20);
    assert!(a.likely_too_large);
}

#[test]
fn empty_schema_is_fine() {
    let a = analyze(&json!({}));
    assert!(!a.likely_too_large);
    assert_eq!(a.nullable_union_count, 0);
    assert!(a.expensive_features.is_empty());
}

#[test]
fn simplify_strips_null_unions_by_default() {
    let s = json!({
        "type": "object",
        "properties": {
            "name": {"type": ["string", "null"]},
            "age":  {"type": ["integer", "null"]},
            "keep": {"type": "string"}
        }
    });
    let out = simplify(s, SimplifyOptions::default());
    assert_eq!(out["properties"]["name"]["type"], "string");
    assert_eq!(out["properties"]["age"]["type"], "integer");
    assert_eq!(out["properties"]["keep"]["type"], "string");
}

#[test]
fn simplify_caps_enum_size() {
    let huge: Vec<_> = (0..40).map(|i| format!("v{i}")).collect();
    let s = json!({"type": "string", "enum": huge});
    let out = simplify(
        s,
        SimplifyOptions {
            max_enum_size: Some(10),
            ..Default::default()
        },
    );
    assert_eq!(out["enum"].as_array().unwrap().len(), 10);
}

#[test]
fn simplify_drop_additional_properties_false() {
    let s = json!({
        "type": "object",
        "properties": {"a": {"type": "string"}},
        "additionalProperties": false
    });
    let out = simplify(
        s,
        SimplifyOptions {
            drop_additional_properties_false: true,
            ..Default::default()
        },
    );
    assert!(out.get("additionalProperties").is_none());
}

#[test]
fn simplify_keeps_additional_properties_false_by_default() {
    let s = json!({"type": "object", "additionalProperties": false});
    let out = simplify(s, SimplifyOptions::default());
    assert_eq!(out["additionalProperties"], false);
}

#[test]
fn analyze_walks_additional_properties_subschema() {
    // `additionalProperties` is itself a subschema (a map value type), not a
    // map of property-name -> schema. Expensive features inside it must be
    // discovered.
    let s = json!({
        "type": "object",
        "additionalProperties": {"type": ["string", "null"]}
    });
    let a = analyze(&s);
    assert_eq!(a.nullable_union_count, 1);
    assert!(a.expensive_features.iter().any(|f| matches!(
        f,
        ExpensiveFeature::NullableUnion { pointer } if pointer == "/additionalProperties"
    )));
}

#[test]
fn analyze_ignores_boolean_additional_properties() {
    let s = json!({"type": "object", "additionalProperties": false});
    let a = analyze(&s);
    assert_eq!(a.nullable_union_count, 0);
    assert!(a.expensive_features.is_empty());
}

#[test]
fn deep_nesting_flagged_once_not_per_descendant() {
    // 7 levels: well past the threshold. Should yield exactly one DeepNesting
    // feature (at the first node that crosses the threshold), not one per node.
    let s = json!({
        "type": "array",
        "items": {"type": "array", "items": {"type": "array", "items": {"type": "array",
        "items": {"type": "array", "items": {"type": "array", "items": {"type": "string"}}}}}}
    });
    let a = analyze(&s);
    let deep_count = a
        .expensive_features
        .iter()
        .filter(|f| matches!(f, ExpensiveFeature::DeepNesting { .. }))
        .count();
    assert_eq!(deep_count, 1, "expected a single DeepNesting feature");
}

#[test]
fn simplify_recurses_into_additional_properties() {
    let s = json!({
        "type": "object",
        "additionalProperties": {"type": ["string", "null"]}
    });
    let out = simplify(s, SimplifyOptions::default());
    assert_eq!(out["additionalProperties"]["type"], "string");
}

#[test]
fn simplify_recurses_into_anyof() {
    let s = json!({
        "anyOf": [
            {"type": ["string", "null"]},
            {"type": "integer"}
        ]
    });
    let out = simplify(s, SimplifyOptions::default());
    assert_eq!(out["anyOf"][0]["type"], "string");
}

#[test]
fn analyze_walks_tuple_items_array() {
    // draft-07 tuple form: `items` is an array of subschemas. Expensive
    // features inside each tuple position must be discovered.
    let s = json!({
        "type": "array",
        "items": [
            {"type": ["string", "null"]},
            {"type": "integer"}
        ]
    });
    let a = analyze(&s);
    assert_eq!(a.nullable_union_count, 1);
    assert!(a.expensive_features.iter().any(|f| matches!(
        f,
        ExpensiveFeature::NullableUnion { pointer } if pointer == "/items/0"
    )));
}

#[test]
fn analyze_walks_prefix_items() {
    // JSON Schema 2020-12 tuple form: `prefixItems` is an array of subschemas.
    let s = json!({
        "type": "array",
        "prefixItems": [
            {"type": ["string", "null"]},
            {"type": "integer"}
        ]
    });
    let a = analyze(&s);
    assert_eq!(a.nullable_union_count, 1);
    assert!(a.expensive_features.iter().any(|f| matches!(
        f,
        ExpensiveFeature::NullableUnion { pointer } if pointer == "/prefixItems/0"
    )));
}

#[test]
fn simplify_recurses_into_tuple_items_array() {
    let s = json!({
        "type": "array",
        "items": [
            {"type": ["string", "null"]},
            {"type": "integer"}
        ]
    });
    let out = simplify(s, SimplifyOptions::default());
    assert_eq!(out["items"][0]["type"], "string");
    assert_eq!(out["items"][1]["type"], "integer");
}

#[test]
fn simplify_recurses_into_prefix_items() {
    let s = json!({
        "type": "array",
        "prefixItems": [
            {"type": ["string", "null"]},
            {"type": "integer"}
        ]
    });
    let out = simplify(s, SimplifyOptions::default());
    assert_eq!(out["prefixItems"][0]["type"], "string");
    assert_eq!(out["prefixItems"][1]["type"], "integer");
}
