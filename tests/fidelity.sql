-- SPDX-License-Identifier: Apache-2.0
-- Fidelity harness: compare the extension's own ST_* reimplementation against
-- the LITERAL Apache SedonaDB kernel (sedona_*) over a diverse geometry corpus.
-- Prints only MISMATCH rows (a clean diff). An empty mismatch set means the two
-- implementations agree everywhere checked. Known-acceptable formatting
-- differences are normalized (envelope compared by area, not ring text).
--
-- Run with:
--   LD_LIBRARY_PATH=<gdal-lib> duckdb -unsigned -cmd "LOAD '<ext>';" < tests/fidelity.sql
.bail off
.mode list

-- (extension loaded via `duckdb -cmd "LOAD '<ext>';"` by tests/run_sql.sh)

-- Corpus: point, line, polygon (with hole), multipoint, collection, empty, nested.
WITH corpus(label, geom) AS (
    SELECT * FROM (VALUES
        ('point',      st_geomfromtext('POINT(1 2)')),
        ('line',       st_geomfromtext('LINESTRING(0 0,1 1,2 2,3 3)')),
        ('polygon',    st_geomfromtext('POLYGON((0 0,4 0,4 4,0 4,0 0),(1 1,2 1,2 2,1 2,1 1))')),
        ('multipoint', st_geomfromtext('MULTIPOINT((0 0),(1 1))')),
        ('collection', st_geomfromtext('GEOMETRYCOLLECTION(POINT(1 2),LINESTRING(3 4,5 6))')),
        ('empty',      st_geomfromtext('POINT EMPTY')),
        ('nested',     st_geomfromtext('GEOMETRYCOLLECTION(GEOMETRYCOLLECTION(POINT(9 9)))'))
    )
),
-- Side-by-side for each scalar accessor/predicate.
cmp(fn, label, local_v, sedona_v) AS (
    SELECT 'dimension',    label, st_dimension(geom)::VARCHAR,    sedona_st_dimension(geom)::VARCHAR    FROM corpus
    UNION ALL SELECT 'isempty',      label, st_isempty(geom)::VARCHAR,      sedona_st_isempty(geom)::VARCHAR      FROM corpus
    UNION ALL SELECT 'isclosed',     label, st_isclosed(geom)::VARCHAR,     sedona_st_isclosed(geom)::VARCHAR     FROM corpus
    UNION ALL SELECT 'geometrytype', label, st_geometrytype(geom),          sedona_st_geometrytype(geom)          FROM corpus
    UNION ALL SELECT 'numpoints',    label, st_numpoints(geom)::VARCHAR,    sedona_st_numpoints(geom)::VARCHAR    FROM corpus
)
SELECT 'MISMATCH' AS kind, fn, label, local_v AS local, sedona_v AS sedona
FROM cmp
WHERE local_v IS DISTINCT FROM sedona_v;

SELECT CASE WHEN (SELECT count(*) FROM cmp WHERE local_v IS DISTINCT FROM sedona_v) = 0
            THEN 'PASS scalar accessors/predicates agree'
            ELSE 'FAIL: '||(SELECT count(*) FROM cmp WHERE local_v IS DISTINCT FROM sedona_v)||' scalar mismatch(es) above' END;

-- Ordinate accessors (only meaningful on points): both must agree within 1e-9.
WITH pts AS (
    SELECT st_geomfromtext('POINT(1 2)') AS g
    UNION ALL SELECT st_geomfromtext('POINT(-3 5.5)')
    UNION ALL SELECT st_geomfromtext('POINT(0 0)')
)
SELECT CASE WHEN (SELECT count(*) FROM pts WHERE
        abs(st_x(g) - sedona_st_x(g)) > 1e-9 OR abs(st_y(g) - sedona_st_y(g)) > 1e-9
        OR abs(st_xmin(g) - sedona_st_xmin(g)) > 1e-9 OR abs(st_xmax(g) - sedona_st_xmax(g)) > 1e-9) = 0
            THEN 'PASS point ordinate accessors agree'
            ELSE 'FAIL point ordinate mismatch' END;

-- Envelope: compare by area (ring winding may legitimately differ CCW/CW).
WITH polys(g) AS (
    SELECT st_geomfromtext('POLYGON((0 0,4 0,4 4,0 4,0 0))')
    UNION ALL SELECT st_geomfromtext('POLYGON((1 1,3 1,3 3,1 3,1 1))')
)
SELECT CASE WHEN (SELECT count(*) FROM polys WHERE abs(st_area(st_envelope(g)) - st_area(sedona_st_envelope(g))) > 1e-9) = 0
            THEN 'PASS envelope area agrees' ELSE 'FAIL envelope mismatch' END;

-- === Extended matrix: overlapping transforms / constructors / measurements ===
-- Constructor: ST_Point
SELECT CASE WHEN st_astext(st_point(3,4)) = st_astext(sedona_st_point(3,4)) THEN 'PASS' ELSE 'FAIL st_point' END;
-- ST_Translate / ST_Scale (compare by coordinate extraction, robust to fmt)
WITH p AS (SELECT st_geomfromtext('POINT(1 2)') AS g)
SELECT CASE WHEN abs(st_x(st_translate(g,5,-1)) - st_x(sedona_st_translate(g,5,-1))) < 1e-9
             AND abs(st_y(st_scale(g,2,3)) - st_y(sedona_st_scale(g,2,3))) < 1e-9
            THEN 'PASS' ELSE 'FAIL translate/scale' END FROM p;
-- ST_Azimuth (radians, compare within 1e-9)
WITH ab AS (SELECT st_geomfromtext('POINT(0 0)') AS a, st_geomfromtext('POINT(1 1)') AS b)
SELECT CASE WHEN abs(st_azimuth(a,b) - sedona_st_azimuth(a,b)) < 1e-9 THEN 'PASS' ELSE 'FAIL azimuth' END FROM ab;
-- ST_ZMFlag (2D geometry → 0 in both)
SELECT CASE WHEN st_zmflag(st_geomfromtext('POINT(1 2)')) = sedona_st_zmflag(st_geomfromtext('POINT(1 2)')) THEN 'PASS' ELSE 'FAIL zmflag' END;
-- ST_MakeLine endpoint agreement
WITH ab AS (SELECT st_geomfromtext('POINT(0 0)') AS a, st_geomfromtext('POINT(1 1)') AS b)
SELECT CASE WHEN st_astext(st_endpoint(st_makeline(a,b))) = st_astext(sedona_st_endpoint(sedona_st_makeline(a,b))) THEN 'PASS' ELSE 'FAIL makeline' END FROM ab;
-- ST_LineSubstring: compare by length (vertex placement may differ in edge cases)
WITH ln AS (SELECT st_geomfromtext('LINESTRING(0 0, 10 0)') AS g)
SELECT CASE WHEN abs(st_length(st_linesubstring(g,0.0,0.5)) - st_length(sedona_st_linesubstring(g,0.0,0.5))) < 1e-9
            THEN 'PASS' ELSE 'FAIL linesubstring' END FROM ln;
-- ST_AsBinary: both must return identical WKB bytes for the same geometry
SELECT CASE WHEN st_asbinary(st_geomfromtext('POINT(1 2)')) = sedona_st_asbinary(st_geomfromtext('POINT(1 2)')) THEN 'PASS' ELSE 'FAIL asbinary' END;
