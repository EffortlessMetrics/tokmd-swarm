# tokmd-model

Deterministic aggregation and receipt modeling for tokmd.

## Problem
Raw scan results are not stable enough yet for output, diffing, or receipt generation.

## What it gives you
- `collect_file_rows`
- `create_lang_report`
- `create_module_report`
- `create_export_data`
- `unique_parent_file_count`
- `normalize_path`
- `module_key`

## API / usage notes
- This crate owns aggregation, sorting, filtering, and path normalization.
- It turns `tokei::Languages` plus optional in-memory inputs into tokmd receipts.
- Model builders that accept native report paths return `std::io::Result`.
  Propagate that result before emitting a receipt. An unsupported encoding
  or a prefix that makes distinct sources share a label produces an error.
- `native_report_path` renders native path components for file rows.
  `normalize_path` accepts portable mixed-separator text.
- `src/lib.rs` shows the exact report-building helpers and invariants.

## Go deeper
- Tutorial: [tokmd README](../../README.md)
- How-to: [Recipes](../../docs/recipes.md)
- Reference: [src/lib.rs](src/lib.rs)
- Explanation: [Architecture](../../docs/architecture.md)
