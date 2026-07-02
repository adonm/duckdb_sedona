# Roadmap to a highest-fidelity SedonaDB + PostGIS superset

Status of the `sedonadb` DuckDB extension against the Apache SedonaDB and
PostGIS spatial surfaces, and the plan to keep it focused, usable, and
maintainable while moving toward a practical superset.

## North-star target

Build the highest-quality spatial engine for DuckDB: a practical **superset of
Apache SedonaDB and the commonly-used PostGIS analysis surface**, under a SQL
namespace that is as close as possible to `ST_*` PostGIS/SedonaDB conventions.

The target is deliberately ambitious but narrow enough to stay maintainable:

1. **SedonaDB superset, literal by default.** Every Apache SedonaDB SQL kernel
   that can be bridged safely should be callable in DuckDB and should be the
   canonical implementation for matching `st_*` functions. The literal bridge is
   not just an oracle; it is the preferred engine wherever SedonaDB already has a
   function.
2. **PostGIS-compatible SQL surface.** Prefer familiar `ST_*` names, arities,
   argument order, units, NULL behavior, and edge-case semantics. Where DuckDB,
   SedonaDB, Rust libraries, or missing infrastructure make exact PostGIS
   behavior impossible, document the mismatch and test it explicitly.
3. **Highest fidelity over catalog inflation.** Never ship a function that can
   silently return wrong geometry. Prefer `NULL`, a documented limitation, or an
   unimplemented item over approximate behavior that looks authoritative.
4. **DuckDB-native usability.** Spatial workflows should feel natural in DuckDB:
   vectorized chunk execution, WKB interop with DuckDB `spatial`, table functions
   for set-returning/raster/join workflows, and SQL as the composition language.
5. **Maintainable growth.** New capability should expand shared executor families
   or narrow backend boundaries (Sedona bridge, GEOS, GDAL, PROJ, GeographicLib),
   not add one-off semantic snowflakes.

## SQL namespace policy

- `st_*`: the user-facing namespace. Match PostGIS/SedonaDB names, arities,
  argument order, units, and NULL behavior wherever feasible.
- `sedona_st_*`: explicit literal Apache SedonaDB bridge functions. These are
  useful to users, tests, migration/debugging, and fidelity comparisons.
- Extension-specific helpers (`sedona_join`, `*_crs`, benchmarks/tools) are
  allowed when DuckDB needs a different shape than PostGIS/SedonaDB, but should be
  documented as DuckDB-native workflow helpers rather than compatibility claims.

Implementation policy:

- Do **not** maintain two independent implementations for the same semantics as a
  permanent state. Once a SedonaDB bridge kernel is supported and validated,
  prefer routing the public `st_*` function to it.
- Keep local Rust implementations for: functions SedonaDB lacks, PostGIS-compat
  capabilities beyond SedonaDB, DuckDB-specific table/aggregate shapes, bridge
  unsupported types, or temporary fallback/performance experiments.
- If a local implementation overlaps SedonaDB, either deprecate it internally,
  convert it into a thin wrapper over the bridge, or document why it intentionally
  diverges.
- Backend choices should be boring and canonical: Apache SedonaDB for SedonaDB
  kernels, GEOS for planar topology, GeographicLib/Karney for spheroid geodesics,
  PROJ for CRS transforms, GDAL for raster I/O, and DuckDB SQL for relational/map
  algebra.

## Quality gates for every new capability

- **Fidelity first:** add local-vs-literal SedonaDB tests when a bridged kernel
  exists; add PostGIS/SedonaDB reference fixtures for hard edge cases otherwise.
- **Fail closed:** invalid WKB, unsupported encodings, missing bridge functions,
  and undefined operations must not panic or fabricate geometry.
- **Vectorized and packageable:** every feature must work through DuckDB chunks,
  SQL regressions, release packaging, and the smoke test.
- **Maintainable by design:** prefer one registry line plus a shared executor;
  keep FFI, Arrow, GDAL/PROJ, GEOS, and topology code isolated behind small
  boundaries.
- **Usable docs:** every non-obvious semantic difference, runtime dependency, and
  extension-specific helper needs a short README/ROADMAP note and at least one SQL
  example or regression.

## Where we are now

The extension already has a broad vector/geography/raster surface over WKB BLOBs:

- Constructors and I/O: WKT/WKB/EWKT/EWKB, typed WKT/WKB constructors, point/Z/M
  constructors, GeoJSON/HexEWKB output.
- Accessors and predicates: dimension, points/geometries/rings, XY/ZM accessors,
  bbox accessors, DE-9IM predicates, validity checks, ordering equality.
- Measurements and processing: area/length/distance/perimeter, Hausdorff/Frechet,
  max/longest/shortest line, affine/editing transforms, simplify/segmentize,
  hulls/oriented envelope, triangulation, make-valid, minimum clearance/circle.
- Aggregates and set-returning functions: collect/union/envelope/makeline
  aggregates plus `ST_Dump`, `ST_DumpPoints`, and `ST_DumpSegments`.
- CRS/geography: `ST_Transform`, sphere geodesics, WGS84 spheroid geodesics.
- Hard algorithms: GEOS-backed `ST_Node`, `ST_Polygonize`, `ST_BuildArea`,
  `ST_VoronoiPolygons`, `ST_Snap`, and `ST_MakeValid`.
- Raster: `st_raster_info`, `st_raster_stats`, `st_raster_transform`, and
  `st_pixeldata(path, band)` for DuckDB-native map algebra.

**Literal Apache SedonaDB is linked.** `src/bridge.rs` invokes the real
`sedona-functions` DataFusion UDF kernels directly from DuckDB via a
DuckDB-chunk ⇄ Arrow bridge. 72 `sedona_st_*` functions are registered and
runtime-verified, including CRS-tagged returns, CRS sidecar extractors,
WKT/WKB typed constructors, Z/M point constructors, and constant-scalar argument
detection. 36 public `st_*` functions already route to the literal SedonaDB
kernel.

Current verification baseline:

- Rust unit tests: 64 pass.
- SQL regressions: 213 pass / 0 fail.
- Release smoke test: 7 backend checks pass (local, SedonaDB, aggregate, GEOS, spheroid, raster).
- Catalog: 231 registered SQL functions (158 `st_*` public + 72 `sedona_st_*` bridge + 1 extension-specific). Audit with `python3 tools/catalog_audit.py`.

## Capability matrix (category-level)

Legend: ✅ shipped · 🟡 partial · ⏳ not yet · ➖ intentionally out of scope.

| Category | PostGIS | SedonaDB | sedonadb extension | Notes |
|---|---|---|---|---|
| Constructors (WKT/WKB/EWKT/EWKB, typed `*FromText`) | ✅ | ✅ | ✅ | WKT/WKB/EWKT/EWKB + typed constructors + point/Z/M constructors. |
| Output (`ST_AsText/Binary/EWKB/GeoJSON/HexEWKB`) | ✅ | ✅ | ✅ | Text/Binary/EWKB/GeoJSON/HexEWKB shipped. |
| Accessors (X/Y/Z/M, dims, rings, N-th geometry/point) | ✅ | ✅ | 🟡 | Broad 2D + bridged Z/M accessors; full Z/M-preserving local pipeline remains limited. |
| DE-9IM predicates (`Intersects`…`Covers`, `OrderingEquals`) | ✅ | ✅ | ✅ | Guarded for invalid input. |
| Measurements (`Area/Length/Distance/Perimeter/Azimuth/Hausdorff/...`) | ✅ | ✅ | ✅ | Core, distance-family, clearance-family shipped. |
| Boolean set ops (`Union/Intersection/Difference/SymDiff`) | ✅ | 🟡 | ✅ | Scalar polygonal set ops shipped; intersection aggregate still open. |
| `ST_MakeValid` / validity | ✅ | 🟡 | ✅ | Robustness hardening shipped. |
| Editing (`Translate/Scale/Rotate/Flip/Reverse/Affine/Segmentize/...`) | ✅ | ✅ | ✅ | Includes 6-param `ST_Affine`; `ST_Snap` via GEOS. |
| Geometry processing (`Buffer/Simplify/Hulls/Triangulate/Voronoi`) | ✅ | 🟡 | ✅ | Bounded Voronoi polygons via GEOS. |
| Topology editing (`Node/Polygonize/BuildArea`) | ✅ | 🟡 | ✅ | GEOS-backed. |
| Linear referencing (`LineInterpolatePoint/Locate/Substring`) | ✅ | 🟡 | ✅ | Done. |
| Aggregates (`Collect/Union/Envelope/Intersection/MakeLine`) | ✅ | ✅ | 🟡 | Collect/Union/Envelope/MakeLine done; intersection aggregate open. |
| Geography/geodesic ops | ✅ | ✅ | ✅ | Sphere + WGS84 spheroid (`Distance/DWithin/Length/Area`) done; custom spheroid parameter open. |
| CRS / PROJ (`ST_Transform`, SRID) | ✅ | ✅ | ✅ | `ST_Transform` via PROJ; SRID represented at extension-native fidelity with CRS sidecars where needed. |
| Spatial index join (`&&`, GiST/R-tree workflows) | ✅ | ✅ | ✅ | `sedona_join` table fn over spilled parquet + bbox prefilter helpers. |
| Raster / map algebra | ✅ | ✅ | 🟡 | Info/stats/transform/pixel streaming done; clipping/rasterization/value APIs open. |
| 3D / Z-M geometry + SFCGAL surfaces | ✅ | ⏳ | ⏳ | Z/M bridge surface exists; full 3D solid/surface operations are out of scope until mature Rust SFCGAL/CGAL exists. |
| Topology schema / Tiger geocoder / address standardizer | ✅ | ➖ | ➖ | PostgreSQL-specific/niche subsystems; intentionally out of scope. |

## What "superset" realistically means

A 100% byte-compatible PostGIS clone is not the target. PostgreSQL operators,
GiST planner integration, SFCGAL 3D solids, topology schemas, Tiger/geocoder, and
some raster administration APIs do not map cleanly to a DuckDB loadable
extension.

The target is higher-value and more focused:

1. **SedonaDB-plus:** every practical SedonaDB vector SQL function available in
   DuckDB, with the literal bridge as the canonical implementation for matching
   public `st_*` functions wherever signatures and return types fit.
2. **PostGIS-compatible core:** the common PostGIS vector/geography/CRS/raster
   analysis surface under familiar `ST_*` names, with exact semantics where
   feasible and tested/documented deltas where not.
3. **DuckDB-native workflows:** install/load/package cleanly, operate on WKB BLOBs
   that interoperate with DuckDB `spatial`, stream through vectorized chunks, and
   provide join/raster workflows that fit DuckDB rather than PostgreSQL internals.
4. **Maintainable growth:** new functions should expand a small number of shared
   executor families and reference tests, not create per-function FFI or semantic
   snowflakes.

## Three-month development plan (July–September 2026)

The next three months are about **quality, fidelity, usability, and
maintainability**, not raw catalog count. Every item below should either make the
extension more PostGIS/SedonaDB-compatible for real SQL, move work to a canonical
backend, or reduce future maintenance risk.

Planning rules:

- Ship in small vertical slices: registry entry, backend/executor shape if needed,
  reference tests, docs, benchmark/smoke coverage when relevant.
- Prefer replacing local duplicate semantics with literal SedonaDB or GEOS/GDAL
  canonical behavior over adding new local algorithms.
- Treat docs as a product surface: no undocumented semantic deltas and no stale
  capability counts.
- Defer large/niche systems that would blur the product: PostgreSQL planner hooks,
  topology schema, Tiger/geocoder, SFCGAL solids, and a custom raster expression
  language.

### Month 1 — compatibility contract and namespace polish — ✅ LANDED

Outcome: users can see exactly what is compatible, what is bridged, and what
differs before they port SQL.

Landed:

1. **Generated catalog audit.** `tools/catalog_audit.py` reads `src/registry.rs`
   and emits the registered SQL catalog grouped by provenance: literal SedonaDB,
   local geo, GEOS, PROJ, GDAL/raster, aggregates, and table functions. Run with
   `python3 tools/catalog_audit.py [--markdown]`.
2. **PostGIS/SedonaDB compatibility table.** `COMPATIBILITY.md` lists common
   PostGIS functions and their status: supported, alias, semantic delta, not yet,
   or intentionally out of scope.
3. **Namespace cleanup.** Added `ST_Force3D`/`3DZ`/`3DM`/`4D` routed to the
   literal SedonaDB kernel (Z/M dimension forcing). Documented semantic delta:
   explicit z/m parameter required (PostGIS defaults to 0).
4. **Literal routing pass.** 36 public `st_*` functions now route to the literal
   SedonaDB kernel (up from 32).
5. **Reference fixtures expansion.** `tests/reference/month1_fixtures.sql` adds
   22 checks: invalid geometry (bowtie), empty/NULL propagation, antimeridian
   geography, CRS round-trip stability, GEOS snap degeneracy, Voronoi single
   point, force-dimension family, large coordinates, nested collections, polygon
   holes, degenerate lines.

### Month 2 — high-fidelity capability work

Outcome: fill high-value gaps with canonical engines, especially where incorrect
approximations would be harmful.

1. **GEOS-backed large-geometry tools.** Implement `ST_Subdivide` if a robust,
   deterministic GEOS or well-tested local strategy is available. Prioritize join
   and raster workflows over exotic options.
2. **Aggregate hardening.** Revisit `ST_IntersectionAgg`; ship only if invalid,
   empty, and mixed-type cases can be tested and fail closed. Otherwise document
   the explicit SQL workaround.
3. **Raster value access.** Add `ST_Value`/point sampling and nodata-aware band
   metadata helpers. Keep map algebra as DuckDB SQL over `st_pixeldata` rather
   than a separate expression parser.
4. **Raster clipping workflow.** Prefer a DuckDB-native clip recipe using
   `st_pixeldata`, `st_raster_transform`, and geometry predicates. Add a GDAL
   `ST_Clip` only if the API remains small and semantics can be tested against
   references.
5. **Spheroid parameter fidelity.** Evaluate PostGIS-compatible custom spheroid
   parsing for `*Spheroid`; ship only if the default WGS84 behavior stays simple
   and unambiguous.

Exit gates:

- New hard algorithms use canonical backend behavior or are explicitly deferred.
- Raster nodata, bounds, and coordinate transforms have regression coverage.
- Benchmarks include any new GEOS/raster paths that can affect analytical scale.

### Month 3 — usability, scale, and release readiness

Outcome: the full-capability extension is easy to install, debug, and use for
real DuckDB spatial analytics.

1. **Spatial join ergonomics.** Add documented recipes for bbox columns,
   IEJoin-friendly predicates, and `sedona_join` spill/R-tree workflows. Consider
   helper SQL/macros only if they reduce user error without hiding semantics.
2. **Performance budgets.** Turn `benchmarks/backends.sql` into a repeatable
   tracking suite for bridge overhead, GEOS calls, spheroid geodesics, raster scan
   throughput, package size, and load time.
3. **Release packaging.** Keep one full-capability extension as the default.
   Harden container build/test scripts and smoke tests for DuckDB, SedonaDB,
   local geo, GEOS, PROJ, GDAL, raster, aggregates, and table functions.
4. **User-facing examples.** Add copy-pasteable workflows for GeoParquet ingest,
   CRS transform joins, geodesic distance, dissolve, dump, raster sampling,
   raster reclassification, and `sedona_join`.
5. **Maintenance cleanup.** Delete or clearly mark dormant local implementations
   once public `st_*` routes to SedonaDB/GEOS. Keep backend boundaries small and
   documented.

Exit gates:

- Release smoke covers every backend family and at least one table function.
- README has a short path from install → load → common workflows → known deltas.
- Benchmarks and SQL regressions can be run by a new contributor using documented
  commands.

## Standing release gates

These are non-negotiable for every month and every release:

- No silent wrong geometry.
- `st_*` remains the ergonomic PostGIS/SedonaDB-like namespace.
- Literal SedonaDB kernels remain callable and tested under `sedona_st_*`.
- All backends fail closed on invalid/unsupported input.
- SQL regressions, Rust tests, release packaging, and smoke tests pass.
- New semantic deltas are documented before they ship.

## Priority backlog

### Now / next three months

- Generated catalog and compatibility table from `registry.rs`.
- More public `st_*` routing to literal SedonaDB where proven equivalent.
- GEOS-backed `ST_Subdivide` or an explicit defer with rationale.
- `ST_IntersectionAgg` decision: robust implementation or documented workaround.
- Raster value/nodata helpers and a tested clipping workflow.
- Spatial-join usability docs and benchmark tracking.

### Later, if justified by users and tests

- Additional PostGIS aliases/overloads once ambiguity and fidelity are resolved.
- More GDAL-backed raster administration helpers.
- macOS/Linux release automation beyond the current container flow.
- Custom spheroid parsing if it can match PostGIS without confusing defaults.

### Not currently a goal

- PostgreSQL planner/operator compatibility (`&&`, `<->`, GiST hooks) beyond
  documented functional equivalents.
- PostGIS topology schema, Tiger geocoder, address standardizer.
- Full SFCGAL/CGAL 3D solids/surfaces before mature Rust bindings exist.
- A custom raster map-algebra expression language; DuckDB SQL is the expression
  language.

## Definition of done for a new capability

1. Namespace matches PostGIS/SedonaDB where feasible, or a DuckDB-specific name is
   justified.
2. Canonical backend chosen and documented.
3. Invalid/unsupported inputs return NULL or a documented error state; no panic.
4. SQL regression covers normal behavior and at least one edge case.
5. If overlapping SedonaDB exists, fidelity comparison is added or divergence is
   documented.
6. README/ROADMAP mention any semantic delta or runtime dependency.
7. `cargo test --lib`, release build, SQL suites, and package smoke pass.
