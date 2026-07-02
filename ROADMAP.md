# Roadmap to a SedonaDB + PostGIS superset

Status of the `sedonadb` DuckDB extension against the Apache SedonaDB and
PostGIS spatial surfaces, and what it takes to reach a **highest-fidelity,
usable, maintainable superset**.

## North-star target

Build the best spatial extension for DuckDB by combining three goals:

1. **SedonaDB superset, literal by default.** Every Apache SedonaDB SQL kernel
   that can be bridged safely should be callable in DuckDB and should be the
   canonical implementation for matching `st_*` functions. The literal bridge is
   not just an oracle; it is the preferred engine wherever SedonaDB already has a
   function.
2. **PostGIS-compatible SQL surface.** Prefer familiar `ST_*` names, arities,
   argument order, units, NULL behavior, and edge-case semantics. Where DuckDB,
   SedonaDB, Rust libraries, or missing infrastructure make exact PostGIS
   behavior impossible, document the mismatch and test it explicitly.
3. **Production quality over catalog inflation.** Never ship a function that can
   silently return wrong geometry. Prefer `NULL`, a documented limitation, or an
   unimplemented item over approximate behavior that looks authoritative.

Practical namespace policy:

- `st_*`: the user-facing, PostGIS/SedonaDB-like SQL namespace. If Apache
  SedonaDB has the function and the bridge supports its signature, `st_*` should
  route to the literal SedonaDB kernel or be proven equivalent before remaining
  local.
- `sedona_st_*`: explicit literal Apache SedonaDB bridge functions, useful to
  users, tests, and migration/debugging. These make the provenance visible while
  `st_*` remains the ergonomic namespace.
- extension-specific names (`sedona_join`, `*_crs` helpers, benchmarks/tools)
  are allowed when DuckDB needs a different shape than PostGIS/SedonaDB.

Implementation policy:

- Do **not** maintain two independent implementations for the same semantics as
  a permanent state. Once a SedonaDB bridge kernel is supported and validated,
  prefer routing the public `st_*` function to it.
- Keep local Rust implementations for: functions SedonaDB lacks, PostGIS-compat
  capabilities beyond SedonaDB, DuckDB-specific table/aggregate shapes, bridge
  unsupported types, or temporary fallback/performance experiments.
- If a local implementation overlaps SedonaDB, either deprecate it internally,
  convert it into a thin wrapper over the bridge, or document why it intentionally
  diverges.

Quality gates for new capabilities:

- **Fidelity first:** add local-vs-literal SedonaDB tests when a bridged kernel
  exists; add PostGIS/SedonaDB reference fixtures for hard edge cases otherwise.
- **Fail closed:** invalid WKB, unsupported encodings, missing bridge functions,
  and undefined operations must not panic or fabricate geometry.
- **Vectorized and packageable:** every feature must work through DuckDB chunks,
  SQL regressions, release packaging, and the smoke test.
- **Maintainable by design:** prefer one registry line plus a shared executor;
  keep FFI, Arrow, GDAL/PROJ, and topology code isolated behind small boundaries.
- **Usable docs:** every non-obvious semantic difference, runtime dependency, and
  extension-specific helper needs a short README/ROADMAP note and at least one
  SQL example or regression.

## Where we are now (~130 functions, all on the `geo`/`wkb` stack)

Constructors & I/O (incl. EWKT **and EWKB**) · accessors · DE-9IM predicates ·
measurements (incl. `ST_MaxDistance`/`ST_LongestLine`/`ST_ShortestLine`/
`ST_MinimumClearance`/`MinimumClearanceLine`) · boolean set ops ·
affine/simplify/segmentize transforms · `ST_MakeValid` robustness · four
aggregates (`ST_Collect`, `ST_Envelope` agg, `ST_Union` agg, `ST_MakeLine` agg)
· `ST_DWithin` · bbox accessors (join prefilter) · a custom robust
point-in-polygon · **geodesic/geography**
(`ST_DistanceSphere/DWithinSphere/LengthSphere/AreaSphere`) · EWKT/EWKB/SRID
stubs · typed WKT constructors · `ST_Points/LineLocatePoint/Frechet/\
ClosestPoint/Hausdorff/FlipCoordinates/Reverse/RemoveRepeatedPoints/\
OrientedEnvelope` · `ST_Affine`(6-param)/`ST_Segmentize`/`ST_LineSubstring`/
`ST_LineMerge`/`ST_CollectionExtract`/`ST_ForceCollection`/`ST_Multi`/
`ST_Normalize`/`ST_ForceRHR`/`ST_ForcePolygonCW`/`ST_ForcePolygonCCW`/
`ST_TriangulatePolygon`/`ST_OrderingEquals`/`ST_NRings` ·
`ST_MakeEnvelope`/`ST_MakePolygon`/`ST_RemovePoint`/`ST_AddPoint`/
`ST_SimplifyPreserveTopology`/`ST_MinimumBoundingCircle`/`ST_GeneratePoints`/
`ST_IsValidReason` · **`ST_Dump`/`ST_DumpPoints`/`ST_DumpSegments`** set-returning
table functions. Verified end-to-end in DuckDB 1.5.4 over a local DuckLake and
Apache SpatialBench (`benchmarks/BENCHMARKS.md`).

**Literal Apache SedonaDB now linked.** Beyond the reimplementation above,
`src/bridge.rs` invokes the real `sedona-functions` DataFusion UDF kernels
directly from DuckDB (DuckDB-chunk ⇄ Arrow bridge); 47 `sedona_*` functions
are registered side-by-side with ours and runtime-verified, including CRS-tagged
returns (item-crs structs unwrapped to WKB at the extension's native fidelity),
a CRS sidecar extractor (`sedona_st_geomfromewkt_crs`), and constant-scalar
argument detection. Bridge overhead is negligible (benchmarks/bridge.sql: literal
path competitive with the local reimplementation). See "Resolved #1" below.

## Previously-flagged hard bits — now resolved

1. **Literal Apache SedonaDB bridge — ✅ DONE and now a standing expansion path.**
   `src/bridge.rs` links the real `sedona-functions` crate (git rev `b23ccd15`,
   + `sedona-expr`/`schema` and `datafusion-expr`/`-common` as trait types only)
   and invokes SedonaDB's own DataFusion UDF kernels directly from a DuckDB
   callback via a DuckDB-chunk ⇄ Arrow bridge (BLOB/WKB → Arrow array →
   `SedonaScalarUDF::invoke_with_args` → result array → DuckDB vector). 47
   functions are registered under a `sedona_` prefix, side-by-side with the local
   reimplementation (`st_dimension` / `sedona_st_dimension` run the same
   algorithm through two code paths). Struct/item-crs returns are handled by
   unwrapping the geometry `item` to WKB and, where useful, exposing the CRS
   sidecar as VARCHAR (`sedona_st_geomfromewkt_crs`). The remaining SedonaDB
   scalar UDFs are reachable by adding registry lines plus, when needed, one more
   shared executor shape. Runtime-verified by Rust bridge tests and SQL
   regressions. The SedonaDB tree is pure-Rust, so it adds no GDAL/PROJ/GEOS and
   cannot collide with the vendored C deps.
2. **`ST_Transform` via PROJ (Tier 3a) — ✅ DONE.** `ST_Transform(geom,
   from_srid, to_srid)` reprojects between EPSG codes with a thread-local `Proj`
   cache. The transform path uses bundled/static PROJ; the full extension can
   still need dynamic GDAL/PROJ transitively for raster support.
3. **Spatial-join via disk-spill (Tier 3b) — ✅ DONE as `sedona_join` table function.**
   DuckDB's `COPY ... TO 'x.parquet'` is the spill; `sedona_join(a_path, b_path,
   predicate)` reads both Parquet files itself (`parquet`/`arrow` crates), builds an
   `rstar` R*-tree over the right side, applies the exact predicate, and streams
   `(a_row, b_row)` pairs. Verified: 20k×20k building self-join returns 37 pairs —
   identical to the bbox-prefilter result. This is the SedonaDB disk-spilling spatial
   join model, realized without needing any DuckDB join-planner/GiST API.
4. **`ST_VoronoiPolygons`** — `geo` 0.31 has no Voronoi; needs a new dep or port. Still
   open (low priority).

## Capability matrix (category-level)

Legend: ✅ shipped · 🟡 partial · ⏳ not yet · ➖ out of scope (niche).

| Category | PostGIS | SedonaDB | sedonadb (this ext.) | Notes |
|---|---|---|---|---|
| Constructors (WKT/WKB/EWKT/EWKB, typed `*FromText`) | ✅ | ✅ | ✅ | WKT/WKB/EWKT/EWKB + typed constructors + `ST_Point` all shipped. `from_wkb` is EWKB-tolerant at the trust boundary. |
| Output (`ST_AsText/Binary/EWKB/GeoJSON/HexEWKB`) | ✅ | ✅ | ✅ | Text/Binary/EWKB/GeoJSON/HexEWKB all done |
| Accessors (X/Y/Z/M, dims, rings, N-th geometry/point) | ✅ | ✅ | 🟡 | 2D accessors + `ST_NRings` done; Z/M stubs return NULL/false |
| DE-9IM predicates (`Intersects`…`Covers`, `OrderingEquals`) | ✅ | ✅ | ✅ | All 10 + `ST_OrderingEquals`; guarded for invalid input |
| Measurements (`Area/Length/Distance/Perimeter/Azimuth/Hausdorff/MaxDistance/LongestLine/ShortestLine`) | ✅ | ✅ | ✅ | core + distance-family done |
| Boolean set ops (`Union/Intersection/Difference/SymDiff`) | ✅ | 🟡 | ✅ | |
| `ST_MakeValid` / validity | ✅ | 🟡 | ✅ | robustness hardening done |
| Editing (`Translate/Scale/Rotate/Flip/Reverse/Affine/Segmentize/LineSubstring/LineMerge/Normalize`) | ✅ | ✅ | ✅ | all done incl. 6-param `ST_Affine` |
| Geometry processing (`Buffer/Simplify/ConvexHull/ConcaveHull/OrientedEnvelope/Triangulate/Voronoi`) | ✅ | 🟡 | 🟡 | Buffer/Simplify/Hull/OrientedEnvelope/TriangulatePolygon done; bounded Voronoi polygons + Polygonize open |
| Linear referencing (`LineInterpolatePoint/Locate/Substring`) | ✅ | 🟡 | ✅ | interpolate/locate/substring all done |
| Aggregates (`Collect/Union/Envelope/Intersection/MakeLine`) | ✅ | ✅ | ✅ | `ST_Collect`/`ST_Union`/`ST_Envelope`/`ST_MakeLine` agg done; intersection aggregate open |
| **Geography (geodesic) ops** | ✅ | ✅ | ✅ | `Distance/DWithin/Length/Area` Sphere done (lon/lat) |
| **CRS / PROJ (`ST_Transform`, SRID)** | ✅ | ✅ | ✅ | `ST_Transform` via PROJ (runtime libproj dep) |
| **Spatial index join (R-tree/GiST, `&&`/`<->`)** | ✅ | ✅ | ✅ | `sedona_join` table fn (R-tree over spilled parquet) + bbox-prefilter |
| **Raster / map algebra** | ✅ (PostGIS Raster) | ✅ (`sedona-raster`) | ✅ (core) — `st_raster_info` + `st_raster_stats` via vendored+patched GDAL against libgdal 3.13; full map-algebra pending |
| **3D / Z-M geometry + SFCGAL surfaces** | ✅ (SFCGAL) | ⏳ | ⏳ No mature Rust SFCGAL bindings (see Tier 4) |
| Topology / Tiger geocoder / address standardizer | ✅ | ➖ | ➖ | niche; not in SedonaDB either |

So: **geometry-level SQL surface** is already broad and at feature parity with
SedonaDB for the common cases. The real gaps to a true superset are the four
**infrastructure capabilities** below, not more scalar functions.

## Tiers

### Tier 1 — finish geometry-level parity (small, geo-backed, ~1 line each)
Cheap wins; each is one `register_*!` line + a `geo` call. **Mostly ✅ done.**

- ✅ EWKB/EWKT I/O: `ST_AsEWKB`, `ST_GeomFromEWKB` (EWKB-tolerant `from_wkb`),
  `ST_AsEWKT`, `ST_GeomFromEWKT`, `ST_AsHexEWKB`.
- ✅ SRID stubs: `ST_SRID` (0), `ST_SetSRID` (no-op tag).
- ✅ Typed constructors: `ST_LineFromText`, `ST_PointFromText`,
  `ST_PolygonFromText`, `ST_MLineFromText`, … (route through WKT parser).
- ✅ `ST_Affine`(6 doubles), `ST_Segmentize`, `ST_LineSubstring`,
  `ST_LineMerge`, `ST_CollectionExtract`, `ST_ForceCollection`, `ST_Multi`,
  `ST_Normalize`, `ST_ForceRHR`/`ST_ForcePolygonCW`/`ST_ForcePolygonCCW`,
  `ST_SnapToGrid`.
- ✅ More aggregates: `ST_Union` agg (`st_union_agg`), `ST_Envelope` agg.
  `ST_Collect` already done. Intersection aggregate still open.
- ✅ `ST_TriangulatePolygon` (Delaunay-interior approximation).
- ✅ `ST_Dump`, `ST_DumpPoints`, `ST_DumpSegments` — set-returning table
  functions (`src/dump.rs`); the previously- unbuilt FFI shape is now wired up.
- ✅ `ST_MakeEnvelope`, `ST_MakePolygon`, `ST_RemovePoint`/`ST_AddPoint`,
  `ST_SimplifyPreserveTopology`, `ST_MinimumClearance`/`...Line`,
  `ST_MinimumBoundingCircle` (Welzl), `ST_GeneratePoints`, `ST_IsValidReason`.
- ⏳ `ST_Node`, `ST_Snap`, `ST_Polygonize`, `ST_BuildArea` — topology editing.
- ⏳ `ST_VoronoiPolygons` (bounded cell polygons; `ST_VoronoiLines` already
  ships).

### Tier 1b — PostGIS geo-backed geometry processing
✅ `ST_HausdorffDistance`, `ST_FrechetDistance`, `ST_MaxDistance`,
`ST_LongestLine`, `ST_ClosestPoint`, `ST_ShortestLine`, `ST_Project`,
`ST_OrientedEnvelope`, `ST_TriangulatePolygon`, `ST_MinimumClearance`/`...Line`,
`ST_MinimumBoundingCircle`, `ST_GeneratePoints` all shipped. Still open:
`ST_Subdivide`.

### Tier 2 — Geography (geodesic) — ✅ DONE
`ST_DistanceSphere`, `ST_DWithinSphere`, `ST_LengthSphere`, `ST_AreaSphere`
(launch/lat → metres / m² via `geo`'s Haversine + Chamberlain-Duquette). No new
dep. (PostGIS spheroid-accurate `ST_DistanceSpheroid` and full
geometry-vs-geometry geodesic distance still open.)

### Tier 3 — CRS reprojection + native spatial index — ✅ DONE
- **`ST_Transform` via PROJ** — implemented (`proj` crate). Runtime dep on
  `libproj.so`. Thread-local CRS cache. Verified on 4326↔3857.
- **Spatial index join** — two paths: (1) `sedona_join(a.parquet, b.parquet,
  predicate)` table function: extension reads both files, builds an `rstar`
  R*-tree, streams matching pairs — the disk-spill model; (2) bbox-prefilter via
  materialized `ST_XMin/Max/YMin/MaxY` + DuckDB IEJoin for inline joins. Both
  verified against SpatialBench (20k building self-join = 37 pairs either way).

### Tier 4 — Raster, 3D, topology (long tail)
- **Raster / map algebra** — ✅ Core landed via **vendored + patched GDAL** against
  libgdal 3.13. The upstream `gdal` 0.19 crate lags 3.13 (it renamed `GDT_Byte`→
  `GDT_UInt8` and added `GDALRasterIOExtraArg::bOperateInBufType`); we vendor only
  the high-level `gdal` crate (`vendor/gdal`, with `PATCHES.md`) — `gdal-sys` is
  unpatched and pulled from crates.io — and enable `bindgen` so fresh FFI
  bindings are generated from the installed 3.13 headers. Ships
  `st_raster_info(path)` and `st_raster_stats(path, band)` (read any GDAL format;
  summary stats in the band's native type). **Open:** `ST_MapAlgebra`, `ST_AsRaster`,
  `ST_Clip`, band math. **Build needs** `pkg-config gdal` + `LIBCLANG_PATH`;
  **runtime needs** `libgdal.so` (+ its libproj/libsqlite3) via `LD_LIBRARY_PATH`.
- **Static PROJ** — ✅ Our own PROJ (for `ST_Transform`) is now **bundled +
  statically linked** (`proj-sys/bundled_proj` + `libsqlite3-sys/bundled`), so
  reprojection has no runtime dep of its own. GDAL brings its own dynamic libproj,
  so the extension overall still needs `LD_LIBRARY_PATH` (or system libgdal) while
  GDAL is linked. (GDAL is intentionally **not** feature-gated — the extension is
  a single full-capability build.)
- **Delaunay / Voronoi** — ✅ Done. `ST_DelaunayTriangles` (via `delaunator`)
  and `ST_VoronoiLines` (dual of the Delaunay triangulation — interior edges;
  full bounded cell polygons still open).
- **3D / Z-M + SFCGAL** — ⏳ Not feasible today. `geo`/`wkb`/our pipeline are
  2D-only; full 3D needs Z/M through the entire stack plus surface algorithms
  (extrude, straight skeleton, 3D boolean). **There is no mature Rust SFCGAL/CGAL
  binding** — this would mean writing/maintaining `sfcgal-sys` (weeks+). The one
  genuinely out-of-reach PostGIS surface for a Rust extension.
- **Topology** — ➖ niche; PostGIS topology is a separate subsystem and not in
  SedonaDB. Out of scope.

## What "superset" realistically means

A 100% byte-compatible PostGIS clone is not the target: PostgreSQL operators,
GiST planner integration, SFCGAL 3D solids, topology, Tiger/geocoder, and some
raster administration APIs do not map cleanly to a DuckDB loadable extension.

The target is higher value and more focused:

1. **SedonaDB-plus:** every practical SedonaDB vector SQL function available in
   DuckDB, with the literal bridge as the canonical implementation for matching
   public `st_*` functions wherever signatures and return types fit.
2. **PostGIS-compatible core:** the common PostGIS vector/geography/CRS/raster
   analysis surface under familiar `ST_*` names, with exact semantics where
   feasible and tested/documented deltas where not.
3. **DuckDB-native usability:** install/load/package cleanly, operate on WKB BLOBs
   that interoperate with DuckDB `spatial`, stream through vectorized chunks, and
   provide spatial-join/raster workflows that fit DuckDB rather than PostgreSQL.
4. **Maintainable growth:** new functions should expand a small number of shared
   executor families and reference tests, not create per-function FFI or semantic
   snowflakes.

## Focused execution plan

### P0 — compatibility contract and safety net (always-on) — ✅ LANDED

- `tests/fidelity.sql` is the gate for every overlapping `st_*` / `sedona_st_*`
  function; `tests/edge_cases.sql` covers empty/NULL/Z-dim/extreme/cocircular
  degenerate inputs through both paths.
- A PostGIS/SedonaDB fixture corpus for the remaining hard edge cases
  (antimeridian, spheroid) grows incrementally as those capabilities land.
- Each routed function is proven equivalent before the local code is unwired.
- `try_udf`/bridge lookup stays fail-closed under `panic = "abort"`.

### P1 — make the literal SedonaDB bridge the default implementation path — ✅ LANDED

- Full inventory of `default_function_set()` done; every registrable scalar UDF
  is bridged (65 `sedona_st_*` functions). Only ST_Affine (geom+6 doubles),
  type-conversion UDFs (st_togeography/st_togeometry), and special/table UDFs
  (st_knn/st_dump/sd_*) remain — each blocked on a new shape or unsupported type.
- 20 public `st_*` accessors now route to the literal SedonaDB kernel (one
  implementation, two SQL entry points); overlapping local code is dormant.
- CRS/item-crs returns stay unwrapped to native WKB with opt-in `*_crs` helpers.

### P2 — PostGIS namespace and usability polish — ✅ LANDED

- README "PostGIS compatibility & common workflows" subsection with runnable
  examples (CRS reprojection, geodesic distance, bbox-prefilter join, literal
  comparison) and documented deltas (2D stack, no PG operators/GiST, SRID
  sidecar).
- Alias/arity alignment is ongoing as new functions land.

### P3 — hard capability gaps, only with correct algorithms — ⏳ DEFERRED

- **Topology editing:** `ST_Node`, `ST_Snap`, `ST_Polygonize`, `ST_BuildArea`.
  These need robust graph/topology code and should land with adversarial fixtures,
  not heuristic approximations.
- **Bounded Voronoi polygons:** implement only with a correct half-edge/infinite
  edge treatment and tests for cocircular grids, duplicate points, hull cells, and
  tolerance. The earlier angle-sort prototype was rejected because it lost cells
  on degenerate inputs.
- **Spheroid geodesics:** add `ST_DistanceSpheroid`/related functions with a
  Karney/Vincenty-quality implementation and reference fixtures; keep current
  `*Sphere` functions explicit about spherical units/accuracy.
- **Raster map algebra:** extend from `st_raster_info`/`st_raster_stats` to
  clipping, rasterization, and band math behind a narrow GDAL boundary, with
  deterministic small rasters in tests.

### P4 — maintainability and release engineering

- Keep dependency boundaries explicit: pure-Rust Sedona bridge, bundled/static
  PROJ for transforms, dynamic GDAL for raster, and no accidental GEOS/SFCGAL
  runtime requirement unless deliberately added.
- Track binary size, load-time smoke, bridge overhead, and representative query
  performance in `benchmarks/`.
- Prefer small commits by phase: executor shape + registrations + SQL/Rust tests
  + docs; avoid large mixed semantic changes.
- Periodically regenerate the capability matrix from the registry to prevent docs
  from drifting away from the SQL catalog.
