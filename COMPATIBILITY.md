# PostGIS / SedonaDB Compatibility Table

This table lists common PostGIS functions and their status in this extension.
"Supported" means the function exists and matches PostGIS semantics. "Alias"
means it is available under a different name or shape. "Delta" means there is a
documented semantic difference. "Not yet" means it is planned. "Out of scope"
means it is intentionally not implemented.

Generated counts: **231 SQL functions** (158 `st_*` + 72 `sedona_st_*` + 1 extension).

Legend: ✅ supported · 🔄 alias/different shape · ⚠️ semantic delta · ⏳ not yet · ➖ out of scope

## Constructors

| PostGIS function | Status | Notes |
|---|---|---|
| `ST_GeomFromText` | ✅ | Also aliased as `ST_GeometryFromText`. |
| `ST_GeomFromWKB` | ✅ | Local reimplementation (trust-boundary validation). |
| `ST_GeomFromEWKB` | ✅ | Local reimplementation. |
| `ST_GeomFromEWKT` | ✅ | Local. Literal twin: `sedona_st_geomfromewkt`. |
| `ST_LineFromText` | ✅ | Alias of `ST_GeomFromText`. |
| `ST_PointFromText` | ✅ | Alias. |
| `ST_PolygonFromText` | ✅ | Alias. |
| `ST_Point` | ✅ | Also aliased as `ST_MakePoint`. |
| `ST_MakeEnvelope` | ✅ | |
| `ST_MakePolygon` | ✅ | |
| `ST_MakeLine` | ✅ | Scalar + aggregate (`ST_MakeLine_Agg`). |
| `ST_Polygon` | ⏳ | Not yet. |

## Output

| PostGIS function | Status | Notes |
|---|---|---|
| `ST_AsText` | ✅ | Routes to literal SedonaDB kernel. |
| `ST_AsBinary` | ✅ | Routes to literal kernel. |
| `ST_AsEWKB` | ✅ | Routes to literal kernel. |
| `ST_AsEWKT` | ✅ | Local. |
| `ST_AsGeoJSON` | ✅ | Local. |
| `ST_AsHEXEWKB` | ✅ | Local. |
| `ST_AsMVT` | ⏳ | Not yet (Mapbox Vector Tiles). |
| `ST_AsTWKB` | ⏳ | Not yet. |
| `ST_AsSVG` | ⏳ | Not yet. |
| `ST_AsKML` | ⏳ | Needs CRS support. |

## Accessors

| PostGIS function | Status | Notes |
|---|---|---|
| `ST_X` / `ST_Y` | ✅ | Routes to literal kernel. |
| `ST_Z` / `ST_M` | ✅ | Routes to literal kernel. Returns NULL on 2D WKB. |
| `ST_XMin..Max` / `ST_YMin..Max` | ✅ | Routes to literal kernel. |
| `ST_GeometryType` | ✅ | Routes to literal kernel. |
| `ST_Dimension` | ⚠️ | Routes to literal kernel. `ST_Dimension(EMPTY)` = 0 (matches SedonaDB), PostGIS returns -1. |
| `ST_NumPoints` | ✅ | Routes to literal kernel. |
| `ST_NumGeometries` | ✅ | Routes to literal kernel. |
| `ST_SRID` | ✅ | Routes to literal kernel. Returns 0 (SRID-less BLOB). |
| `ST_ZMFlag` | ✅ | Routes to literal kernel. |
| `ST_HasZ` / `ST_HasM` | ✅ | Routes to literal kernel. |
| `ST_IsEmpty` | ✅ | Routes to literal kernel. |
| `ST_IsClosed` | ✅ | Routes to literal kernel. |
| `ST_IsRing` | ✅ | Local. |
| `ST_IsCollection` | ✅ | Routes to literal kernel. |
| `ST_IsValid` | ✅ | Local. |
| `ST_IsValidReason` | ✅ | Local. |
| `ST_IsValidDetail` | ⏳ | Not yet. |
| `ST_NumInteriorRings` | ✅ | Also aliased as `ST_NumInteriorRing`. |
| `ST_NRings` | ✅ | Local. |
| `ST_CoordDim` | ✅ | Local. |

## Predicates (DE-9IM)

| PostGIS function | Status | Notes |
|---|---|---|
| `ST_Intersects` | ✅ | |
| `ST_Contains` | ✅ | |
| `ST_Within` | ✅ | |
| `ST_Disjoint` | ✅ | |
| `ST_Equals` | ✅ | |
| `ST_Touches` | ✅ | |
| `ST_Crosses` | ✅ | |
| `ST_Overlaps` | ✅ | |
| `ST_Covers` | ✅ | |
| `ST_CoveredBy` | ✅ | |
| `ST_ContainsProperly` | ⏳ | Not yet. |
| `ST_OrderingEquals` | ✅ | |
| `ST_DWithin` | ✅ | Also `ST_DWithinSphere` / `ST_DWithinSpheroid`. |

## Measurements

| PostGIS function | Status | Notes |
|---|---|---|
| `ST_Area` | ✅ | |
| `ST_Length` | ✅ | |
| `ST_Perimeter` | ✅ | |
| `ST_Distance` | ✅ | |
| `ST_Azimuth` | ✅ | |
| `ST_MaxDistance` | ✅ | |
| `ST_LongestLine` | ✅ | |
| `ST_ShortestLine` | ✅ | |
| `ST_ClosestPoint` | ✅ | |
| `ST_HausdorffDistance` | ✅ | |
| `ST_FrechetDistance` | ✅ | |
| `ST_MinimumClearance` | ✅ | |
| `ST_MinimumClearanceLine` | ✅ | |
| `ST_DistanceSphere` | ✅ | Also `ST_LengthSphere`, `ST_AreaSphere`. |
| `ST_DistanceSpheroid` | ⚠️ | WGS84 only (Karney/GeographicLib). Custom spheroid parameter not supported. |
| `ST_LengthSpheroid` | ⚠️ | Same as above. |
| `ST_AreaSpheroid` | ⚠️ | Same as above. |

## Boolean set operations

| PostGIS function | Status | Notes |
|---|---|---|
| `ST_Intersection` | ✅ | |
| `ST_Union` | ✅ | Scalar + aggregate (`ST_Union_Agg`). |
| `ST_Difference` | ✅ | |
| `ST_SymDifference` | ✅ | |
| `ST_Intersection_Agg` | ✅ | Aggregate. |

## Topology / validity

| PostGIS function | Status | Notes |
|---|---|---|
| `ST_MakeValid` | ✅ | GEOS `make_valid` (canonical PostGIS engine). |
| `ST_Node` | ✅ | GEOS. |
| `ST_Polygonize` | ✅ | GEOS. |
| `ST_BuildArea` | ✅ | GEOS. |
| `ST_Snap` | ✅ | GEOS `snap` (canonical PostGIS engine). |
| `ST_Subdivide` | ✅ | Local. |

## Geometry processing

| PostGIS function | Status | Notes |
|---|---|---|
| `ST_Buffer` | ✅ | |
| `ST_Simplify` | ✅ | RDP. |
| `ST_SimplifyPreserveTopology` | ✅ | |
| `ST_SimplifyVW` | ✅ | Visvalingam-Whyatt. |
| `ST_ConvexHull` | ✅ | |
| `ST_ConcaveHull` | ✅ | |
| `ST_OrientedEnvelope` | ✅ | |
| `ST_MinimumBoundingCircle` | ✅ | |
| `ST_TriangulatePolygon` | ✅ | |
| `ST_DelaunayTriangles` | ✅ | |
| `ST_VoronoiPolygons` | ✅ | GEOS (bounded). |
| `ST_VoronoiLines` | ✅ | Local. |
| `ST_GeneratePoints` | ✅ | Seeded random. |

## Editing

| PostGIS function | Status | Notes |
|---|---|---|
| `ST_Translate` | ✅ | |
| `ST_Scale` | ✅ | |
| `ST_Rotate` | ✅ | |
| `ST_Affine` | ✅ | 6-param 2D. |
| `ST_FlipCoordinates` | ✅ | Routes to literal kernel. |
| `ST_Reverse` | ✅ | Routes to literal kernel. |
| `ST_Segmentize` | ✅ | Routes to literal kernel. |
| `ST_Force2D` | ✅ | Routes to literal kernel. |
| `ST_Force3D` / `ST_Force3DZ` | ⚠️ | Routes to literal kernel. Requires explicit z-value parameter (PostGIS defaults to 0). |
| `ST_Force3DM` | ⚠️ | Same delta: explicit m-value required. |
| `ST_Force4D` | ⚠️ | Same delta: explicit z,m values required. |
| `ST_ForceCollection` | ✅ | |
| `ST_ForcePolygonCW` / `CCW` | ✅ | |
| `ST_ForceRHR` | ✅ | |
| `ST_Multi` | ✅ | |
| `ST_Normalize` | ✅ | |
| `ST_RemoveRepeatedPoints` | ✅ | |
| `ST_SetPoint` | ⏳ | Not yet. |
| `ST_SnapToGrid` | ✅ | |
| `ST_CollectionExtract` | ✅ | |
| `ST_LineMerge` | ✅ | |
| `ST_LineSubstring` | ✅ | Routes to literal kernel. |
| `ST_LineInterpolatePoint` | ✅ | |
| `ST_LineLocatePoint` | ✅ | |
| `ST_Project` | ✅ | |
| `ST_RemovePoint` | ✅ | |
| `ST_AddPoint` | ✅ | |

## CRS / projection

| PostGIS function | Status | Notes |
|---|---|---|
| `ST_Transform` | ✅ | PROJ (bundled static). `ST_Transform(geom, from_srid, to_srid)`. |
| `ST_SetSRID` | ✅ | Routes to literal kernel. Sets SRID at type level. |
| `ST_SRID` | ✅ | Routes to literal kernel. Returns 0 (SRID-less BLOB). |

## Aggregates

| PostGIS function | Status | Notes |
|---|---|---|
| `ST_Collect` | ⚠️ | Aggregate only. DuckDB cannot overload scalar + aggregate on same name. Scalar `ST_Collect(g1, g2)` unavailable. |
| `ST_Union_Agg` | ✅ | Cascaded polygonal union. |
| `ST_Envelope_Agg` | ✅ | Bbox union. |
| `ST_MakeLine_Agg` | ✅ | Points → LineString. |
| `ST_Intersection_Agg` | ✅ | Cascaded polygonal intersection. |
| `ST_MemUnion` | 🔄 | Use `ST_Union_Agg`. |

## Set-returning / table functions

| PostGIS function | Status | Notes |
|---|---|---|
| `ST_Dump` | ✅ | Returns `(path, geom)` rows. |
| `ST_DumpPoints` | ✅ | Returns `(path, geom)` per vertex. |
| `ST_DumpSegments` | ✅ | Returns `(path, geom)` per edge. |
| `ST_DumpRings` | ⏳ | Not yet. |

## Raster

| PostGIS function | Status | Notes |
|---|---|---|
| `ST_RasterInfo` | ✅ | Extension-specific: metadata table function. |
| `ST_RasterStats` | ✅ | Extension-specific: per-band statistics. |
| `ST_RasterTransform` | ✅ | Extension-specific: GeoTransform + spatial bounds. |
| `ST_PixelData` | ✅ | Extension-specific: `(row, col, value)` pixel streaming. |
| `ST_Value` | ⏳ | Not yet (point sampling). |
| `ST_Clip` | ⏳ | Not yet. |
| `ST_AsRaster` | ⏳ | Not yet. |
| `ST_MapAlgebra` | ➖ | DuckDB SQL is the map algebra engine (via `ST_PixelData`). |

## Spatial join

| PostGIS function | Status | Notes |
|---|---|---|
| `&&` operator | 🔄 | Use bbox columns + DuckDB predicates, or `sedona_join`. |
| `<->` KNN | 🔄 | Use `sedona_join` or cross-join with `ST_Distance` + `ORDER BY` + `LIMIT`. |
| `sedona_join` | ✅ | Extension-specific: R-tree spatial join over spilled parquet. |

## Intentionally out of scope

| Feature | Status | Reason |
|---|---|---|
| PostgreSQL GiST/R-tree planner hooks | ➖ | DuckDB C API has no join-planner hooks. |
| PostGIS topology schema | ➖ | PostgreSQL-specific subsystem. |
| Tiger geocoder | ➖ | PostgreSQL-specific. |
| Address standardizer | ➖ | PostgreSQL-specific. |
| SFCGAL 3D solids/surfaces | ➖ | No mature Rust binding. |
| Raster map-algebra expression language | ➖ | DuckDB SQL is the expression language. |
