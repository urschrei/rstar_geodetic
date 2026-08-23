//! The `LineStringTree` class.

use rstar_geodetic::indexed::IndexedLeaf;
use rstar_geodetic::{GeodeticLineString, GeodeticRTree};
use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::wasm_bindgen;

use crate::csr::{validate_offsets, vertex_count, vertex_pairs};
use crate::error::geodetic_error;
use crate::indexed_query::{
    indexed_nearest, indexed_nearest_n, indexed_nearest_with_distance, indexed_within,
};
use crate::point_tree::query_coord;
use crate::result::{Neighbor, Neighbors};

/// A geodetic R-tree over linestrings.
#[wasm_bindgen]
pub struct LineStringTree {
    tree: GeodeticRTree<IndexedLeaf<Box<GeodeticLineString>>>,
    len: usize,
}

#[wasm_bindgen]
impl LineStringTree {
    /// Builds a tree from interleaved `[lon0, lat0, lon1, lat1, ...]` vertex coordinates
    /// in degrees, partitioned into linestrings by `offsets`: a CSR array with one entry
    /// per linestring plus a final end marker, so linestring `i` spans vertices
    /// `offsets[i]..offsets[i + 1]` (the GeoArrow layout). Throws a `GeodeticError` on an
    /// invalid offset array, an out-of-range or non-finite coordinate, or a linestring
    /// with fewer than two vertices.
    #[wasm_bindgen(constructor)]
    pub fn new(coords: &[f64], offsets: &[u32]) -> Result<LineStringTree, JsValue> {
        let num_vertices = vertex_count(coords)?;
        let num_lines = validate_offsets(offsets, num_vertices, "offsets")?;
        let mut leaves = Vec::with_capacity(num_lines);
        for index in 0..num_lines {
            let pairs = vertex_pairs(coords, offsets[index], offsets[index + 1]);
            let geometry = GeodeticLineString::try_from_lonlat(pairs).map_err(geodetic_error)?;
            leaves.push(IndexedLeaf {
                geometry: Box::new(geometry),
                index,
            });
        }
        Ok(Self {
            tree: GeodeticRTree::bulk_load(leaves),
            len: num_lines,
        })
    }

    /// The number of linestrings in the tree.
    #[wasm_bindgen(getter)]
    pub fn length(&self) -> u32 {
        self.len as u32
    }

    /// The input position of the linestring nearest to `(lon, lat)` by great-circle
    /// distance, or `undefined` if the tree is empty.
    pub fn nearest(&self, lon: f64, lat: f64) -> Result<Option<u32>, JsValue> {
        Ok(indexed_nearest(&self.tree, query_coord(lon, lat)?))
    }

    /// The linestring nearest to `(lon, lat)` as a `{ index, distance }` object, or
    /// `undefined` if the tree is empty.
    #[wasm_bindgen(js_name = nearestWithDistance)]
    pub fn nearest_with_distance(&self, lon: f64, lat: f64) -> Result<Option<Neighbor>, JsValue> {
        Ok(indexed_nearest_with_distance(
            &self.tree,
            query_coord(lon, lat)?,
        ))
    }

    /// The `k` nearest linestrings to `(lon, lat)` by great-circle distance, nearest first,
    /// as a `{ indices, distances }` object. Fewer than `k` are returned if the tree
    /// holds fewer linestrings.
    #[wasm_bindgen(js_name = nearestN)]
    pub fn nearest_n(&self, lon: f64, lat: f64, k: u32) -> Result<Neighbors, JsValue> {
        Ok(indexed_nearest_n(
            &self.tree,
            query_coord(lon, lat)?,
            k as usize,
        ))
    }

    /// Every linestring within `radiusMetres` great-circle metres of `(lon, lat)`, as a
    /// `{ indices, distances }` object.
    #[wasm_bindgen(js_name = withinDistance)]
    pub fn within_distance(
        &self,
        lon: f64,
        lat: f64,
        radius_metres: f64,
    ) -> Result<Neighbors, JsValue> {
        Ok(indexed_within(
            &self.tree,
            query_coord(lon, lat)?,
            radius_metres,
        ))
    }
}
