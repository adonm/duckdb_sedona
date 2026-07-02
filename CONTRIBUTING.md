# Contributing

This extension aims to be the highest-quality spatial engine for DuckDB: a
practical superset of Apache SedonaDB and the commonly-used PostGIS analysis
surface. This document defines what "done" means for a new capability.

## Definition of done

Before a function or feature is merged, **all** of the following must hold:

1. **Namespace match.** The SQL name matches PostGIS/SedonaDB where feasible,
   or a DuckDB-specific name is justified in the commit message and docs.
2. **Canonical backend.** The backend is chosen and documented:
   - Literal SedonaDB kernel (preferred where a bridged kernel exists).
   - GEOS for planar topology (`ST_Node`, `ST_Polygonize`, etc.).
   - GeographicLib/Karney for spheroid geodesics.
   - PROJ for CRS transforms.
   - GDAL for raster I/O.
   - DuckDB SQL for relational/map algebra.
3. **Fail closed.** Invalid WKB, unsupported encodings, missing bridge
   functions, and undefined operations return `NULL` or a documented error —
   never a panic (the release profile uses `panic = "abort"`).
4. **SQL regression.** Add at least one check in the appropriate `tests/*.sql`
   file covering normal behavior plus at least one edge case (empty, NULL,
   degenerate input, Z-dim, large coordinates).
5. **Fidelity comparison.** If a literal SedonaDB bridge function exists for
   the same operation, add a check to `tests/fidelity.sql` proving equivalence.
   If divergence is intentional, document why in the commit message.
6. **Documentation.** Mention any semantic delta or runtime dependency in
   README.md or ROADMAP.md.
7. **Verification.** All of the following pass:
   - `cargo test --lib`
   - `cargo build --release`
   - `./tests/run_sql.sh` (all SQL suites)
   - `./ci/package-and-smoke.sh`

## Architecture quick reference

- `src/registry.rs` — THE CATALOG. One line per function via declarative macros.
- `src/dispatch.rs` — Generic vectorized executors (chunk-oriented).
- `src/bridge.rs` — DuckDB-chunk ⇄ Arrow bridge to literal SedonaDB kernels.
- `src/functions.rs` — Local geo-crate-backed implementations.
- `src/geos_backend.rs` — Narrow WKB → GEOS → WKB boundary for topology.
- `src/raster.rs` — GDAL raster table functions.
- `src/geometry.rs` — WKB ⇄ geo_types conversion (EWKB-tolerant).

## Commit conventions

- Lowercase prefix: `feat:`, `fix:`, `test:`, `refactor:`, `docs:`, `ci:`.
- One logical change per commit (executor shape + registrations + tests + docs).
- Small, focused commits — avoid mixing unrelated semantic changes.
