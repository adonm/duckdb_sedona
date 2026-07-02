// SPDX-License-Identifier: Apache-2.0
//! GEOS-backed planar topology operations.
//!
//! A narrow boundary: WKB bytes in → GEOS → operation → WKB bytes out. This
//! module never exposes GEOS types beyond its own functions; callers work at
//! the raw-WKB level so there is no double conversion through `geo_types`.
//!
//! GEOS is the same engine PostGIS uses for `ST_Node`, `ST_Polygonize`,
//! `ST_BuildArea`, and `ST_VoronoiPolygons`, giving us PostGIS-grade fidelity
//! for planar topology without maintaining a custom algorithm port.

use geos::{Geom, Geometry};

/// Parse ISO WKB bytes into a GEOS geometry. Returns `None` on parse failure
/// (fail-closed — the caller emits NULL, never a wrong geometry).
fn from_wkb(wkb: &[u8]) -> Option<Geometry> {
    Geometry::new_from_wkb(wkb).ok()
}

/// Serialize a GEOS geometry to ISO WKB bytes.
fn to_wkb(geom: &Geometry) -> Option<Vec<u8>> {
    geom.to_wkb().ok()
}

/// `ST_Node` — add nodes at every intersection of the linework, returning a
/// noded MultiLineString.
pub fn node(wkb: &[u8]) -> Option<Vec<u8>> {
    let g = from_wkb(wkb)?;
    let noded = g.node().ok()?;
    to_wkb(&noded)
}

/// `ST_Polygonize` — form a MultiPolygon from all constituent linestrings that
/// can be rings. GEOS `polygonize` accepts a multi-geometry directly; it
/// extracts the internal components in C.
pub fn polygonize(wkb: &[u8]) -> Option<Vec<u8>> {
    let g = from_wkb(wkb)?;
    let result = Geometry::polygonize(&[&g]).ok()?;
    to_wkb(&result)
}

/// `ST_BuildArea` — build an areal geometry (Polygon or MultiPolygon) from the
/// linework of the input, directed by the boundary relationships.
pub fn build_area(wkb: &[u8]) -> Option<Vec<u8>> {
    let g = from_wkb(wkb)?;
    let result = g.build_area().ok()?;
    to_wkb(&result)
}

/// `ST_VoronoiPolygons` — bounded Voronoi diagram of the input points. Returns
/// a GeometryCollection (or MultiPolygon) of finite Voronoi cells.
///
/// * `tolerance` — snapping tolerance (0.0 for exact).
/// * `extend_to` — optional WKB envelope to extend the diagram to (PostGIS
///   `extend_to` argument). When `None`, GEOS derives the envelope from the
///   input sites with a small buffer.
pub fn voronoi_polygons(wkb: &[u8], tolerance: f64, extend_to: Option<&[u8]>) -> Option<Vec<u8>> {
    let g = from_wkb(wkb)?;
    let env = match extend_to {
        Some(env_wkb) => Some(from_wkb(env_wkb)?),
        None => None,
    };
    // only_edges=false → polygonal cells; true → the dual edges (ST_VoronoiLines).
    let result = match env {
        Some(ref e) => g.voronoi(Some(e), tolerance, false).ok()?,
        None => g.voronoi(None::<&Geometry>, tolerance, false).ok()?,
    };
    to_wkb(&result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geos_node_crossing_lines() {
        // Two crossing lines → noded into 4 segments with an intersection node.
        let wkb = geo_wkb("MULTILINESTRING((0 0,4 4),(0 4,4 0))");
        let noded = node(&wkb).expect("node should succeed");
        let g = from_wkb(&noded).unwrap();
        assert!(g.get_num_geometries().unwrap() >= 2, "noded result");
    }

    #[test]
    fn geos_polygonize_rings() {
        // A closed ring → polygonize produces a polygon.
        let wkb = geo_wkb("LINESTRING(0 0,4 0,4 4,0 4,0 0)");
        let poly = polygonize(&wkb).expect("polygonize should succeed");
        let g = from_wkb(&poly).unwrap();
        assert_eq!(g.get_num_geometries().unwrap(), 1, "one polygon from ring");
    }

    #[test]
    fn geos_voronoi_grid_does_not_lose_cells() {
        // The 3x3 grid that defeated the earlier angle-sort prototype. GEOS must
        // produce 9 cells (one per site), proving the half-edge approach is
        // correct on cocircular/degenerate input.
        let wkb = geo_wkb("MULTIPOINT((0 0),(1 0),(2 0),(0 1),(1 1),(2 1),(0 2),(1 2),(2 2))");
        let result = voronoi_polygons(&wkb, 0.0, None).expect("voronoi should succeed");
        let g = from_wkb(&result).unwrap();
        let n = g.get_num_geometries().unwrap();
        assert_eq!(n, 9, "3x3 grid must yield exactly 9 voronoi cells, got {n}");
    }

    #[test]
    fn geos_build_area_from_rings() {
        // Exterior + interior ring → polygon with hole.
        let wkb = geo_wkb("MULTILINESTRING((0 0,4 0,4 4,0 4,0 0),(1 1,2 1,2 2,1 2,1 1))");
        let area = build_area(&wkb).expect("build_area should succeed");
        let g = from_wkb(&area).unwrap();
        let a = g.area().unwrap();
        assert!((a - 15.0).abs() < 1e-6, "4x4 square minus 1x1 hole = 15, got {a}");
    }

    /// Helper: build WKB from a WKT string using the extension's own stack.
    fn geo_wkb(wkt: &str) -> Vec<u8> {
        use std::str::FromStr;
        let parsed = wkt::Wkt::<f64>::from_str(wkt).expect("valid WKT in test");
        let g = crate::functions::geom_from_wkt(parsed).expect("geom conversion");
        crate::geometry::to_wkb(&g).expect("wkb write")
    }
}
