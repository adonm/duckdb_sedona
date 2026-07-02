#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Full release pipeline: build the release artifact, package the
# .duckdb_extension (512-byte DuckDB trailer), LOAD it, and smoke-test that both
# the local ST_* path and the literal Apache SedonaDB bridge path run in a real
# DuckDB session. Catches regressions that Rust unit tests can't (packaging,
# symbol export, runtime library resolution).
#
# Usage: ./ci/package-and-smoke.sh [duckdb_binary]
# Needs the GDAL/PROJ/libclang build env; auto-locates Linuxbrew if present.
set -euo pipefail

cd "$(dirname "$0")/.."

DUCKDB="${1:-duckdb}"
PLATFORM="linux_amd64"

# Build env (Linuxbrew provides GDAL 3.13.1 headers + libclang + runtime libs).
BREW=/var/home/linuxbrew/.linuxbrew
if [ -d "$BREW/lib" ]; then
    export PKG_CONFIG_PATH="$BREW/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
    if [ -z "${LIBCLANG_PATH:-}" ] || [ ! -e "${LIBCLANG_PATH:-}/libclang.so" ]; then
        export LIBCLANG_PATH="$(dirname "$(find "$BREW" -name libclang.so 2>/dev/null | head -1)")"
    fi
    export LD_LIBRARY_PATH="$BREW/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
fi

echo ">> cargo build --release"
cargo build --release

SO=target/release/libsedonadb.so
EXT=build/dev/sedonadb.duckdb_extension
mkdir -p build/dev

echo ">> package $SO -> $EXT ($PLATFORM)"
./target/release/sedonadb-package "$SO" "$EXT" "$PLATFORM"

echo ">> smoke-test in DuckDB ($DUCKDB)"
# Three smoke checks: local path, literal SedonaDB path, and an aggregate.
"$DUCKDB" -unsigned <<SQL
LOAD '$(pwd)/$EXT';
.mode list
SELECT CASE WHEN st_astext(st_geomfromtext('POINT(1 2)')) = 'POINT(1 2)'
            THEN 'PASS local' ELSE 'FAIL local' END;
SELECT CASE WHEN sedona_st_dimension(st_geomfromtext('POINT(1 2)')) = 0
            THEN 'PASS sedona' ELSE 'FAIL sedona' END;
SELECT CASE WHEN sedona_st_astext(st_geomfromtext('POINT(1 2)')) = 'POINT(1 2)'
            THEN 'PASS sedona-astext' ELSE 'FAIL sedona-astext' END;
SELECT CASE WHEN st_area(st_envelope_agg(g)) > 0
            THEN 'PASS aggregate' ELSE 'FAIL aggregate' END
FROM (SELECT st_geomfromtext('POLYGON((0 0,1 0,1 1,0 1,0 0))') AS g);
SQL

echo ">> smoke OK: packaged extension loads and runs both paths"
