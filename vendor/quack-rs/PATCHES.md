# Patches to the vendored `quack-rs` 0.14 crate

Upstream `quack-rs` 0.14 (the DuckDB loadable-extension SDK this extension is
built on) has a correctness bug on the BLOB hot path that silently corrupts
binary data. We vendor the crate and fix it at the source rather than continue
working around it, and add a small accessor (`Value::as_blob`) needed for
table-function bind parameters.

The rest of the crate is byte-identical to the 0.14.0 release on crates.io.

## Patch 1 — `VectorReader::read_blob` validated UTF-8 and dropped binary bytes

`VectorReader::read_blob` (and `StructReader::read_blob`, which delegates to
it) was implemented as:

```rust
pub unsafe fn read_blob(&self, idx: usize) -> &[u8] {
    unsafe { crate::vector::string::read_duck_string(self.data, idx).as_bytes() }
}
```

`read_duck_string` returns a `&str`, validating UTF-8 and substituting an empty
string when the bytes are not valid UTF-8. For a BLOB column that is arbitrary
binary — and in particular for ISO-WKB, whose IEEE-754 coordinate bytes are
frequently invalid UTF-8 (e.g. a `1.0` ordinate encodes an `0xF0` byte) — this
silently returns an empty slice for roughly half of all geometries.

The fix adds a binary-safe counterpart `read_duck_blob` in
`src/vector/string.rs` (a copy of `read_duck_string` that calls the existing
private `DuckStringView::as_bytes_unsafe` instead of the UTF-8-validating
`as_str`), exports it from `src/vector/mod.rs`, and points `VectorReader::read_blob`
(in `src/vector/reader.rs`) at it. `StructReader::read_blob` needs no change —
it forwards to `VectorReader::read_blob`.

```diff
 pub unsafe fn read_blob(&self, idx: usize) -> &[u8] {
-    unsafe { crate::vector::string::read_duck_string(self.data, idx).as_bytes() }
+    unsafe { crate::vector::string::read_duck_blob(self.data, idx) }
 }
```

This is the root-cause fix for the bug the README previously documented as
requiring a hand-rolled `BlobCol` reader in `src/dispatch.rs`. With this patch
the extension's dispatch layer uses `VectorReader::read_blob` directly; the
`BlobCol` workaround is gone.

## Patch 2 — `Value::as_blob()` for binary bind parameters

`quack_rs::value::Value` exposed `as_str` (VARCHAR) and numeric accessors but
no way to read a BLOB bind parameter. Set-returning table functions (this
extension's `ST_Dump` family) read their geometry argument in the bind
callback via `Value`, so we add a binary-safe `Value::as_blob() -> Vec<u8>`
implemented with the raw `duckdb_get_blob` / `duckdb_free` C API.

## Build requirements

None beyond the extension's normal requirements — the vendored crate uses the
same `libduckdb-sys` (with `loadable-extension`) already pulled in by the root
`Cargo.toml`. The `[patch.crates-io]` line in the root manifest points Cargo at
this directory.

When an upstream `quack-rs` release includes a binary-safe `read_blob` and a
`Value::as_blob` accessor, drop the `[patch.crates-io]` line for `quack-rs` in
the root `Cargo.toml` and delete this `vendor/` directory.
