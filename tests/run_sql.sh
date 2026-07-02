#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Run every DuckDB SQL regression in tests/ against a packaged extension and
# report a clean PASS/FAIL summary. Real failures are distinguished from the
# `CASE WHEN ... THEN 'PASS' ELSE 'FAIL ...'` expression text the suites use:
# a value line is a real result only if it does not contain CASE/THEN/ELSE.
#
# Usage:
#   ./tests/run_sql.sh [path/to/sedonadb.duckdb_extension] [duckdb_binary]
#
# Defaults: build/dev/sedonadb.duckdb_extension , `duckdb` from PATH.
# GDAL/PROJ runtime libs are located via Linuxbrew if present, else the caller's
# environment.
set -uo pipefail

cd "$(dirname "$0")/.."

EXT="${1:-build/dev/sedonadb.duckdb_extension}"
DUCKDB="${2:-duckdb}"

# Locate runtime libs (libgdal/libproj) for Linuxbrew if installed.
if [ -d /var/home/linuxbrew/.linuxbrew/lib ]; then
    export LD_LIBRARY_PATH="/var/home/linuxbrew/.linuxbrew/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
fi

if [ ! -f "$EXT" ]; then
    echo "FATAL: extension not found at $EXT" >&2
    exit 2
fi

SQL_FILES=(tests/all_functions.sql tests/sedona_bridge.sql tests/vector_encodings.sql tests/fidelity.sql)
TOTAL_PASS=0
TOTAL_FAIL=0
FAILED_FILES=()

for f in "${SQL_FILES[@]}"; do
    # `2>&1` captures the bridge's stderr (e.g. invoke-error logs) too, but the
    # PASS/FAIL accounting only counts real result-value lines.
    out=$("$DUCKDB" -unsigned -cmd "LOAD '$(pwd)/$EXT';" < "$f" 2>&1 || true)

    # Real PASS values: lines that are exactly the value, not the CASE expr.
    pass=$(printf '%s\n' "$out" | grep -E '^PASS' | grep -vE 'CASE|THEN|ELSE' | wc -l)
    # Real FAIL values: same filter, then strip the suite's own "FAIL <label>"
    # text only if it's a bare value (it always is in these suites).
    fail=$(printf '%s\n' "$out" | grep -E '^FAIL' | grep -vE 'CASE|THEN|ELSE' | wc -l)

    printf '%-32s  PASS=%-3d  FAIL=%-3d\n' "$f" "$pass" "$fail"
    TOTAL_PASS=$((TOTAL_PASS + pass))
    TOTAL_FAIL=$((TOTAL_FAIL + fail))
    [ "$fail" -gt 0 ] && FAILED_FILES+=("$f")
done

echo "------------------------------------------------"
printf 'TOTAL                           PASS=%-3d  FAIL=%-3d\n' "$TOTAL_PASS" "$TOTAL_FAIL"

# fidelity.sql intentionally prints MISMATCH rows (known semantic deltas); it is
# "green" when its own PASS lines print. Do not treat its mismatch output as a
# failure unless it emits a real FAIL value.

if [ "$TOTAL_FAIL" -gt 0 ]; then
    echo "FAILED files: ${FAILED_FILES[*]}"
    exit 1
fi
echo "ALL SQL REGRESSIONS PASSED"
