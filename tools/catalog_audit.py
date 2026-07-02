#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Generate a catalog audit from src/registry.rs.

Parses the macro/builder registrations and groups every SQL function name by
provenance: literal SedonaDB bridge, local geo, GEOS, PROJ, GDAL/raster,
aggregate, table function, or extension-specific.

Usage:
    python3 tools/catalog_audit.py [--markdown] [path/to/registry.rs]

With --markdown, emits a markdown table suitable for docs.  Without it, emits
plain text counts.
"""
from __future__ import annotations

import re
import sys
from collections import defaultdict
from dataclasses import dataclass, field
from pathlib import Path


@dataclass
class CatalogEntry:
    name: str
    provenance: str  # "literal-sedonadb", "local-geo", "geos", "proj", "gdal-raster", "aggregate", "table-function", "extension"
    signature: str = ""
    sedona_name: str = ""  # the SedonaDB kernel name (for bridge entries)


def classify(
    name: str,
    macro: str,
    sedona_name: str,
    is_aggregate: bool = False,
    is_table_fn: bool = False,
) -> str:
    if is_aggregate:
        return "aggregate"
    if is_table_fn:
        if "raster" in name or "pixeldata" in name:
            return "gdal-raster"
        if name == "sedona_join":
            return "extension"
        return "table-function"
    if sedona_name:
        return "literal-sedonadb"
    # Local registrations — classify by backend hint from comments / known names.
    if name in ("st_transform",):
        return "proj"
    if name in (
        "st_node", "st_polygonize", "st_buildarea", "st_voronoipolygons",
        "st_snap", "st_makevalid",
    ):
        return "geos"
    return "local-geo"


# Regex patterns for all registration macros.
# Each captures the SQL name (first quoted string) and optionally the SedonaDB
# kernel name (second quoted string, only for register_sedona_* macros).
MACRO_PATTERNS: list[tuple[str, re.Pattern[str]]] = [
    # SedonaDB bridge macros (two-arg: sql_name, sedona_kernel)
    ("sedona", re.compile(
        r'register_sedona_\w+!\(\s*"([^"]+)"\s*,\s*"([^"]+)"'
    )),
    # Local macros (one-arg: sql_name)
    ("local", re.compile(
        r'register_(?:unary_geom|binary_geom|predicate|geom_double|geom_varchar|'
        r'geom_int|geom_bool|binary_double|geom_int_to_geom|geom_double_to_geom|'
        r'geom_double2_to_geom|geom_double6_to_geom|geom_int2_to_geom|'
        r'geom_int_to_varchar|str_geom|doubles2_geom|doubles4_geom|'
        r'geom_double3_to_geom)!'
        r'\(\s*"([^"]+)"'
    )),
]

# Inline builder patterns (for manually-registered functions).
BUILDER_RE = re.compile(r'(?:Scalar|Aggregate|Table)FunctionBuilder::new\(\s*"([^"]+)"')


def parse_registry(path: Path) -> list[CatalogEntry]:
    """Parse registry.rs and return all catalog entries."""
    text = path.read_text()

    # Track which lines are inside the bridge section (after the bridge comment).
    bridge_section = False

    # First pass: find all inline builder registrations and their context.
    # These are GEOS, aggregate, table-function, and a few special cases.
    entries: list[CatalogEntry] = []
    seen_names: set[str] = set()

    lines = text.splitlines()
    i = 0
    while i < len(lines):
        line = lines[i]

        # Detect bridge section start.
        if "Literal Apache SedonaDB bridge" in line:
            bridge_section = True

        # Check for macro-based registrations.
        # SedonaDB bridge macros (two-arg).
        for pattern_name, pattern in MACRO_PATTERNS:
            m = pattern.search(line)
            if m:
                if pattern_name == "sedona":
                    sql_name = m.group(1)
                    sedona_name = m.group(2)
                    prov = "literal-sedonadb"
                else:
                    sql_name = m.group(1)
                    sedona_name = ""
                    prov = classify(sql_name, "", "")

                if sql_name not in seen_names:
                    entries.append(CatalogEntry(
                        name=sql_name,
                        provenance=prov,
                        sedona_name=sedona_name,
                    ))
                    seen_names.add(sql_name)
                break
        else:
            # Check for inline builder registrations.
            bm = BUILDER_RE.search(line)
            if bm:
                name = bm.group(1)
                if name in seen_names:
                    i += 1
                    continue
                # Look at context to classify.
                is_agg = "AggregateFunctionBuilder" in line
                is_table = "TableFunctionBuilder" in line
                prov = classify(name, "", "", is_aggregate=is_agg, is_table_fn=is_table)
                entries.append(CatalogEntry(name=name, provenance=prov))
                seen_names.add(name)

        i += 1

    return entries


def group_by_provenance(entries: list[CatalogEntry]) -> dict[str, list[str]]:
    groups: dict[str, list[str]] = defaultdict(list)
    for e in entries:
        groups[e.provenance].append(e.name)
    for k in groups:
        groups[k].sort()
    return groups


def print_summary(entries: list[CatalogEntry], markdown: bool = False) -> None:
    groups = group_by_provenance(entries)

    # Count routed public st_* (functions that share a literal kernel).
    st_names = {e.name for e in entries if e.name.startswith("st_") and not e.name.startswith("sedona_")}
    sedona_st_names = {e.name for e in entries if e.name.startswith("sedona_st_")}
    # A routed function has both st_X and sedona_st_X.
    routed = set()
    for e in entries:
        if e.provenance == "literal-sedonadb" and e.name.startswith("st_") and not e.name.startswith("sedona_"):
            routed.add(e.name)

    total = len(entries)
    st_count = len(st_names)
    sedona_count = len([e for e in entries if e.name.startswith("sedona_")])
    routed_count = len(routed)

    if markdown:
        print(f"| Metric | Count |")
        print(f"|--------|-------|")
        print(f"| Total SQL functions | {total} |")
        print(f"| Public `st_*` | {st_count} |")
        print(f"| Literal `sedona_st_*` | {len(sedona_st_names)} |")
        print(f"| Extension-specific | {total - st_count - len(sedona_st_names)} |")
        print(f"| `st_*` routed to literal kernel | {routed_count} |")
        print()
        print("### By backend")
        print(f"| Backend | Functions | Count |")
        print(f"|---------|-----------|-------|")
        for prov in ["literal-sedonadb", "local-geo", "geos", "proj", "gdal-raster", "aggregate", "table-function", "extension"]:
            if prov in groups:
                label = prov.replace("-", " ").title()
                fns = ", ".join(f"`{f}`" for f in groups[prov])
                print(f"| {label} | {fns} | {len(groups[prov])} |")
        print()
        if routed:
            print("### `st_*` functions routed to literal SedonaDB kernel")
            print(", ".join(f"`{f}`" for f in sorted(routed)))
    else:
        print(f"Catalog audit: {total} functions")
        print(f"  st_*:       {st_count}")
        print(f"  sedona_st_*: {len(sedona_st_names)}")
        print(f"  routed:     {routed_count}")
        print()
        for prov in ["literal-sedonadb", "local-geo", "geos", "proj", "gdal-raster", "aggregate", "table-function", "extension"]:
            if prov in groups:
                print(f"  {prov} ({len(groups[prov])}):")
                for f in groups[prov]:
                    print(f"    {f}")
                print()


def main() -> None:
    markdown = "--markdown" in sys.argv
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    registry_path = Path(args[0]) if args else Path("src/registry.rs")

    if not registry_path.exists():
        # Try relative to script location.
        registry_path = Path(__file__).resolve().parent.parent / "src" / "registry.rs"

    if not registry_path.exists():
        print(f"Error: cannot find registry.rs at {registry_path}", file=sys.stderr)
        sys.exit(1)

    entries = parse_registry(registry_path)
    print_summary(entries, markdown=markdown)


if __name__ == "__main__":
    main()
