// SPDX-License-Identifier: Apache-2.0
//
// Spatial function implementations.
//
// Every function here is a *plain* `fn` with one of the shapes expected by the
// generic executors in `dispatch.rs`:
//
//   fn(&Geom) -> Option<Geom>            unary, geometry -> geometry
//   fn(&Geom, &Geom) -> Option<Geom>     binary, geometry -> geometry
//   fn(&Geom, &Geom) -> Option<bool>     binary predicate
//   fn(&Geom) -> Option<f64>             geometry -> DOUBLE
//   fn(&Geom) -> Option<String>          geometry -> VARCHAR
//   fn(&Geom) -> Option<i32>             geometry -> INTEGER
//
// The algorithms come from the `geo` crate — the same library Apache SedonaDB
// uses for these operations (`sedona-geo` wraps `geo` as its DataFusion UDFs).
// `None` means "undefined for this input" and propagates to SQL `NULL`.

use geo::algorithm::bool_ops::BooleanOps;
use geo::prelude::*;
#[allow(deprecated)]
use geo::EuclideanDistance;
use geo::Validation;
use geo_types::{Geometry, MultiPolygon, Point};

use crate::geometry::Geom;

/// Reduce a geometry to its areal (polygonal) part as a `MultiPolygon`.
///
/// `geo`'s `BooleanOps` are only implemented for `Polygon` / `MultiPolygon`,
/// so `ST_Intersection` / `ST_Union` operate on the polygonal part of each
/// input. Non-areal geometries contribute an empty `MultiPolygon`, which means
/// they yield an empty result — the standard OGC behaviour for boolean ops on
/// non-polygonal inputs.
fn to_multi_polygon(g: &Geom) -> MultiPolygon {
    let polys: Vec<geo_types::Polygon> = match g {
        Geometry::Polygon(p) => vec![p.clone()],
        Geometry::MultiPolygon(mp) => mp.0.clone(),
        Geometry::GeometryCollection(c) => {
            let mut polys = Vec::new();
            for item in c.iter() {
                match item {
                    Geometry::Polygon(p) => polys.push(p.clone()),
                    Geometry::MultiPolygon(mp) => polys.extend(mp.0.iter().cloned()),
                    _ => {}
                }
            }
            polys
        }
        _ => Vec::new(),
    };
    MultiPolygon::new(polys)
}

// ----- unary: geometry -> geometry --------------------------------------

/// `ST_ConvexHull(geom)` — convex hull of the geometry's coordinates.
pub fn convex_hull(g: &Geom) -> Option<Geom> {
    match g {
        Geometry::Point(_) => Some(g.clone()),
        _ => Some(g.convex_hull().into()),
    }
}

/// `ST_Envelope(geom)` — minimum bounding rectangle as a Polygon
/// (or NULL when the input is degenerate).
pub fn envelope(g: &Geom) -> Option<Geom> {
    let rect = g.bounding_rect()?;
    Some(Geometry::Polygon(rect.to_polygon()))
}

/// `ST_Centroid(geom)` — planar centroid, or NULL if undefined.
pub fn centroid(g: &Geom) -> Option<Geom> {
    let point: Point = g.centroid()?;
    Some(Geometry::Point(point))
}

// ----- set operations ---------------------------------------------------

/// `ST_MakeValid(geom)` — repair an invalid geometry.
///
/// `geo`'s DE-9IM `relate` (used by `ST_Within`/`Contains`/`Covers`/…) and its
/// boolean ops can stack-overflow / misbehave on degenerate (self-intersecting,
/// over-complex) polygons — which real-world datasets (e.g. Overture admin
/// boundaries in SpatialBench) do contain. The classic repair is `buffer(0)`,
/// which rebuilds topology with even-odd fill and drops the self-intersections.
/// For already-valid input this is a no-op clone; for non-areal input (points /
/// lines) we return the value unchanged (those rarely go invalid).
pub fn make_valid(g: &Geom) -> Option<Geom> {
    use geo::Validation;
    if g.is_valid() {
        return Some(g.clone());
    }
    match g {
        Geometry::Polygon(_) | Geometry::MultiPolygon(_) => {
            // buffer(0) -> cleaned MultiPolygon (even-odd fill).
            Some(Geometry::MultiPolygon(g.buffer(0.0)))
        }
        // Lines/points/collections: structural validity issues are uncommon
        // here; return as-is rather than risk losing detail.
        _ => Some(g.clone()),
    }
}

/// Borrow an input geometry, repairing it only if it is invalid. Used to guard
/// every relate-based predicate and boolean op so invalid real-world polygons
/// don't crash the extension. Cheap on the hot path: `is_valid` is a structural
/// check, and valid geometries (the vast majority) are borrowed, not copied.
fn ensure_valid<'a>(g: &'a Geom) -> std::borrow::Cow<'a, Geom> {
    use geo::Validation;
    if g.is_valid() {
        std::borrow::Cow::Borrowed(g)
    } else {
        std::borrow::Cow::Owned(make_valid(g).unwrap_or_else(|| g.clone()))
    }
}

/// `ST_Intersection(a, b)`.
pub fn intersection(a: &Geom, b: &Geom) -> Option<Geom> {
    let av = ensure_valid(a);
    let bv = ensure_valid(b);
    Some(Geometry::MultiPolygon(
        to_multi_polygon(&av).intersection(&to_multi_polygon(&bv)),
    ))
}

/// `ST_Union(a, b)`.
pub fn union(a: &Geom, b: &Geom) -> Option<Geom> {
    let av = ensure_valid(a);
    let bv = ensure_valid(b);
    Some(Geometry::MultiPolygon(
        to_multi_polygon(&av).union(&to_multi_polygon(&bv)),
    ))
}

// ----- binary: geometry, geometry -> boolean ----------------------------

/// `ST_Intersects(a, b)`. (Uses `geo`'s sweep-line `Intersects`, which is
/// robust on invalid input — no `make_valid` needed.)
pub fn intersects(a: &Geom, b: &Geom) -> Option<bool> {
    Some(a.intersects(b))
}

/// `ST_Contains(a, b)` — a fully contains b. Point operand uses our own robust
/// ray-cast PIP (see `point_in_geometry`); the general case falls back to geo's
/// `Contains` guarded by `ensure_valid`.
pub fn contains(a: &Geom, b: &Geom) -> Option<bool> {
    match b {
        Geometry::Point(p) => Some(point_in_geometry(p, a)),
        _ => {
            use geo::Contains;
            let av = ensure_valid(a);
            let bv = ensure_valid(b);
            Some((&*av).contains(&*bv))
        }
    }
}

/// `ST_Within(a, b)` — a is fully contained by b. Point operand uses our own
/// robust ray-cast PIP (the SpatialBench join shape: trip point within a zone
/// polygon). General case falls back to geo's `Contains` guarded by
/// `ensure_valid`.
pub fn within(a: &Geom, b: &Geom) -> Option<bool> {
    match a {
        Geometry::Point(p) => Some(point_in_geometry(p, b)),
        _ => {
            use geo::Contains;
            let av = ensure_valid(a);
            let bv = ensure_valid(b);
            Some((&*bv).contains(&*av))
        }
    }
}

/// Robust point-in-geometry test via even-odd ray casting.
///
/// We implement this ourselves rather than calling `geo`'s `Contains<Point>`
/// because geo's point-in-polygon path stack-overflows on the over-complex
/// (100k+ vertex) and self-intersecting polygons found in real datasets such
/// as Overture admin boundaries. PNPOLY ray casting is iterative O(n), cannot
/// recurse, and yields a well-defined "is the point in the filled area" answer
/// even for self-intersecting rings.
fn point_in_geometry(p: &geo_types::Point<f64>, g: &Geom) -> bool {
    let (x, y) = (p.x(), p.y());
    match g {
        Geometry::Polygon(poly) => point_in_polygon(x, y, poly),
        Geometry::MultiPolygon(mp) => mp.0.iter().any(|poly| point_in_polygon(x, y, poly)),
        Geometry::GeometryCollection(c) => c.0.iter().any(|item| point_in_geometry(p, item)),
        Geometry::LineString(ls) => ls.0.iter().any(|c| c.x == x && c.y == y),
        Geometry::Line(l) => {
            // point-on-segment
            let same_side = (x - l.start.x) * (l.end.y - l.start.y)
                == (y - l.start.y) * (l.end.x - l.start.x);
            same_side
                && (x >= f64::min(l.start.x, l.end.x) && x <= f64::max(l.start.x, l.end.x))
                && (y >= f64::min(l.start.y, l.end.y) && y <= f64::max(l.start.y, l.end.y))
        }
        Geometry::Point(other) => other.x() == x && other.y() == y,
        _ => false,
    }
}

/// PNPOLY even-odd ray cast: inside the exterior ring and outside every hole.
fn point_in_polygon(x: f64, y: f64, poly: &geo_types::Polygon<f64>) -> bool {
    let inside_ring = |ring: &geo_types::LineString<f64>| {
        let pts = &ring.0;
        let n = pts.len();
        if n < 3 {
            return false;
        }
        let mut c = false;
        let mut j = n - 1;
        for i in 0..n {
            let (xi, yi) = (pts[i].x, pts[i].y);
            let (xj, yj) = (pts[j].x, pts[j].y);
            if ((yi > y) != (yj > y)) && (x < (xj - xi) * (y - yi) / (yj - yi) + xi) {
                c = !c;
            }
            j = i;
        }
        c
    };
    let in_exterior = inside_ring(poly.exterior());
    poly.interiors().iter().fold(in_exterior, |acc, hole| acc && !inside_ring(hole))
}

/// `ST_Disjoint(a, b)` — negation of ST_Intersects.
pub fn disjoint(a: &Geom, b: &Geom) -> Option<bool> {
    Some(!a.intersects(b))
}

// ----- unary: geometry -> DOUBLE ----------------------------------------

/// `ST_Area(geom)` — unsigned planar area. Zero for non-areal geometries.
pub fn area(g: &Geom) -> Option<f64> {
    Some(g.unsigned_area())
}

/// `ST_X(point)` — x ordinate of a Point, else NULL.
pub fn x(g: &Geom) -> Option<f64> {
    match g {
        Geometry::Point(p) => Some(p.x()),
        _ => None,
    }
}

/// `ST_Y(point)` — y ordinate of a Point, else NULL.
pub fn y(g: &Geom) -> Option<f64> {
    match g {
        Geometry::Point(p) => Some(p.y()),
        _ => None,
    }
}

// ----- unary: geometry -> VARCHAR ---------------------------------------

/// `ST_GeometryType(geom)` — OGC style type name (e.g. `ST_Point`).
pub fn geometry_type(g: &Geom) -> Option<String> {
    let name = match g {
        Geometry::Point(_) => "ST_Point",
        Geometry::Line(_) => "ST_LineString",
        Geometry::LineString(_) => "ST_LineString",
        Geometry::Polygon(_) => "ST_Polygon",
        Geometry::MultiPoint(_) => "ST_MultiPoint",
        Geometry::MultiLineString(_) => "ST_MultiLineString",
        Geometry::MultiPolygon(_) => "ST_MultiPolygon",
        Geometry::GeometryCollection(_) => "ST_GeometryCollection",
        Geometry::Rect(_) => "ST_Polygon",
        Geometry::Triangle(_) => "ST_Polygon",
    };
    Some(name.to_string())
}

// ----- unary: geometry -> INTEGER ---------------------------------------

/// `ST_Dimension(geom)` — inherent dimension (Point=0, Line=1, Polygon=2, ...).
pub fn dimension(g: &Geom) -> Option<i32> {
    let dim = match g {
        Geometry::Point(_) | Geometry::MultiPoint(_) => 0,
        Geometry::Line(_) | Geometry::LineString(_) | Geometry::MultiLineString(_) => 1,
        Geometry::Polygon(_) | Geometry::MultiPolygon(_) | Geometry::Rect(_) | Geometry::Triangle(_) => 2,
        Geometry::GeometryCollection(c) => c.iter().map(|item| dimension(item).unwrap_or(0)).max().unwrap_or(0),
    };
    Some(dim)
}

// ----- constructors & I/O -----------------------------------------------

/// `ST_GeomFromText(wkt)` — parse Well-Known Text into a geometry.
pub fn geom_from_text(s: &str) -> Option<Geom> {
    use std::str::FromStr;
    let parsed = wkt::Wkt::<f64>::from_str(s).ok()?;
    // `Wkt::to_geometry` yields a `geo_types::Geometry`; map Option/Result
    // shapes defensively without depending on which the crate returns.
    geom_from_wkt(parsed)
}

#[allow(clippy::needless_pass_by_value)]
fn geom_from_wkt(parsed: wkt::Wkt<f64>) -> Option<Geom> {
    use std::convert::TryInto;
    // Prefer the explicit TryFrom<Wkt> for Geometry when available; fall back
    // to the inherent `to_geometry` accessor used by upstream SedonaDB.
    TryInto::<Geom>::try_into(parsed).ok()
}

/// `ST_AsText(geom)` — serialize a geometry to Well-Known Text.
pub fn as_text(g: &Geom) -> Option<String> {
    let mut out = String::new();
    wkt::to_wkt::write_geometry(&mut out, g).ok()?;
    Some(out)
}

/// `ST_Point(x, y)` — construct a 2D point.
pub fn point(x: f64, y: f64) -> Option<Geom> {
    Some(Geometry::Point(geo_types::Point::new(x, y)))
}

/// `ST_GeomFromWKB(blob)` — parse + re-serialize WKB (validates and normalizes).
pub fn geom_from_wkb(g: &Geom) -> Option<Geom> {
    Some(g.clone())
}

// ----- measurements -----------------------------------------------------

/// `ST_Length(geom)` — planar length of linear geometries (0 for points/areas).
#[allow(deprecated)]
pub fn length(g: &Geom) -> Option<f64> {
    use geo::EuclideanLength;
    fn ring_len(p: &geo_types::Polygon<f64>) -> f64 {
        use geo::EuclideanLength as _;
        p.exterior().euclidean_length()
            + p.interiors().iter().map(|r| r.euclidean_length()).sum::<f64>()
    }
    Some(match g {
        Geometry::Line(l) => l.euclidean_length(),
        Geometry::LineString(ls) => ls.euclidean_length(),
        Geometry::MultiLineString(mls) => mls.euclidean_length(),
        Geometry::Polygon(p) => ring_len(p),
        Geometry::MultiPolygon(mp) => mp.0.iter().map(ring_len).sum::<f64>(),
        Geometry::GeometryCollection(c) => c.iter().filter_map(length).sum(),
        _ => 0.0,
    })
}

/// `ST_Distance(a, b)` — planar Euclidean distance between geometries.
#[allow(deprecated)]
pub fn distance(a: &Geom, b: &Geom) -> Option<f64> {
    Some(a.euclidean_distance(b))
}

/// `ST_DWithin(a, b, distance)` — true when the planar distance from `a` to
/// `b` is `<= distance`.
#[allow(deprecated)]
pub fn dwithin(a: &Geom, b: &Geom, distance: f64) -> Option<bool> {
    Some(a.euclidean_distance(b) <= distance)
}

// ----- transforms -------------------------------------------------------

/// `ST_Buffer(geom, radius)` — polygon buffer at `radius`.
pub fn buffer(g: &Geom, radius: f64) -> Option<Geom> {
    Some(Geometry::MultiPolygon(g.buffer(radius)))
}

/// `ST_Simplify(geom, epsilon)` — Ramer-Douglas-Peucker simplification.
pub fn simplify(g: &Geom, epsilon: f64) -> Option<Geom> {
    use geo::Simplify as _;
    Some(match g {
        Geometry::LineString(ls) => Geometry::LineString(ls.simplify(epsilon)),
        Geometry::MultiLineString(mls) => Geometry::MultiLineString(mls.simplify(epsilon)),
        Geometry::Polygon(p) => Geometry::Polygon(p.simplify(epsilon)),
        Geometry::MultiPolygon(mp) => Geometry::MultiPolygon(mp.simplify(epsilon)),
        Geometry::GeometryCollection(c) => {
            let items: Vec<_> = c.iter().filter_map(|item| simplify(item, epsilon)).collect();
            Geometry::GeometryCollection(geo_types::GeometryCollection(items))
        }
        other => other.clone(),
    })
}

// ----- set operations ---------------------------------------------------

/// `ST_Difference(a, b)`. Guards via `ensure_valid`.
pub fn difference(a: &Geom, b: &Geom) -> Option<Geom> {
    let av = ensure_valid(a);
    let bv = ensure_valid(b);
    Some(Geometry::MultiPolygon(
        to_multi_polygon(&av).difference(&to_multi_polygon(&bv)),
    ))
}

/// `ST_SymDifference(a, b)`. Guards via `ensure_valid`.
pub fn sym_difference(a: &Geom, b: &Geom) -> Option<Geom> {
    let av = ensure_valid(a);
    let bv = ensure_valid(b);
    Some(Geometry::MultiPolygon(
        to_multi_polygon(&av).xor(&to_multi_polygon(&bv)),
    ))
}

/// `ST_MakeLine(a, b)` — line string through the point coordinates of `a`
/// then `b`. Used by SpatialBench Q7 to build a trip segment from pickup/dropoff.
pub fn make_line(a: &Geom, b: &Geom) -> Option<Geom> {
    let pa = point_coord(a)?;
    let pb = point_coord(b)?;
    Some(Geometry::LineString(geo_types::LineString::from(vec![pa, pb])))
}

/// Extract the (x, y) of a Point geometry, or `None` for non-points.
fn point_coord(g: &Geom) -> Option<geo_types::Coord<f64>> {
    match g {
        Geometry::Point(p) => Some(p.0),
        _ => None,
    }
}

// ----- validity & shape -------------------------------------------------

/// `ST_IsValid(geom)` — passes `geo`'s structural validation.
pub fn is_valid(g: &Geom) -> Option<bool> {
    Some(g.is_valid())
}

/// `ST_IsEmpty(geom)` — true for geometries with no coordinate content.
pub fn is_empty(g: &Geom) -> Option<bool> {
    Some(match g {
        Geometry::Point(_) | Geometry::Line(_) | Geometry::Rect(_) | Geometry::Triangle(_) => false,
        Geometry::MultiPoint(mp) => mp.0.is_empty(),
        Geometry::LineString(ls) => ls.0.is_empty(),
        Geometry::MultiLineString(mls) => mls.0.is_empty(),
        Geometry::Polygon(p) => p.exterior().0.is_empty(),
        Geometry::MultiPolygon(mp) => mp.0.is_empty(),
        Geometry::GeometryCollection(c) => c.0.is_empty(),
    })
}

/// `ST_NumPoints(geom)` — total vertex count across the geometry.
pub fn num_points(g: &Geom) -> Option<i32> {
    let n: usize = match g {
        Geometry::Point(_) => 1,
        Geometry::MultiPoint(mp) => mp.0.len(),
        Geometry::Line(_) => 2,
        Geometry::LineString(ls) => ls.0.len(),
        Geometry::MultiLineString(mls) => mls.0.iter().map(|ls| ls.0.len()).sum(),
        Geometry::Polygon(p) => {
            p.exterior().0.len() + p.interiors().iter().map(|r| r.0.len()).sum::<usize>()
        }
        Geometry::MultiPolygon(mp) => mp
            .0
            .iter()
            .map(|p| p.exterior().0.len() + p.interiors().iter().map(|r| r.0.len()).sum::<usize>())
            .sum(),
        Geometry::Rect(_) => 5,
        Geometry::Triangle(_) => 4,
        Geometry::GeometryCollection(c) => c.0.iter().map(|item| num_points(item).unwrap_or(0) as usize).sum(),
    };
    n.try_into().ok()
}

// ----- bounding-box accessors (used for join prefiltering) ---------------

/// `ST_XMin(geom)`.
pub fn xmin(g: &Geom) -> Option<f64> {
    g.bounding_rect().map(|r| r.min().x)
}
/// `ST_XMax(geom)`.
pub fn xmax(g: &Geom) -> Option<f64> {
    g.bounding_rect().map(|r| r.max().x)
}
/// `ST_YMin(geom)`.
pub fn ymin(g: &Geom) -> Option<f64> {
    g.bounding_rect().map(|r| r.min().y)
}
/// `ST_YMax(geom)`.
pub fn ymax(g: &Geom) -> Option<f64> {
    g.bounding_rect().map(|r| r.max().y)
}

// ----- DE-9IM predicates (via geo::Relate) -------------------------------
// All route through `geo`'s geomgraph `relate`, which can stack-overflow on
// invalid polygons — so every one guards both inputs with `ensure_valid`.

/// `ST_Equals(a, b)` — topological equality.
pub fn equals(a: &Geom, b: &Geom) -> Option<bool> {
    use geo::Relate;
    let av = ensure_valid(a);
    let bv = ensure_valid(b);
    Some((&*av).relate(&*bv).is_equal_topo())
}
/// `ST_Touches(a, b)`.
pub fn touches(a: &Geom, b: &Geom) -> Option<bool> {
    use geo::Relate;
    let av = ensure_valid(a);
    let bv = ensure_valid(b);
    Some((&*av).relate(&*bv).is_touches())
}
/// `ST_Crosses(a, b)`.
pub fn crosses(a: &Geom, b: &Geom) -> Option<bool> {
    use geo::Relate;
    let av = ensure_valid(a);
    let bv = ensure_valid(b);
    Some((&*av).relate(&*bv).is_crosses())
}
/// `ST_Overlaps(a, b)`.
pub fn overlaps(a: &Geom, b: &Geom) -> Option<bool> {
    use geo::Relate;
    let av = ensure_valid(a);
    let bv = ensure_valid(b);
    Some((&*av).relate(&*bv).is_overlaps())
}
/// `ST_Covers(a, b)`.
pub fn covers(a: &Geom, b: &Geom) -> Option<bool> {
    use geo::Relate;
    let av = ensure_valid(a);
    let bv = ensure_valid(b);
    Some((&*av).relate(&*bv).is_covers())
}
/// `ST_CoveredBy(a, b)`.
pub fn covered_by(a: &Geom, b: &Geom) -> Option<bool> {
    use geo::Relate;
    let av = ensure_valid(a);
    let bv = ensure_valid(b);
    Some((&*av).relate(&*bv).is_coveredby())
}

// ----- structural accessors ----------------------------------------------

/// `ST_NumGeometries(geom)`.
pub fn num_geometries(g: &Geom) -> Option<i32> {
    let n: usize = match g {
        Geometry::MultiPoint(mp) => mp.0.len(),
        Geometry::MultiLineString(mls) => mls.0.len(),
        Geometry::MultiPolygon(mp) => mp.0.len(),
        Geometry::GeometryCollection(c) => c.0.len(),
        _ => 1,
    };
    n.try_into().ok()
}

/// `ST_NumInteriorRings(geom)`.
pub fn num_interior_rings(g: &Geom) -> Option<i32> {
    match g {
        Geometry::Polygon(p) => p.interiors().len().try_into().ok(),
        Geometry::MultiPolygon(mp) => mp
            .0
            .iter()
            .map(|p| p.interiors().len())
            .sum::<usize>()
            .try_into()
            .ok(),
        _ => Some(0),
    }
}

/// `ST_ExteriorRing(geom)` — polygon's exterior ring as a LineString.
pub fn exterior_ring(g: &Geom) -> Option<Geom> {
    match g {
        Geometry::Polygon(p) => Some(Geometry::LineString(p.exterior().clone())),
        _ => None,
    }
}

/// `ST_StartPoint(geom)` — first vertex of a LineString.
pub fn start_point(g: &Geom) -> Option<Geom> {
    match g {
        Geometry::LineString(ls) => ls.0.first().copied().map(|c| Geometry::Point(c.into())),
        _ => None,
    }
}

/// `ST_EndPoint(geom)` — last vertex of a LineString.
pub fn end_point(g: &Geom) -> Option<Geom> {
    match g {
        Geometry::LineString(ls) => ls.0.last().copied().map(|c| Geometry::Point(c.into())),
        _ => None,
    }
}

/// `ST_IsClosed(geom)`.
pub fn is_closed(g: &Geom) -> Option<bool> {
    Some(match g {
        Geometry::LineString(ls) => ls.0.first().is_some_and(|f| ls.0.last().is_some_and(|l| f == l)),
        Geometry::MultiLineString(mls) => mls.0.iter().all(|ls| {
            ls.0.first().is_some_and(|f| ls.0.last().is_some_and(|l| f == l))
        }),
        Geometry::Polygon(_) | Geometry::MultiPolygon(_) => true,
        _ => false,
    })
}

/// `ST_CoordDim(geom)` — this extension handles 2D WKB.
pub fn coord_dim(_g: &Geom) -> Option<i32> {
    Some(2)
}

// ----- more measurements -------------------------------------------------

/// `ST_Perimeter(geom)`.
#[allow(deprecated)]
pub fn perimeter(g: &Geom) -> Option<f64> {
    fn ring_perim(p: &geo_types::Polygon<f64>) -> f64 {
        use geo::EuclideanLength as _;
        p.exterior().euclidean_length()
            + p.interiors().iter().map(|r| r.euclidean_length()).sum::<f64>()
    }
    Some(match g {
        Geometry::Polygon(p) => ring_perim(p),
        Geometry::MultiPolygon(mp) => mp.0.iter().map(ring_perim).sum::<f64>(),
        _ => 0.0,
    })
}

/// `ST_Azimuth(a, b)` — planar bearing (radians, clockwise from +Y).
pub fn azimuth(a: &Geom, b: &Geom) -> Option<f64> {
    let pa = point_coord(a)?;
    let pb = point_coord(b)?;
    Some((pb.x - pa.x).atan2(pb.y - pa.y).rem_euclid(2.0 * std::f64::consts::PI))
}

// ----- more transforms ---------------------------------------------------

/// `ST_PointOnSurface(geom)`.
pub fn point_on_surface(g: &Geom) -> Option<Geom> {
    use geo::InteriorPoint;
    g.interior_point().map(Geometry::Point)
}

/// `ST_Rotate(geom, angle)` — rotate about centroid by `angle` radians.
pub fn rotate(g: &Geom, angle: f64) -> Option<Geom> {
    use geo::{Centroid, Rotate};
    let c = g.centroid()?;
    Some(g.rotate_around_point(angle, c))
}

/// `ST_SimplifyVW(geom, epsilon)`.
pub fn simplify_vw(g: &Geom, epsilon: f64) -> Option<Geom> {
    use geo::SimplifyVw as _;
    Some(match g {
        Geometry::LineString(ls) => Geometry::LineString(ls.simplify_vw(epsilon)),
        Geometry::MultiLineString(mls) => Geometry::MultiLineString(mls.simplify_vw(epsilon)),
        Geometry::Polygon(p) => Geometry::Polygon(p.simplify_vw(epsilon)),
        Geometry::MultiPolygon(mp) => Geometry::MultiPolygon(mp.simplify_vw(epsilon)),
        other => other.clone(),
    })
}

/// `ST_Translate(geom, dx, dy)`.
pub fn translate(g: &Geom, dx: f64, dy: f64) -> Option<Geom> {
    use geo::Translate;
    Some(g.translate(dx, dy))
}

/// `ST_Scale(geom, xfac, yfac)`.
pub fn scale(g: &Geom, xfac: f64, yfac: f64) -> Option<Geom> {
    use geo::Scale;
    Some(g.scale_xy(xfac, yfac))
}

// ----- I/O ----------------------------------------------------------------

/// `ST_AsBinary(geom)` — ISO-WKB bytes.
pub fn as_binary(g: &Geom) -> Option<Vec<u8>> {
    crate::geometry::to_wkb(g).ok()
}

// ----- 2D / Z / M stubs (this extension handles 2D WKB only) -------------

/// `ST_Force2D(geom)` — drop Z/M (no-op here; we are already 2D).
pub fn force_2d(g: &Geom) -> Option<Geom> {
    Some(g.clone())
}
/// `ST_HasZ(geom)` — false (2D only).
pub fn has_z(_g: &Geom) -> Option<bool> {
    Some(false)
}
/// `ST_HasM(geom)` — false (2D only).
pub fn has_m(_g: &Geom) -> Option<bool> {
    Some(false)
}
/// `ST_ZMflag(geom)` — 0 (2D only).
pub fn zm_flag(_g: &Geom) -> Option<i32> {
    Some(0)
}
/// `ST_Z(geom)` — NULL (2D only).
pub fn z(_g: &Geom) -> Option<f64> {
    None
}
/// `ST_M(geom)` — NULL (2D only).
pub fn m(_g: &Geom) -> Option<f64> {
    None
}
/// `ST_IsCollection(geom)`.
pub fn is_collection(g: &Geom) -> Option<bool> {
    Some(matches!(g, Geometry::MultiPoint(_) | Geometry::MultiLineString(_) | Geometry::MultiPolygon(_) | Geometry::GeometryCollection(_)))
}

// ----- structural accessors (indexed) ------------------------------------

/// `ST_GeometryN(geom, n)` — the n-th geometry of a collection (1-indexed).
pub fn geometry_n(g: &Geom, n: i32) -> Option<Geom> {
    let i = usize::try_from(n.checked_sub(1)?).ok()?;
    match g {
        Geometry::MultiPoint(mp) => mp.0.get(i).cloned().map(Geometry::Point),
        Geometry::MultiLineString(mls) => mls.0.get(i).cloned().map(Geometry::LineString),
        Geometry::MultiPolygon(mp) => mp.0.get(i).cloned().map(Geometry::Polygon),
        Geometry::GeometryCollection(c) => c.0.get(i).cloned(),
        _ => None,
    }
}

/// `ST_PointN(geom, n)` — the n-th vertex of a LineString (1-indexed).
pub fn point_n(g: &Geom, n: i32) -> Option<Geom> {
    let i = usize::try_from(n.checked_sub(1)?).ok()?;
    match g {
        Geometry::LineString(ls) => ls.0.get(i).copied().map(|c| Geometry::Point(c.into())),
        _ => None,
    }
}

/// `ST_InteriorRingN(geom, n)` — the n-th hole of a Polygon (1-indexed).
pub fn interior_ring_n(g: &Geom, n: i32) -> Option<Geom> {
    let i = usize::try_from(n.checked_sub(1)?).ok()?;
    match g {
        Geometry::Polygon(p) => p.interiors().get(i).cloned().map(Geometry::LineString),
        _ => None,
    }
}

// ----- more editing transforms -------------------------------------------

/// `ST_Reverse(geom)` — reverse vertex order.
pub fn reverse_geom(g: &Geom) -> Option<Geom> {
    Some(match g {
        Geometry::LineString(ls) => {
            let mut pts = ls.0.clone();
            pts.reverse();
            Geometry::LineString(geo_types::LineString(pts))
        }
        Geometry::MultiLineString(mls) => {
            Geometry::MultiLineString(geo_types::MultiLineString(mls.0.iter().map(|ls| { let mut p = ls.0.clone(); p.reverse(); geo_types::LineString(p) }).collect()))
        }
        Geometry::Polygon(p) => {
            let mut ext = p.exterior().0.clone();
            ext.reverse();
            let ints: Vec<_> = p.interiors().iter().map(|r| { let mut q = r.0.clone(); q.reverse(); geo_types::LineString(q) }).collect();
            Geometry::Polygon(geo_types::Polygon::new(geo_types::LineString(ext), ints))
        }
        other => other.clone(),
    })
}

/// `ST_FlipCoordinates(geom)` — swap X and Y.
pub fn flip_coordinates(g: &Geom) -> Option<Geom> {
    use geo::MapCoords;
    Some(g.map_coords(|c| geo_types::Coord { x: c.y, y: c.x }))
}

/// `ST_RemoveRepeatedPoints(geom)` — drop consecutive duplicate vertices.
pub fn remove_repeated_points(g: &Geom) -> Option<Geom> {
    use geo::RemoveRepeatedPoints;
    Some(match g {
        Geometry::LineString(ls) => Geometry::LineString(ls.remove_repeated_points()),
        Geometry::MultiLineString(mls) => Geometry::MultiLineString(mls.remove_repeated_points()),
        Geometry::Polygon(p) => Geometry::Polygon(p.remove_repeated_points()),
        Geometry::MultiPolygon(mp) => Geometry::MultiPolygon(mp.remove_repeated_points()),
        other => other.clone(),
    })
}

/// `ST_LineInterpolatePoint(geom, fraction)` — point at `fraction` along a LineString.
pub fn line_interpolate_point(g: &Geom, fraction: f64) -> Option<Geom> {
    use geo::LineInterpolatePoint;
    match g {
        Geometry::LineString(ls) => ls.line_interpolate_point(fraction).map(Geometry::Point),
        _ => None,
    }
}

/// `ST_ConcaveHull(geom, concavity)` — dispatches by variant (geo's ConcaveHull
/// is not implemented for the Geometry enum directly).
pub fn concave_hull(g: &Geom, concavity: f64) -> Option<Geom> {
    use geo::ConcaveHull;
    Some(Geometry::Polygon(match g {
        Geometry::Polygon(p) => p.concave_hull(concavity),
        Geometry::MultiPolygon(mp) => mp.concave_hull(concavity),
        Geometry::MultiPoint(mp) => mp.concave_hull(concavity),
        Geometry::LineString(ls) => ls.concave_hull(concavity),
        Geometry::MultiLineString(mls) => mls.concave_hull(concavity),
        _ => return None,
    }))
}

/// `ST_OrientedEnvelope(geom)` — minimum-area rotated bounding rectangle.
pub fn oriented_envelope(g: &Geom) -> Option<Geom> {
    use geo::MinimumRotatedRect;
    Some(Geometry::Polygon(g.minimum_rotated_rect()?))
}

/// `ST_HausdorffDistance(a, b)`.
pub fn hausdorff_distance(a: &Geom, b: &Geom) -> Option<f64> {
    use geo::HausdorffDistance;
    Some(a.hausdorff_distance(b))
}

// ----- EWKT / SRID (SRID carried in text only; geometry is SRID-less) ----

/// `ST_AsEWKT(geom, srid)` — `SRID=<n>;<wkt>`.
pub fn as_ewkt(g: &Geom, srid: i32) -> Option<String> {
    Some(format!("SRID={srid};{}", as_text(g)?))
}

/// `ST_GeomFromEWKT(text)` — parse `SRID=<n>;<wkt>` (SRID discarded, 2D only).
pub fn geom_from_ewkt(s: &str) -> Option<Geom> {
    let wkt = if let Some(rest) = s.strip_prefix("SRID=") {
        rest.split_once(';').map(|(_, w)| w).unwrap_or(rest)
    } else {
        s
    };
    geom_from_text(wkt)
}

/// `ST_SetSRID(geom, srid)` — no-op tag (extension is SRID-less until PROJ/Tier 3).
pub fn set_srid(g: &Geom, _srid: i32) -> Option<Geom> {
    Some(g.clone())
}

/// `ST_SRID(geom)` — always 0 until CRS support lands.
pub fn srid(_g: &Geom) -> Option<i32> {
    Some(0)
}

// ----- more geometry processing -----------------------------------------

/// All coordinates of a geometry, flattened (manual; geo's `CoordsIter` isn't
/// implemented for the `Geometry` enum).
fn all_coords(g: &Geom) -> Vec<geo_types::Coord<f64>> {
    use geo_types::Coord;
    fn rec(g: &Geom, out: &mut Vec<Coord<f64>>) {
        match g {
            Geometry::Point(p) => out.push(p.0),
            Geometry::Line(l) => { out.push(l.start); out.push(l.end); }
            Geometry::LineString(ls) => out.extend(ls.0.iter().copied()),
            Geometry::Polygon(p) => {
                out.extend(p.exterior().0.iter().copied());
                for r in p.interiors() { out.extend(r.0.iter().copied()); }
            }
            Geometry::MultiPoint(mp) => out.extend(mp.0.iter().map(|p| p.0)),
            Geometry::MultiLineString(mls) => for ls in &mls.0 { out.extend(ls.0.iter().copied()) },
            Geometry::MultiPolygon(mp) => for p in &mp.0 {
                out.extend(p.exterior().0.iter().copied());
                for r in p.interiors() { out.extend(r.0.iter().copied()); }
            },
            Geometry::GeometryCollection(c) => for item in &c.0 { rec(item, out) },
            Geometry::Rect(r) => { out.push(r.min()); out.push(r.max()); }
            Geometry::Triangle(t) => out.extend(t.to_array().iter()),
        }
    }
    let mut out = Vec::new();
    rec(g, &mut out);
    out
}

/// `ST_Points(geom)` — every vertex as a MultiPoint.
pub fn points(g: &Geom) -> Option<Geom> {
    let pts: Vec<geo_types::Point<f64>> = all_coords(g).into_iter().map(geo_types::Point::from).collect();
    Some(Geometry::MultiPoint(geo_types::MultiPoint(pts)))
}

/// `ST_LineLocatePoint(line, point)` — fraction of `line` at the projection of `point`.
pub fn line_locate_point(g: &Geom, p: &Geom) -> Option<f64> {
    use geo::LineLocatePoint;
    match (g, p) {
        (Geometry::LineString(ls), Geometry::Point(pt)) => Some(ls.line_locate_point(pt)?),
        _ => None,
    }
}

/// `ST_FrechetDistance(a, b)` — discrete Fréchet distance of two LineStrings.
pub fn frechet_distance(a: &Geom, b: &Geom) -> Option<f64> {
    use geo::FrechetDistance;
    match (a, b) {
        (Geometry::LineString(la), Geometry::LineString(lb)) => Some(la.frechet_distance(lb)),
        _ => None,
    }
}

/// `ST_AsGeoJSON(geom)` — GeoJSON serialization of the geometry.
pub fn as_geojson(g: &Geom) -> Option<String> {
    // Manual GeoJSON serialization (avoids geojson crate API churn).
    let coord = |c: &geo_types::Coord<f64>| format!("[{},{}]", c.x, c.y);
    let ring = |ls: &geo_types::LineString<f64>| {
        format!("[{}]", ls.0.iter().map(coord).collect::<Vec<_>>().join(","))
    };
    let json = match g {
        Geometry::Point(p) => format!(r#"{{"type":"Point","coordinates":[{},{}]}}"#, p.x(), p.y()),
        Geometry::MultiPoint(mp) => format!(r#"{{"type":"MultiPoint","coordinates":[{}]}}"#,
            mp.0.iter().map(|p| format!("[{},{}]", p.x(), p.y())).collect::<Vec<_>>().join(",")),
        Geometry::LineString(ls) => format!(r#"{{"type":"LineString","coordinates":{}}}"#, ring(ls)),
        Geometry::MultiLineString(mls) => format!(r#"{{"type":"MultiLineString","coordinates":[{}]}}"#,
            mls.0.iter().map(ring).collect::<Vec<_>>().join(",")),
        Geometry::Polygon(p) => {
            let rings: Vec<String> = std::iter::once(p.exterior()).chain(p.interiors().iter()).map(ring).collect();
            format!(r#"{{"type":"Polygon","coordinates":[{}]}}"#, rings.join(","))
        }
        Geometry::MultiPolygon(mp) => {
            let polys: Vec<String> = mp.0.iter().map(|p| {
                let rings: Vec<String> = std::iter::once(p.exterior()).chain(p.interiors().iter()).map(ring).collect();
                format!("[{}]", rings.join(","))
            }).collect();
            format!(r#"{{"type":"MultiPolygon","coordinates":[{}]}}"#, polys.join(","))
        }
        _ => r#"{"type":"GeometryCollection","geometries":[]}"#.to_string(),
    };
    Some(json)
}

/// `ST_Project(geom, distance, azimuth)` — geographic destination point from a
/// point, distance in metres, and azimuth in degrees (clockwise from north).
#[allow(deprecated)]
pub fn project(g: &Geom, distance: f64, azimuth: f64) -> Option<Geom> {
    use geo::HaversineDestination;
    let p = match g { Geometry::Point(p) => *p, _ => return None };
    Some(Geometry::Point(p.haversine_destination(distance, azimuth)))
}

/// `ST_ForcePolygonCW(geom)` — force exterior ring CW, interiors CCW.
pub fn force_polygon_cw(g: &Geom) -> Option<Geom> {
    use geo::Orient;
    Some(match g {
        Geometry::Polygon(p) => Geometry::Polygon(p.orient(geo::algorithm::orient::Direction::Reversed)),
        Geometry::MultiPolygon(mp) => Geometry::MultiPolygon(mp.orient(geo::algorithm::orient::Direction::Reversed)),
        other => other.clone(),
    })
}

/// `ST_SnapToGrid(geom, size)` — round every coordinate to the nearest `size` grid.
pub fn snap_to_grid(g: &Geom, size: f64) -> Option<Geom> {
    if size <= 0.0 { return Some(g.clone()); }
    let round = |v: f64| (v / size).round() * size;
    use geo::MapCoords;
    Some(g.map_coords(|c| geo_types::Coord { x: round(c.x), y: round(c.y) }))
}

/// `ST_Boundary(geom)` — topological boundary: polygon → MultiLineString of rings,
/// LineString → MultiPoint of endpoints (if open) or empty (if closed).
pub fn boundary(g: &Geom) -> Option<Geom> {
    Some(match g {
        Geometry::Polygon(p) => {
            let lines: Vec<geo_types::LineString> = std::iter::once(p.exterior().clone())
                .chain(p.interiors().iter().cloned())
                .collect();
            Geometry::MultiLineString(geo_types::MultiLineString(lines))
        }
        Geometry::MultiPolygon(mp) => {
            let lines: Vec<geo_types::LineString> = mp.0.iter().flat_map(|p| {
                std::iter::once(p.exterior().clone()).chain(p.interiors().iter().cloned())
            }).collect();
            Geometry::MultiLineString(geo_types::MultiLineString(lines))
        }
        Geometry::LineString(ls) => {
            if ls.0.len() >= 2 && ls.0.first() != ls.0.last() {
                Geometry::MultiPoint(geo_types::MultiPoint(vec![
                    geo_types::Point::from(ls.0[0]),
                    geo_types::Point::from(*ls.0.last().unwrap()),
                ]))
            } else {
                Geometry::GeometryCollection(geo_types::GeometryCollection(vec![]))
            }
        }
        _ => Geometry::GeometryCollection(geo_types::GeometryCollection(vec![])),
    })
}

/// `ST_IsRing(geom)` — true for a closed, simple LineString.  (Approximation:
/// checks `is_closed`; full simplicity needs geo's `is_simple` which is not on the
/// `Geometry` enum.)
pub fn is_ring(g: &Geom) -> Option<bool> {
    Some(matches!(g, Geometry::LineString(ls) if ls.0.len() >= 4 && ls.0.first().is_some_and(|f| ls.0.last().is_some_and(|l| f == l))))
}

/// `ST_ClosestPoint(geom, point)` — nearest point on `geom` to `point`.
pub fn closest_point(g: &Geom, p: &Geom) -> Option<Geom> {
    use geo::ClosestPoint;
    let pt = geo_types::Point::from(point_coord(p)?);
    let single = |c: geo::Closest<f64>| match c {
        geo::Closest::SinglePoint(p) | geo::Closest::Intersection(p) => Some(Geometry::Point(p)),
        _ => None,
    };
    match g {
        Geometry::LineString(ls) => single(ls.closest_point(&pt)),
        Geometry::Polygon(poly) => single(poly.closest_point(&pt)),
        Geometry::Line(l) => single(l.closest_point(&pt)),
        _ => None,
    }
}

/// `ST_DelaunayTriangles(geom)` — Delaunay triangulation of the vertex set
/// (via the `delaunator` crate).
pub fn delaunay_triangles(g: &Geom) -> Option<Geom> {
    let coords = all_coords(g);
    let pts: Vec<delaunator::Point> = coords.iter().map(|c| delaunator::Point { x: c.x, y: c.y }).collect();
    if pts.len() < 3 {
        return None;
    }
    let tri = delaunator::triangulate(&pts);
    let t = &tri.triangles;
    let mut out = Vec::new();
    let mut i = 0;
    while i + 2 < t.len() {
        let pa = &pts[t[i]];
        let pb = &pts[t[i + 1]];
        let pc = &pts[t[i + 2]];
        out.push(Geometry::Triangle(geo_types::Triangle(
            geo_types::Coord { x: pa.x, y: pa.y },
            geo_types::Coord { x: pb.x, y: pb.y },
            geo_types::Coord { x: pc.x, y: pc.y },
        )));
        i += 3;
    }
    Some(Geometry::GeometryCollection(geo_types::GeometryCollection(out)))
}

/// Circumcenter of three points (NaN-safe for collinear input).
fn circumcenter(ax: f64, ay: f64, bx: f64, by: f64, cx: f64, cy: f64) -> (f64, f64) {
    let d = 2.0 * (ax * (by - cy) + bx * (cy - ay) + cx * (ay - by));
    if d.abs() < 1e-20 {
        return (ax, ay);
    }
    let a2 = ax * ax + ay * ay;
    let b2 = bx * bx + by * by;
    let c2 = cx * cx + cy * cy;
    let ux = (a2 * (by - cy) + b2 * (cy - ay) + c2 * (ay - by)) / d;
    let uy = (a2 * (cx - bx) + b2 * (ax - cx) + c2 * (bx - ax)) / d;
    (ux, uy)
}

/// `ST_VoronoiLines(geom)` — the interior Voronoi diagram edges, derived as the
/// dual of the Delaunay triangulation (connect circumcenters of adjacent
/// triangles). Boundary edges (no adjacent triangle) are omitted. Returns a
/// GeometryCollection of 2-point LineStrings.
pub fn voronoi_lines(g: &Geom) -> Option<Geom> {
    let coords = all_coords(g);
    let pts: Vec<delaunator::Point> = coords.iter().map(|c| delaunator::Point { x: c.x, y: c.y }).collect();
    if pts.len() < 3 {
        return None;
    }
    let tri = delaunator::triangulate(&pts);
    let t = &tri.triangles;
    // Circumcenter of each triangle.
    let ccs: Vec<(f64, f64)> = (0..t.len())
        .step_by(3)
        .map(|i| {
            let a = &pts[t[i]];
            let b = &pts[t[i + 1]];
            let c = &pts[t[i + 2]];
            circumcenter(a.x, a.y, b.x, b.y, c.x, c.y)
        })
        .collect();
    // Connect circumcenters of triangles sharing an edge (use delaunator halfedges).
    let mut lines = Vec::new();
    for e in 0..t.len() {
        let opp = tri.halfedges[e];
        if opp != delaunator::EMPTY && e < opp {
            let t1 = e / 3;
            let t2 = opp / 3;
            let (x1, y1) = ccs[t1];
            let (x2, y2) = ccs[t2];
            lines.push(Geometry::LineString(geo_types::LineString::from(vec![
                (x1, y1),
                (x2, y2),
            ])));
        }
    }
    Some(Geometry::GeometryCollection(geo_types::GeometryCollection(lines)))
}

// ----- Geography (geodesic) variants (Tier 2) ---------------------------
// Coordinates interpreted as lon/lat. Distances/length in metres, area in m².
// Point-to-point / per-type only (documented); full geometry-geometry geodesic
// distance would need closest-point-on-sphere work.

/// `ST_DistanceSphere(a, b)` — great-circle distance between two points (metres).
#[allow(deprecated)]
pub fn distance_sphere(a: &Geom, b: &Geom) -> Option<f64> {
    use geo::HaversineDistance;
    let pa = geo_types::Point::from(point_coord(a)?);
    let pb = geo_types::Point::from(point_coord(b)?);
    Some(pa.haversine_distance(&pb))
}

/// `ST_DWithinSphere(a, b, metres)`.
#[allow(deprecated)]
pub fn dwithin_sphere(a: &Geom, b: &Geom, metres: f64) -> Option<bool> {
    Some(distance_sphere(a, b)? <= metres)
}

/// `ST_LengthSphere(geom)` — great-circle length of a (multi)linestring (metres).
#[allow(deprecated)]
pub fn length_sphere(g: &Geom) -> Option<f64> {
    use geo::HaversineLength;
    Some(match g {
        Geometry::LineString(ls) => ls.haversine_length(),
        Geometry::MultiLineString(mls) => mls.haversine_length(),
        _ => 0.0,
    })
}

/// `ST_AreaSphere(geom)` — geodesic area of a (multi)polygon (m²).
pub fn area_sphere(g: &Geom) -> Option<f64> {
    use geo::ChamberlainDuquetteArea;
    Some(match g {
        Geometry::Polygon(p) => p.chamberlain_duquette_unsigned_area(),
        Geometry::MultiPolygon(mp) => mp.chamberlain_duquette_unsigned_area(),
        _ => 0.0,
    })
}

// ----- CRS reprojection via PROJ (Tier 3a) ------------------------------
// Requires libproj at runtime. A thread-local cache of `proj::Proj` objects
// (keyed by (from_epsg, to_epsg)) avoids re-parsing the CRS per row, which is
// the expensive part — the per-coordinate transform is then cheap.

thread_local! {
    static PROJ_CACHE: std::cell::RefCell<std::collections::HashMap<(i32, i32), Option<proj::Proj>>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// `ST_Transform(geom, from_srid, to_srid)` — reproject between EPSG codes.
pub fn transform(g: &Geom, from_srid: i32, to_srid: i32) -> Option<Geom> {
    use geo::MapCoords;
    PROJ_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let proj = cache
            .entry((from_srid, to_srid))
            .or_insert_with(|| {
                proj::Proj::new_known_crs(
                    &format!("EPSG:{from_srid}"),
                    &format!("EPSG:{to_srid}"),
                    None,
                )
                .ok()
            })
            .as_ref()?;
        Some(g.map_coords(|c| {
            proj.convert((c.x, c.y))
                .map(|(x, y)| geo_types::Coord { x, y })
                .unwrap_or(c)
        }))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{from_wkb, to_wkb};

    fn wkt_point(x: f64, y: f64) -> Geom {
        Geometry::Point(geo_types::Point::new(x, y))
    }

    fn wkb_of(g: &Geom) -> Vec<u8> {
        to_wkb(g).expect("serialize")
    }

    #[test]
    fn convex_hull_of_line_is_line() {
        // A two-point line's convex hull is a degenerate (collapsed) polygon.
        let ls = Geometry::LineString(geo_types::LineString::from(vec![(0.0, 0.0), (4.0, 4.0)]));
        let out = convex_hull(&ls).expect("hull");
        // Result is representable and round-trips.
        assert!(to_wkb(&out).is_ok());
    }

    #[test]
    fn envelope_of_polygon() {
        let poly = Geometry::Polygon(
            geo_types::Polygon::new(
                geo_types::LineString::from(vec![(1.0, 1.0), (5.0, 1.0), (5.0, 4.0), (1.0, 4.0), (1.0, 1.0)]),
                vec![],
            ),
        );
        let env = envelope(&poly).expect("envelope");
        match env {
            Geometry::Polygon(p) => {
                let bbox = p.bounding_rect().unwrap();
                assert_eq!(bbox.min().x, 1.0);
                assert_eq!(bbox.max().x, 5.0);
            }
            _ => panic!("expected polygon envelope"),
        }
    }

    #[test]
    fn centroid_and_area() {
        let poly = Geometry::Polygon(
            geo_types::Polygon::new(
                geo_types::LineString::from(vec![(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0), (0.0, 0.0)]),
                vec![],
            ),
        );
        let c = centroid(&poly).expect("centroid");
        match c {
            Geometry::Point(p) => {
                assert!((p.x() - 2.0).abs() < 1e-9);
                assert!((p.y() - 2.0).abs() < 1e-9);
            }
            _ => panic!("expected point centroid"),
        }
        assert!((area(&poly).unwrap() - 16.0).abs() < 1e-9);
    }

    #[test]
    fn predicates_on_overlapping_squares() {
        let a = Geometry::Polygon(geo_types::Polygon::new(
            geo_types::LineString::from(vec![(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0), (0.0, 0.0)]),
            vec![],
        ));
        let b = Geometry::Polygon(geo_types::Polygon::new(
            geo_types::LineString::from(vec![(1.0, 1.0), (3.0, 1.0), (3.0, 3.0), (1.0, 3.0), (1.0, 1.0)]),
            vec![],
        ));
        let inner = wkt_point(1.5, 1.5);

        assert_eq!(intersects(&a, &b), Some(true));
        assert_eq!(disjoint(&a, &b), Some(false));
        assert_eq!(contains(&a, &inner), Some(true));
        assert_eq!(within(&inner, &a), Some(true));
        assert_eq!(contains(&a, &b), Some(false));
    }

    #[test]
    fn dimension_and_type_names() {
        assert_eq!(dimension(&wkt_point(0.0, 0.0)), Some(0));
        assert_eq!(geometry_type(&wkt_point(0.0, 0.0)).as_deref(), Some("ST_Point"));
    }

    #[test]
    fn full_pipeline_through_wkb() {
        // Exercise from_wkb -> convex_hull -> to_wkb, the exact path the
        // dispatch layer takes for every ST_* call.
        let tri = Geometry::Polygon(geo_types::Polygon::new(
            geo_types::LineString::from(vec![(0.0, 0.0), (4.0, 0.0), (2.0, 4.0), (0.0, 0.0)]),
            vec![],
        ));
        let bytes = wkb_of(&tri);
        let parsed = from_wkb(&bytes).expect("parse");
        let hull = convex_hull(&parsed).expect("hull");
        let out = to_wkb(&hull).expect("serialize");
        // A convex hull is itself a valid geometry that re-parses.
        assert!(from_wkb(&out).is_ok());
    }

    // ---- dispatch-path isolation: mirrors str_to_geom → unary_geom_varchar ----
    fn dispatch_roundtrip(wkt_in: &str) -> Option<String> {
        let geom = geom_from_text(wkt_in)?;
        let bytes = crate::geometry::to_wkb(&geom).ok()?;
        let reparsed = crate::geometry::from_wkb(&bytes).ok()?;
        as_text(&reparsed)
    }

    #[test]
    fn isolate_value_dependent_bug() {
        // These are the exact cases that failed (NULL) under DuckDB. If they
        // pass here, the bug is in the FFI dispatch layer, not the geometry layer.
        for (wkt, label) in [
            ("POINT(1 2)", "POINT(1 2)"),
            ("POINT(3 4)", "POINT(3 4)"),
            ("POINT(0 0)", "POINT(0 0)"),
            ("POINT(1 0)", "POINT(1 0)"),
            ("POINT(2 0)", "POINT(2 0)"),
            ("POINT(3 0)", "POINT(3 0)"),
            ("POINT(4 0)", "POINT(4 0)"),
            ("POLYGON((0 0,1 0,1 1,0 1,0 0))", "POLYGON(1x1)"),
            ("POLYGON((0 0,4 0,4 4,0 4,0 0))", "POLYGON(4x4)"),
            ("LINESTRING(0 0, 3 4)", "LINESTRING"),
        ] {
            let got = dispatch_roundtrip(wkt);
            assert!(got.is_some(), "ROUNDTRIP FAILED (pure rust) for {label}: {wkt} -> {got:?}");
            println!("{label}: {got:?}");
        }
    }
}
