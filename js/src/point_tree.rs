//! The `PointTree` class.
//!
//! Keeps the concrete `GeodeticRTree<GeodeticPoint>` so it retains the crate's indexed
//! rectangle query and tuned wgs84 refine, recovering input positions from a
//! coordinate-keyed map (see [`rstar_geodetic::indexed`]).

use std::collections::HashSet;

use rstar::PointDistance;
use rstar_geodetic::indexed::{CoordIndex, coord_index, coord_key};
use rstar_geodetic::{
    GeodeticCoord, GeodeticPoint, GeodeticRTree, UnitVec, geodesic_distance_wgs84,
    squared_chord_to_metres,
};
use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::wasm_bindgen;

use crate::error::{geodetic_error, geodetic_error_message};
use crate::result::{Neighbor, Neighbors, neighbor, neighbors};

/// A geodetic R-tree over points.
#[wasm_bindgen]
pub struct PointTree {
    tree: GeodeticRTree<GeodeticPoint>,
    index_by_coord: CoordIndex,
    len: usize,
}

#[wasm_bindgen]
impl PointTree {
    /// Builds a tree from interleaved `[lon0, lat0, lon1, lat1, ...]` coordinates in
    /// degrees. Throws a `GeodeticError` if the array has odd length or a coordinate is
    /// out of range or not finite.
    #[wasm_bindgen(constructor)]
    pub fn new(coords: &[f64]) -> Result<PointTree, JsValue> {
        if coords.len() % 2 != 0 {
            return Err(geodetic_error_message(
                "coordinate array must have even length (interleaved lon, lat pairs)",
            ));
        }
        let points = coords
            .chunks_exact(2)
            .map(|pair| GeodeticPoint::try_new(pair[0], pair[1]))
            .collect::<Result<Vec<_>, _>>()
            .map_err(geodetic_error)?;
        let index_by_coord = coord_index(&points);
        let len = points.len();
        Ok(Self {
            tree: GeodeticRTree::bulk_load(points),
            index_by_coord,
            len,
        })
    }

    /// The number of points in the tree.
    #[wasm_bindgen(getter)]
    pub fn length(&self) -> u32 {
        self.len as u32
    }

    /// The input position of the nearest point to `(lon, lat)` by great-circle distance,
    /// or `undefined` if the tree is empty.
    pub fn nearest(&self, lon: f64, lat: f64) -> Result<Option<u32>, JsValue> {
        let query = query_coord(lon, lat)?;
        Ok(self
            .tree
            .nearest_neighbor_with_distance(query)
            .map(|(point, _)| self.position(point.coord())))
    }

    /// The nearest point to `(lon, lat)` by great-circle distance as a
    /// `{ index, distance }` object, or `undefined` if the tree is empty.
    #[wasm_bindgen(js_name = nearestWithDistance)]
    pub fn nearest_with_distance(&self, lon: f64, lat: f64) -> Result<Option<Neighbor>, JsValue> {
        let query = query_coord(lon, lat)?;
        Ok(self
            .tree
            .nearest_neighbor_with_distance(query)
            .map(|(point, metres)| neighbor(self.position(point.coord()) as usize, metres)))
    }

    /// Every point within `radiusMetres` great-circle metres of `(lon, lat)`, as a
    /// `{ indices, distances }` object.
    #[wasm_bindgen(js_name = withinDistance)]
    pub fn within_distance(
        &self,
        lon: f64,
        lat: f64,
        radius_metres: f64,
    ) -> Result<Neighbors, JsValue> {
        let query = query_coord(lon, lat)?;
        let query_vector = UnitVec::from(query);
        let hits = self.tree.locate_within_distance(query, radius_metres);
        Ok(self.collect_neighbors(hits, |point| {
            squared_chord_to_metres(point.distance_2(&query_vector))
        }))
    }

    /// The input positions of every point inside the longitude/latitude rectangle from
    /// `(minLon, minLat)` to `(maxLon, maxLat)`. `minLon > maxLon` denotes a window
    /// crossing the antimeridian (RFC 7946); `minLat <= maxLat` is required.
    #[wasm_bindgen(js_name = inRectangle)]
    pub fn in_rectangle(
        &self,
        min_lon: f64,
        min_lat: f64,
        max_lon: f64,
        max_lat: f64,
    ) -> Result<Vec<u32>, JsValue> {
        let lower = query_coord(min_lon, min_lat)?;
        let upper = query_coord(max_lon, max_lat)?;
        if lower.lat > upper.lat {
            return Err(geodetic_error_message(
                "inRectangle requires minLat <= maxLat",
            ));
        }
        let mut seen: HashSet<(u64, u64)> = HashSet::new();
        let mut indices: Vec<u32> = Vec::new();
        for point in self.tree.locate_in_rectangle(lower, upper) {
            let key = coord_key(point.coord());
            if seen.insert(key) {
                indices.extend(self.index_by_coord[&key].iter().map(|&i| i as u32));
            }
        }
        Ok(indices)
    }

    /// The input position of the nearest point to `(lon, lat)` by WGS84-ellipsoid
    /// geodesic distance, or `undefined` if the tree is empty.
    #[wasm_bindgen(js_name = nearestWgs84)]
    pub fn nearest_wgs84(&self, lon: f64, lat: f64) -> Result<Option<u32>, JsValue> {
        let query = query_coord(lon, lat)?;
        Ok(self
            .tree
            .nearest_neighbor_with_distance_wgs84(query)
            .map(|(point, _)| self.position(point.coord())))
    }

    /// The nearest point to `(lon, lat)` on the WGS84 ellipsoid as a `{ index, distance }`
    /// object, or `undefined` if the tree is empty.
    #[wasm_bindgen(js_name = nearestWithDistanceWgs84)]
    pub fn nearest_with_distance_wgs84(
        &self,
        lon: f64,
        lat: f64,
    ) -> Result<Option<Neighbor>, JsValue> {
        let query = query_coord(lon, lat)?;
        Ok(self
            .tree
            .nearest_neighbor_with_distance_wgs84(query)
            .map(|(point, metres)| neighbor(self.position(point.coord()) as usize, metres)))
    }

    /// Every point within `radiusMetres` WGS84-ellipsoid geodesic metres of `(lon, lat)`,
    /// as a `{ indices, distances }` object.
    #[wasm_bindgen(js_name = withinDistanceWgs84)]
    pub fn within_distance_wgs84(
        &self,
        lon: f64,
        lat: f64,
        radius_metres: f64,
    ) -> Result<Neighbors, JsValue> {
        let query = query_coord(lon, lat)?;
        let hits = self.tree.locate_within_distance_wgs84(query, radius_metres);
        Ok(self.collect_neighbors(hits, |point| geodesic_distance_wgs84(query, point.coord())))
    }
}

impl PointTree {
    /// The first input position recorded at `coord`.
    fn position(&self, coord: GeodeticCoord) -> u32 {
        self.index_by_coord[&coord_key(coord)][0] as u32
    }

    /// Expands each distinct hit coordinate to every input position recorded there,
    /// computing the distance once per coordinate.
    fn collect_neighbors<'a>(
        &self,
        hits: impl Iterator<Item = &'a GeodeticPoint>,
        distance: impl Fn(&GeodeticPoint) -> f64,
    ) -> Neighbors {
        let mut seen: HashSet<(u64, u64)> = HashSet::new();
        let mut indices: Vec<u32> = Vec::new();
        let mut distances: Vec<f64> = Vec::new();
        for point in hits {
            let key = coord_key(point.coord());
            if seen.insert(key) {
                let metres = distance(point);
                for &index in &self.index_by_coord[&key] {
                    indices.push(index as u32);
                    distances.push(metres);
                }
            }
        }
        neighbors(&indices, &distances)
    }
}

/// A range-validated query coordinate.
fn query_coord(lon: f64, lat: f64) -> Result<GeodeticCoord, JsValue> {
    GeodeticPoint::try_new(lon, lat)
        .map(|point| point.coord())
        .map_err(geodetic_error)
}
