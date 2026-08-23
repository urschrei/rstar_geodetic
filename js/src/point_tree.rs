//! The `PointTree` class.
//!
//! Each leaf is an [`IndexedPoint`], a point carrying its input position, so a hit reports
//! its position directly; the linestring and polygon trees do the same with
//! [`rstar_geodetic::indexed::IndexedLeaf`] (see [`crate::indexed_query`]).

use rstar::PointDistance;
use rstar_geodetic::indexed::{
    IndexedPoint, indexed_points, nearest_position, nearest_position_wgs84,
};
use rstar_geodetic::{
    GeodeticCoord, GeodeticPoint, GeodeticRTree, PointLeaf, UnitVec, geodesic_distance_wgs84,
    squared_chord_to_metres,
};
use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::wasm_bindgen;

use crate::error::{geodetic_error, geodetic_error_message};
use crate::result::{Neighbor, Neighbors, neighbor, neighbors};

/// A geodetic R-tree over points.
#[wasm_bindgen]
pub struct PointTree {
    tree: GeodeticRTree<IndexedPoint>,
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
        let len = points.len();
        Ok(Self {
            tree: GeodeticRTree::bulk_load(indexed_points(points)),
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
        Ok(nearest_position(&self.tree, query).map(|(index, _)| index as u32))
    }

    /// The nearest point to `(lon, lat)` by great-circle distance as a
    /// `{ index, distance }` object, or `undefined` if the tree is empty.
    #[wasm_bindgen(js_name = nearestWithDistance)]
    pub fn nearest_with_distance(&self, lon: f64, lat: f64) -> Result<Option<Neighbor>, JsValue> {
        let query = query_coord(lon, lat)?;
        Ok(nearest_position(&self.tree, query).map(|(index, metres)| neighbor(index, metres)))
    }

    /// The `k` nearest points to `(lon, lat)` by great-circle distance, nearest first,
    /// as a `{ indices, distances }` object. Fewer than `k` are returned if the tree
    /// holds fewer points.
    #[wasm_bindgen(js_name = nearestN)]
    pub fn nearest_n(&self, lon: f64, lat: f64, k: u32) -> Result<Neighbors, JsValue> {
        let query = query_coord(lon, lat)?;
        let k = k as usize;
        let mut indices: Vec<u32> = Vec::with_capacity(k);
        let mut distances: Vec<f64> = Vec::with_capacity(k);
        for (leaf, metres) in self.tree.nearest_neighbor_iter_with_distance(query).take(k) {
            indices.push(leaf.data as u32);
            distances.push(metres);
        }
        Ok(neighbors(&indices, &distances))
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
        Ok(collect_neighbors(hits, |leaf| {
            squared_chord_to_metres(leaf.distance_2(&query_vector))
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
        Ok(self
            .tree
            .locate_in_rectangle(lower, upper)
            .map(|leaf| leaf.data as u32)
            .collect())
    }

    /// The input position of the nearest point to `(lon, lat)` by WGS84-ellipsoid
    /// geodesic distance, or `undefined` if the tree is empty.
    #[wasm_bindgen(js_name = nearestWgs84)]
    pub fn nearest_wgs84(&self, lon: f64, lat: f64) -> Result<Option<u32>, JsValue> {
        let query = query_coord(lon, lat)?;
        Ok(nearest_position_wgs84(&self.tree, query).map(|(index, _)| index as u32))
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
        Ok(
            nearest_position_wgs84(&self.tree, query)
                .map(|(index, metres)| neighbor(index, metres)),
        )
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
        Ok(collect_neighbors(hits, |leaf| {
            geodesic_distance_wgs84(query, leaf.coord())
        }))
    }
}

/// Collects hits into a `{ indices, distances }` object, computing each distance once.
fn collect_neighbors<'a>(
    hits: impl Iterator<Item = &'a IndexedPoint>,
    distance: impl Fn(&IndexedPoint) -> f64,
) -> Neighbors {
    let mut indices: Vec<u32> = Vec::new();
    let mut distances: Vec<f64> = Vec::new();
    for leaf in hits {
        indices.push(leaf.data as u32);
        distances.push(distance(leaf));
    }
    neighbors(&indices, &distances)
}

/// A range-validated query coordinate.
pub fn query_coord(lon: f64, lat: f64) -> Result<GeodeticCoord, JsValue> {
    GeodeticPoint::try_new(lon, lat)
        .map(|point| point.coord())
        .map_err(geodetic_error)
}
