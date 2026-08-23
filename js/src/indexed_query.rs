//! Nearest and within-distance queries shared by the linestring and polygon trees, whose
//! leaves carry their input position.

use rstar::PointDistance;
use rstar_geodetic::indexed::IndexedLeaf;
use rstar_geodetic::{
    GeodeticCoord, GeodeticObject, GeodeticRTree, UnitVec, squared_chord_to_metres,
};

use crate::result::{Neighbor, Neighbors, neighbor, neighbors};

/// The nearest leaf's input position, if any.
pub fn indexed_nearest<G>(tree: &GeodeticRTree<IndexedLeaf<G>>, query: GeodeticCoord) -> Option<u32>
where
    IndexedLeaf<G>: GeodeticObject,
{
    tree.nearest_neighbor_with_distance(query)
        .map(|(leaf, _)| leaf.index as u32)
}

/// The nearest leaf's input position and great-circle distance, if any.
pub fn indexed_nearest_with_distance<G>(
    tree: &GeodeticRTree<IndexedLeaf<G>>,
    query: GeodeticCoord,
) -> Option<Neighbor>
where
    IndexedLeaf<G>: GeodeticObject,
{
    tree.nearest_neighbor_with_distance(query)
        .map(|(leaf, metres)| neighbor(leaf.index, metres))
}

/// The `k` nearest leaves' input positions and great-circle distances, nearest first.
pub fn indexed_nearest_n<G>(
    tree: &GeodeticRTree<IndexedLeaf<G>>,
    query: GeodeticCoord,
    k: usize,
) -> Neighbors
where
    IndexedLeaf<G>: GeodeticObject,
{
    let mut indices: Vec<u32> = Vec::with_capacity(k);
    let mut distances: Vec<f64> = Vec::with_capacity(k);
    for (leaf, metres) in tree.nearest_neighbor_iter_with_distance(query).take(k) {
        indices.push(leaf.index as u32);
        distances.push(metres);
    }
    neighbors(&indices, &distances)
}

/// Every leaf within `radius_metres`, each with its own great-circle distance.
pub fn indexed_within<G>(
    tree: &GeodeticRTree<IndexedLeaf<G>>,
    query: GeodeticCoord,
    radius_metres: f64,
) -> Neighbors
where
    IndexedLeaf<G>: GeodeticObject,
{
    let query_vector = UnitVec::from(query);
    let mut indices: Vec<u32> = Vec::new();
    let mut distances: Vec<f64> = Vec::new();
    for leaf in tree.locate_within_distance(query, radius_metres) {
        indices.push(leaf.index as u32);
        distances.push(squared_chord_to_metres(leaf.distance_2(&query_vector)));
    }
    neighbors(&indices, &distances)
}
