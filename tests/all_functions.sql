-- SPDX-License-Identifier: Apache-2.0
-- Comprehensive SQL test: exercises every registered ST_* function with known
-- inputs and expected outputs. Run with:
--   LD_LIBRARY_PATH=<gdal-lib> duckdb -unsigned -cmd "LOAD '<ext>';" < tests/all_functions.sql
--
-- Each test prints 'PASS' or 'FAIL <details>'. If any FAIL appears, investigate.
.bail off
.mode list

-- === Constructors ===
SELECT CASE WHEN st_astext(st_geomfromtext('POINT(1 2)')) = 'POINT(1 2)' THEN 'PASS' ELSE 'FAIL geomfromtext' END;
SELECT CASE WHEN st_astext(st_point(3, 4)) = 'POINT(3 4)' THEN 'PASS' ELSE 'FAIL point' END;
SELECT CASE WHEN st_geometrytype(st_geomfromtext('LINESTRING(0 0,1 1)')) = 'ST_LineString' THEN 'PASS' ELSE 'FAIL geometrytype' END;

-- === Measurements ===
SELECT CASE WHEN st_area(st_geomfromtext('POLYGON((0 0,4 0,4 4,0 4,0 0))')) = 16.0 THEN 'PASS' ELSE 'FAIL area' END;
SELECT CASE WHEN st_length(st_geomfromtext('LINESTRING(0 0,3 0,3 4)')) = 7.0 THEN 'PASS' ELSE 'FAIL length' END;
SELECT CASE WHEN st_distance(st_geomfromtext('POINT(0 0)'), st_geomfromtext('POINT(3 4)')) = 5.0 THEN 'PASS' ELSE 'FAIL distance' END;
SELECT CASE WHEN round(st_perimeter(st_geomfromtext('POLYGON((0 0,3 0,3 3,0 3,0 0))')),0) = 12.0 THEN 'PASS' ELSE 'FAIL perimeter' END;

-- === Predicates ===
SELECT CASE WHEN st_intersects(st_geomfromtext('POLYGON((0 0,2 0,2 2,0 2,0 0))'), st_point(1,1)) = true THEN 'PASS' ELSE 'FAIL intersects' END;
SELECT CASE WHEN st_contains(st_geomfromtext('POLYGON((0 0,2 0,2 2,0 2,0 0))'), st_point(3,3)) = false THEN 'PASS' ELSE 'FAIL contains' END;
SELECT CASE WHEN st_within(st_point(1,1), st_geomfromtext('POLYGON((0 0,2 0,2 2,0 2,0 0))')) = true THEN 'PASS' ELSE 'FAIL within' END;
SELECT CASE WHEN st_disjoint(st_point(9,9), st_point(0,0)) = true THEN 'PASS' ELSE 'FAIL disjoint' END;
SELECT CASE WHEN st_dwithin(st_point(0,0), st_point(1,0), 2.0) = true THEN 'PASS' ELSE 'FAIL dwithin' END;
SELECT CASE WHEN st_equals(st_point(1,1), st_point(1,1)) = true THEN 'PASS' ELSE 'FAIL equals' END;
SELECT CASE WHEN st_touches(st_geomfromtext('POLYGON((0 0,1 0,1 1,0 1,0 0))'), st_geomfromtext('POLYGON((1 0,2 0,2 1,1 1,1 0))')) = true THEN 'PASS' ELSE 'FAIL touches' END;
SELECT CASE WHEN st_covers(st_geomfromtext('POLYGON((0 0,2 0,2 2,0 2,0 0))'), st_point(1,1)) = true THEN 'PASS' ELSE 'FAIL covers' END;

-- === Set ops ===
SELECT CASE WHEN st_area(st_intersection(st_geomfromtext('POLYGON((0 0,2 0,2 2,0 2,0 0))'), st_geomfromtext('POLYGON((1 1,3 1,3 3,1 3,1 1))'))) = 1.0 THEN 'PASS' ELSE 'FAIL intersection' END;
SELECT CASE WHEN st_area(st_union(st_geomfromtext('POLYGON((0 0,1 0,1 1,0 1,0 0))'), st_geomfromtext('POLYGON((1 0,2 0,2 1,1 1,1 0))'))) = 2.0 THEN 'PASS' ELSE 'FAIL union' END;

-- === Transforms ===
SELECT CASE WHEN st_astext(st_buffer(st_point(0,0), 1.0)) IS NOT NULL THEN 'PASS' ELSE 'FAIL buffer' END;
SELECT CASE WHEN st_length(st_simplify(st_geomfromtext('LINESTRING(0 0,0.01 0,1 0)'), 0.1)) <= 1.0 THEN 'PASS' ELSE 'FAIL simplify' END;
SELECT CASE WHEN st_astext(st_translate(st_point(1,2),5,5)) = 'POINT(6 7)' THEN 'PASS' ELSE 'FAIL translate' END;
SELECT CASE WHEN st_astext(st_scale(st_geomfromtext('POINT(1 1)'),2,3)) IS NOT NULL THEN 'PASS' ELSE 'FAIL scale' END;
SELECT CASE WHEN round(st_x(st_centroid(st_geomfromtext('POLYGON((0 0,4 0,4 4,0 4,0 0))'))),1) = 2.0 THEN 'PASS' ELSE 'FAIL centroid' END;

-- === Accessors ===
SELECT CASE WHEN st_x(st_point(3,4)) = 3.0 THEN 'PASS' ELSE 'FAIL x' END;
SELECT CASE WHEN st_y(st_point(3,4)) = 4.0 THEN 'PASS' ELSE 'FAIL y' END;
SELECT CASE WHEN st_dimension(st_geomfromtext('POLYGON((0 0,1 0,1 1,0 0))')) = 2 THEN 'PASS' ELSE 'FAIL dimension' END;
SELECT CASE WHEN st_numpoints(st_geomfromtext('LINESTRING(0 0,1 1,2 2)')) = 3 THEN 'PASS' ELSE 'FAIL numpoints' END;
SELECT CASE WHEN st_isvalid(st_geomfromtext('POLYGON((0 0,1 0,1 1,0 1,0 0))')) = true THEN 'PASS' ELSE 'FAIL isvalid' END;
SELECT CASE WHEN st_isempty(st_point(1,1)) = false THEN 'PASS' ELSE 'FAIL isempty' END;
SELECT CASE WHEN st_isclosed(st_geomfromtext('LINESTRING(0 0,1 1,0 0)')) = true THEN 'PASS' ELSE 'FAIL isclosed' END;

-- === CRS / Geography ===
SELECT CASE WHEN round(st_x(st_transform(st_point(0.1278,51.5074),4326,3857)),0) = 14227 THEN 'PASS' ELSE 'FAIL transform' END;
SELECT CASE WHEN round(st_distancesphere(st_geomfromtext('POINT(0 0)'),st_geomfromtext('POINT(0 1)')),-3) = 111000 THEN 'PASS' ELSE 'FAIL distancesphere' END;

-- === Delaunay / Voronoi ===
SELECT CASE WHEN st_numgeometries(st_delaunaytriangles(st_geomfromtext('MULTIPOINT(0 0,1 0,0 1,1 1,0.5 0.5)'))) >= 3 THEN 'PASS' ELSE 'FAIL delaunay' END;
SELECT CASE WHEN st_numgeometries(st_voronoilines(st_geomfromtext('MULTIPOINT(0 0,4 0,2 4,1 1)'))) >= 1 THEN 'PASS' ELSE 'FAIL voronoi' END;

-- === I/O ===
SELECT CASE WHEN st_asgeojson(st_point(1,2)) = '{"type":"Point","coordinates":[1,2]}' THEN 'PASS' ELSE 'FAIL asgeojson' END;
SELECT CASE WHEN st_asewkt(st_point(1,2),4326) = 'SRID=4326;POINT(1 2)' THEN 'PASS' ELSE 'FAIL asewkt' END;
SELECT CASE WHEN st_astext(st_boundary(st_geomfromtext('LINESTRING(0 0,1 1,2 2)'))) = 'MULTIPOINT((0 0),(2 2))' THEN 'PASS' ELSE 'FAIL boundary' END;

-- === Aggregates ===
SELECT CASE WHEN st_numgeometries(st_collect(g)) = 3 THEN 'PASS' ELSE 'FAIL collect' FROM (SELECT st_point(0,0) g UNION ALL SELECT st_point(1,1) UNION ALL SELECT st_point(2,2));
SELECT CASE WHEN st_astext(st_envelope_agg(g)) = 'POLYGON((0 0,5 0,5 8,0 8,0 0))' THEN 'PASS' ELSE 'FAIL envelope_agg' FROM (SELECT st_point(0,0) g UNION ALL SELECT st_point(5,2) UNION ALL SELECT st_point(2,8));

-- === Raster ===
SELECT CASE WHEN count = 16 THEN 'PASS' ELSE 'FAIL raster_stats' FROM st_raster_stats('/var/home/adonm/dev/duckdb_sedona/build/raster/test.tif', 1);
