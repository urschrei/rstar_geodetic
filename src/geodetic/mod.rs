//! Internal module assembling the geodetic index. The public API is re-exported at
//! the crate root; see the crate-level documentation for the overview.

mod arc;
mod coord;
mod distance;
mod embedding;
mod linestring;
mod point;
mod polygon;
#[cfg(feature = "wgs84")]
mod spheroid;
mod tree;

#[cfg(feature = "geo-types")]
mod geo_types_compat;

pub use arc::{arc_bounding_box, arc_contains_point, arc_distance_2, nearest_point_on_arc};
pub use coord::{GeodeticCoord, GeodeticError};
pub use distance::{
    EARTH_RADIUS_METRES, haversine_distance, metres_to_squared_chord, squared_chord_to_metres,
};
pub use embedding::{UnitVec, squared_chord};
pub use linestring::GeodeticLineString;
pub use point::GeodeticPoint;
pub use polygon::{GeodeticPolygon, GeodeticRing};
#[cfg(feature = "wgs84")]
pub use spheroid::{Ellipsoid, geodesic_distance, geodesic_distance_wgs84};
pub use tree::{GeodeticObject, GeodeticRTree, envelope_distance_metres};
