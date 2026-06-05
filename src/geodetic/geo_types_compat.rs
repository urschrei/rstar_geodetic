//! Conversions between this crate's geodetic geometries and [`geo-types`](geo_types).
//!
//! Enabled by the default `geo-types` feature. Coordinate order matches on both sides
//! (`x = lon`, `y = lat`, the OGC convention), so each conversion is a direct field map
//! with no swapping.
//!
//! Conversions *into* the validated geometries ([`GeodeticPoint`],
//! [`GeodeticLineString`], [`GeodeticPolygon`]) are fallible ([`TryFrom`], yielding
//! [`GeodeticError`]): they range-check every coordinate and enforce the structural
//! preconditions (a linestring needs at least two vertices and each edge `< 180`
//! degrees; a polygon ring must be closed, with at least three distinct vertices).
//! Conversions *out* to `geo-types` are infallible ([`From`]). [`GeodeticCoord`] carries
//! no invariants, so its conversions are infallible in both directions.
//!
//! `geo-types` geometries are generic over the coordinate scalar; these conversions
//! cover the `f64` instantiation, the scalar the geodetic types use.

use alloc::vec::Vec;

use geo_types::{Coord, LineString, MultiLineString, MultiPoint, MultiPolygon, Point, Polygon};

use super::coord::{GeodeticCoord, GeodeticError};
use super::linestring::GeodeticLineString;
use super::point::GeodeticPoint;
use super::polygon::{GeodeticPolygon, GeodeticRing};
use super::tree::GeodeticRTree;

/// Builds a `geo-types` [`LineString`] from the degree coordinates of a ring or polyline.
fn to_linestring(coords: &[GeodeticCoord]) -> LineString<f64> {
    LineString::new(coords.iter().copied().map(Coord::from).collect())
}

// --- GeodeticCoord <-> Coord / Point (infallible; GeodeticCoord has no invariants) ---

/// Maps `(x, y)` to `(lon, lat)`. Infallible and unvalidated, matching the existing
/// `From<(f64, f64)>` for [`GeodeticCoord`]; range-check with [`GeodeticCoord::try_new`]
/// or one of the fallible geometry constructors if the source is untrusted.
impl From<Coord<f64>> for GeodeticCoord {
    fn from(c: Coord<f64>) -> Self {
        Self { lon: c.x, lat: c.y }
    }
}

impl From<Point<f64>> for GeodeticCoord {
    fn from(p: Point<f64>) -> Self {
        Self {
            lon: p.x(),
            lat: p.y(),
        }
    }
}

impl From<GeodeticCoord> for Coord<f64> {
    fn from(c: GeodeticCoord) -> Self {
        Coord { x: c.lon, y: c.lat }
    }
}

impl From<GeodeticCoord> for Point<f64> {
    fn from(c: GeodeticCoord) -> Self {
        Point::new(c.lon, c.lat)
    }
}

// --- GeodeticPoint (validated inbound, infallible outbound) ---

impl TryFrom<Point<f64>> for GeodeticPoint {
    type Error = GeodeticError;
    fn try_from(p: Point<f64>) -> Result<Self, Self::Error> {
        GeodeticPoint::try_new(p.x(), p.y())
    }
}

impl TryFrom<Coord<f64>> for GeodeticPoint {
    type Error = GeodeticError;
    fn try_from(c: Coord<f64>) -> Result<Self, Self::Error> {
        GeodeticPoint::try_new(c.x, c.y)
    }
}

impl From<GeodeticPoint> for Point<f64> {
    fn from(p: GeodeticPoint) -> Self {
        p.coord().into()
    }
}

impl From<GeodeticPoint> for Coord<f64> {
    fn from(p: GeodeticPoint) -> Self {
        p.coord().into()
    }
}

// --- GeodeticLineString ---

impl TryFrom<LineString<f64>> for GeodeticLineString {
    type Error = GeodeticError;
    fn try_from(ls: LineString<f64>) -> Result<Self, Self::Error> {
        GeodeticLineString::try_from_lonlat(ls.0)
    }
}

impl From<GeodeticLineString> for LineString<f64> {
    fn from(ls: GeodeticLineString) -> Self {
        to_linestring(ls.coords())
    }
}

// --- GeodeticPolygon ---

/// `geo-types` polygons store their rings closed (first vertex == last), which is exactly
/// what [`GeodeticRing::try_from_lonlat`] requires; ring orientation is taken as given.
impl TryFrom<Polygon<f64>> for GeodeticPolygon {
    type Error = GeodeticError;
    fn try_from(poly: Polygon<f64>) -> Result<Self, Self::Error> {
        let (exterior, interiors) = poly.into_inner();
        let exterior = GeodeticRing::try_from_lonlat(exterior.0)?;
        let interiors = interiors
            .into_iter()
            .map(|ring| GeodeticRing::try_from_lonlat(ring.0))
            .collect::<Result<Vec<_>, _>>()?;
        GeodeticPolygon::try_new(exterior, interiors)
    }
}

impl From<GeodeticPolygon> for Polygon<f64> {
    fn from(poly: GeodeticPolygon) -> Self {
        let exterior = to_linestring(poly.exterior().coords());
        let interiors = poly
            .interiors()
            .iter()
            .map(|ring| to_linestring(ring.coords()))
            .collect();
        Polygon::new(exterior, interiors)
    }
}

// --- geo-types multi-geometries -> a bulk-loaded GeodeticRTree ---
//
// The orphan rule forbids implementing a foreign trait for `Vec<GeodeticPolygon>` (the
// `Vec` is foreign), so the collection conversions target the local `GeodeticRTree<_>`
// instead, building the index directly. For a plain `Vec`, or to go the other way, map
// the element conversions over the geometry's iterator (see the crate documentation).
// Each is fallible for the same reason the element conversions are: every coordinate is
// range-checked and the structural preconditions enforced.

impl TryFrom<MultiPoint<f64>> for GeodeticRTree<GeodeticPoint> {
    type Error = GeodeticError;
    fn try_from(points: MultiPoint<f64>) -> Result<Self, Self::Error> {
        let leaves = points
            .0
            .into_iter()
            .map(GeodeticPoint::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(GeodeticRTree::bulk_load(leaves))
    }
}

impl TryFrom<MultiLineString<f64>> for GeodeticRTree<GeodeticLineString> {
    type Error = GeodeticError;
    fn try_from(lines: MultiLineString<f64>) -> Result<Self, Self::Error> {
        let leaves = lines
            .0
            .into_iter()
            .map(GeodeticLineString::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(GeodeticRTree::bulk_load(leaves))
    }
}

impl TryFrom<MultiPolygon<f64>> for GeodeticRTree<GeodeticPolygon> {
    type Error = GeodeticError;
    fn try_from(polygons: MultiPolygon<f64>) -> Result<Self, Self::Error> {
        let leaves = polygons
            .0
            .into_iter()
            .map(GeodeticPolygon::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(GeodeticRTree::bulk_load(leaves))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coord_order_is_preserved_both_ways() {
        let g: GeodeticCoord = Coord { x: 12.0, y: -7.0 }.into();
        assert_eq!((g.lon, g.lat), (12.0, -7.0));
        let c: Coord<f64> = g.into();
        assert_eq!((c.x, c.y), (12.0, -7.0));
    }

    #[test]
    fn point_round_trips_through_geo_types() {
        let p = GeodeticPoint::new(2.5, 48.8);
        let geo: Point<f64> = p.into();
        assert_eq!((geo.x(), geo.y()), (2.5, 48.8));
        let back = GeodeticPoint::try_from(geo).unwrap();
        assert_eq!(back.coord(), p.coord());
    }

    #[test]
    fn point_try_from_range_checks() {
        // Latitude out of range (a swapped lat/lon with |lon| > 90).
        let bad = Point::new(10.0, 200.0);
        assert_eq!(
            GeodeticPoint::try_from(bad),
            Err(GeodeticError::LatOutOfRange(200.0))
        );
    }

    #[test]
    fn linestring_round_trips() {
        let geo = LineString::new(vec![
            Coord { x: 0.0, y: 0.0 },
            Coord { x: 1.0, y: 1.0 },
            Coord { x: 2.0, y: 0.0 },
        ]);
        let ls = GeodeticLineString::try_from(geo.clone()).unwrap();
        assert_eq!(ls.coords().len(), 3);
        let back: LineString<f64> = ls.into();
        assert_eq!(back, geo);
    }

    #[test]
    fn linestring_try_from_rejects_single_vertex() {
        let geo = LineString::new(vec![Coord { x: 0.0, y: 0.0 }]);
        assert_eq!(
            GeodeticLineString::try_from(geo),
            Err(GeodeticError::TooFewPoints {
                found: 1,
                needed: 2,
            })
        );
    }

    #[test]
    fn polygon_round_trips_with_closed_ring() {
        // geo-types closes the ring on construction; a square, CCW as seen from outside.
        let geo = Polygon::new(
            LineString::new(vec![
                Coord { x: 0.0, y: 0.0 },
                Coord { x: 10.0, y: 0.0 },
                Coord { x: 10.0, y: 10.0 },
                Coord { x: 0.0, y: 10.0 },
            ]),
            Vec::new(),
        );
        let poly = GeodeticPolygon::try_from(geo).unwrap();
        // A query strictly inside is at distance zero (membership survived the round trip).
        let back: Polygon<f64> = poly.clone().into();
        assert_eq!(back.exterior().0.first(), back.exterior().0.last());
        assert!(poly.interiors().is_empty());
    }

    #[test]
    fn multi_point_builds_a_tree() {
        let mp = MultiPoint(vec![Point::new(0.0, 0.0), Point::new(10.0, 10.0)]);
        let tree = GeodeticRTree::try_from(mp).unwrap();
        assert_eq!(tree.size(), 2);
    }

    #[test]
    fn multi_line_string_builds_a_tree() {
        let mls = MultiLineString(vec![
            LineString::new(vec![Coord { x: 0.0, y: 0.0 }, Coord { x: 1.0, y: 1.0 }]),
            LineString::new(vec![Coord { x: 2.0, y: 2.0 }, Coord { x: 3.0, y: 3.0 }]),
        ]);
        let tree = GeodeticRTree::try_from(mls).unwrap();
        assert_eq!(tree.size(), 2);
    }

    #[test]
    fn multi_polygon_builds_a_tree() {
        let square = Polygon::new(
            LineString::new(vec![
                Coord { x: 0.0, y: 0.0 },
                Coord { x: 10.0, y: 0.0 },
                Coord { x: 10.0, y: 10.0 },
                Coord { x: 0.0, y: 10.0 },
            ]),
            Vec::new(),
        );
        let tree = GeodeticRTree::try_from(MultiPolygon(vec![square])).unwrap();
        assert_eq!(tree.size(), 1);
    }

    #[test]
    fn multi_point_propagates_element_validation_error() {
        // The second point has a latitude out of range; the whole conversion fails.
        let mp = MultiPoint(vec![Point::new(0.0, 0.0), Point::new(0.0, 200.0)]);
        let result = GeodeticRTree::try_from(mp);
        assert_eq!(result.err(), Some(GeodeticError::LatOutOfRange(200.0)));
    }
}
