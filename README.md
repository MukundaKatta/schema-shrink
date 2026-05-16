# schema-shrink

[![crates.io](https://img.shields.io/crates/v/schema-shrink.svg)](https://crates.io/crates/schema-shrink)
[![docs.rs](https://docs.rs/schema-shrink/badge.svg)](https://docs.rs/schema-shrink)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)

Analyze and simplify JSON Schemas that bump into Anthropic strict-mode's "compiled grammar too large" error.

```toml
[dependencies]
schema-shrink = "0.1"
```

## Why

You set `strict: true` on your Anthropic tool, send a moderately complex JSON Schema, and the API returns:

```
The compiled grammar is too large
```

Three patterns explain almost every hit (from the [SDK issue thread](https://github.com/anthropics/anthropic-sdk-python/issues/1185)):

- **Nullable unions** (`type: ["string", "null"]`) compile exponentially; Anthropic caps them at 16 per strict schema.
- **Large enums** (20+ string values) inflate grammar rule counts.
- **Deep nesting + many `$ref`s** compile inline, not deduped.

`schema-shrink` analyzes your schema, points at the expensive features by JSON pointer, and applies the documented workarounds. It does **not** semantically decompose your schema; that needs domain knowledge you have and I don't.

## Analyze

```rust
use schema_shrink::analyze;
use serde_json::json;

let schema = json!({
    "type": "object",
    "properties": {
        "name": {"type": ["string", "null"]},
        "color": {"type": "string", "enum": ["red","green","blue", /* 25 more */]}
    }
});

let report = analyze(&schema);
println!("estimated grammar size: {}", report.estimated_grammar_size);
println!("likely too large: {}", report.likely_too_large);
for f in &report.expensive_features {
    println!("  {:?}", f);
}
```

Output points at the JSON pointer of each problem:

```
NullableUnion { pointer: "/properties/name" }
LargeEnum { pointer: "/properties/color", count: 28 }
```

## Simplify

```rust
use schema_shrink::{simplify, SimplifyOptions};

let lighter = simplify(schema, SimplifyOptions {
    strip_null_unions: true,           // type: ["X","null"] -> type: "X"
    max_enum_size: Some(20),           // truncate enums beyond 20 values
    drop_additional_properties_false: false,  // keep your strict constraint
});
```

After simplifying, you handle nulls in application code (since `null` is no longer in the schema). You also handle the enum overflow in application code.

## What it does NOT do

- **Semantic decomposition.** If your schema fundamentally has 50 fields and 30 enums, no analyzer can split it into two passes for you; that requires domain knowledge. The pattern that does work — two-pass extraction (first pass: top-level structure; second pass: nested details) — is sketched in the SDK issue thread and is a workflow change, not a code change.
- **Pre-validate against Anthropic's compiler.** Anthropic doesn't expose a dry-run endpoint. The `likely_too_large` heuristic is approximate.
- **Repair broken JSON Schemas.** If the schema itself is malformed, you get an unmodified copy back.

## Pairs with

- [`agentvet`](https://crates.io/crates/agentvet) — validate tool-call args before running them. Use a schema-shrink'd schema as the validation target.
- [`agentcast`](https://crates.io/crates/agentcast) — repair-and-validate LLM JSON output against a schema.

## License

MIT
