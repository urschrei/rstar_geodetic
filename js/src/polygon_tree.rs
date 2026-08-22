//! The `PolygonTree` class.

use rstar_geodetic::indexed::IndexedLeaf;
use rstar_geodetic::{GeodeticPolygon, GeodeticRTree, GeodeticRing};
use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::wasm_bindgen;

use crate::csr::{validate_offsets, vertex_count, vertex_pairs};
use crate::error::{geodetic_error, geodetic_error_message};
use crate::indexed_query::{indexed_nearest, indexed_nearest_with_distance, indexed_within};
use crate::point_tree::query_coord;
use crate::result::{Neighbor, Neighbors};

/// A geodetic R-tree over polygons.
#[wasm_bindgen]
pub struct PolygonTree {
    tree: GeodeticRTree<IndexedLeaf<Box<GeodeticPolygon>>>,
    len: usize,
}

#[wasm_bindgen]
impl PolygonTree {
    /// Builds a tree from interleaved `[lon0, lat0, lon1, lat1, ...]` vertex coordinates
    /// in degrees and a two-level CSR layout: `ringOffsets` partitions the vertices into
    /// rings and `polygonOffsets` partitions the rings into polygons, each with one entry
    /// per part plus a final end marker. Within a polygon the first ring is the exterior
    /// and the rest are holes (the GeoArrow layout). Throws a `GeodeticError` on an
    /// invalid offset array, a polygon with no rings, an out-of-range or non-finite
    /// coordinate, or a ring that is not closed or has too few vertices.
    #[wasm_bindgen(constructor)]
    pub fn new(
        coords: &[f64],
        ring_offsets: &[u32],
        polygon_offsets: &[u32],
    ) -> Result<PolygonTree, JsValue> {
        let num_vertices = vertex_count(coords)?;
        let num_rings = validate_offsets(ring_offsets, num_vertices, "ringOffsets")?;
        let num_polygons = validate_offsets(polygon_offsets, num_rings, "polygonOffsets")?;
        let mut leaves = Vec::with_capacity(num_polygons);
        for index in 0..num_polygons {
            let first_ring = polygon_offsets[index] as usize;
            let last_ring = polygon_offsets[index + 1] as usize;
            if first_ring == last_ring {
                return Err(geodetic_error_message(&format!(
                    "polygon {index} has no rings; a polygon needs at least an exterior ring"
                )));
            }
            let mut rings = Vec::with_capacity(last_ring - first_ring);
            for ring in first_ring..last_ring {
                let pairs = vertex_pairs(coords, ring_offsets[ring], ring_offsets[ring + 1]);
                rings.push(GeodeticRing::try_from_lonlat(pairs).map_err(geodetic_error)?);
            }
            let mut rings = rings.into_iter();
            let exterior = rings.next().expect("a polygon has at least one ring");
            let interiors = rings.collect();
            let geometry = GeodeticPolygon::try_new(exterior, interiors).map_err(geodetic_error)?;
            leaves.push(IndexedLeaf {
                geometry: Box::new(geometry),
                index,
            });
        }
        Ok(Self {
            tree: GeodeticRTree::bulk_load(leaves),
            len: num_polygons,
        })
    }

    /// The number of polygons in the tree.
    #[wasm_bindgen(getter)]
    pub fn length(&self) -> u32 {
        self.len as u32
    }

    /// The input position of the polygon nearest to `(lon, lat)` by great-circle
    /// distance (zero if the point is inside one), or `undefined` if the tree is empty.
    pub fn nearest(&self, lon: f64, lat: f64) -> Result<Option<u32>, JsValue> {
        Ok(indexed_nearest(&self.tree, query_coord(lon, lat)?))
    }

    /// The polygon nearest to `(lon, lat)` as a `{ index, distance }` object (distance
    /// zero if the point is inside it), or `undefined` if the tree is empty.
    #[wasm_bindgen(js_name = nearestWithDistance)]
    pub fn nearest_with_distance(&self, lon: f64, lat: f64) -> Result<Option<Neighbor>, JsValue> {
        Ok(indexed_nearest_with_distance(
            &self.tree,
            query_coord(lon, lat)?,
        ))
    }

    /// Every polygon within `radiusMetres` great-circle metres of `(lon, lat)` (including
    /// any containing it), as a `{ indices, distances }` object.
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
