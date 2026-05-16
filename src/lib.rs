//! Analyze and simplify JSON Schemas that bump into Anthropic's
//! "compiled grammar too large" error in strict mode.
//!
//! From the [Anthropic SDK issue thread](https://github.com/anthropics/anthropic-sdk-python/issues/1185),
//! three patterns explain almost every hit:
//!
//! - **Nullable unions** (`["string", "null"]`) cost exponentially; Anthropic
//!   docs cap them at 16 across a single strict schema.
//! - **Large enums** (20+ string values) inflate grammar rule counts.
//! - **Deep nesting + many `$ref`** compiles to inlined paths, not deduped
//!   references.
//!
//! `schema-shrink` analyzes a schema, points at the expensive features, and
//! applies the documented workarounds. It does **not** semantically
//! decompose your schema into multi-pass extraction; that requires domain
//! knowledge only you have.
//!
//! # Quick start
//!
//! ```
//! use schema_shrink::{analyze, simplify, SimplifyOptions};
//! use serde_json::json;
//!
//! let schema = json!({
//!     "type": "object",
//!     "properties": {
//!         "name": {"type": ["string", "null"]},
//!         "tags": {"type": "array", "items": {"type": ["string", "null"]}},
//!         "color": {"type": "string", "enum": [
//!             "red","green","blue","cyan","magenta","yellow","black","white",
//!             "gray","orange","pink","brown","purple","teal","navy","crimson",
//!             "gold","silver","bronze","ivory","khaki","lime","olive","plum","tan"
//!         ]}
//!     }
//! });
//!
//! let report = analyze(&schema);
//! assert!(report.likely_too_large || report.expensive_features.len() >= 2);
//!
//! // Drop nullable unions, drop the largest enums to a cap.
//! let lighter = simplify(schema, SimplifyOptions::default());
//! ```
#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

mod analyze;
mod simplify;

pub use crate::analyze::{analyze, Analysis, ExpensiveFeature};
pub use crate::simplify::{simplify, SimplifyOptions};
