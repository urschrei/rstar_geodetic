//! The three tree classes: `GeodeticPointTree`, `GeodeticLineStringTree`, and
//! `GeodeticPolygonTree`.
//!
//! Queries return integer input positions (the shapely `STRtree` convention). The point
//! tree keeps the concrete `GeodeticRTree<GeodeticPoint>` so it retains the crate's indexed
//! rectangle query and tuned wgs84 refine, recovering positions from a coordinate-keyed
//! map; the linestring and polygon trees wrap each geometry in an [`IndexedLeaf`] that
//! carries its position, sharing one `Arc` with the by-index geometry store.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use pyo3::exceptions::PyIndexError;
use pyo3::prelude::*;
use pyo3::types::PyList;

use rstar::{AABB, PointDistance, RTreeObject};
use rstar_geodetic::{
    GeodeticCoord, GeodeticLineString, GeodeticPoint, GeodeticPolygon, GeodeticRTree, UnitVec,
    squared_chord_to_metres,
};

use crate::geo_interface::{collect_linestrings, collect_points, collect_polygons, parse_point};
use crate::geometry::{LineString, Point, Polygon};

// --- index leaf shared with the by-index store ---

/// A tree leaf carrying its input position and an `Arc` to the geometry, delegating the two
/// index traits to it. The `Arc` is shared with the tree's by-index geometry store.
struct IndexedLeaf<G> {
    geometry: Arc<G>,
    index: usize,
}

impl<G> RTreeObject for IndexedLeaf<G>
where
    G: RTreeObject<Envelope = AABB<UnitVec>>,
{
    type Envelope = AABB<UnitVec>;

    fn envelope(&self) -> Self::Envelope {
        self.geometry.envelope()
    }
}

impl<G> PointDistance for IndexedLeaf<G>
where
    G: RTreeObject<Envelope = AABB<UnitVec>> + PointDistance,
{
    fn distance_2(&self, point: &UnitVec) -> f64 {
        self.geometry.distance_2(point)
    }

    fn contains_point(&self, point: &UnitVec) -> bool {
        self.geometry.contains_point(point)
    }

    fn distance_2_if_less_or_equal(&self, point: &UnitVec, max_distance_2: f64) -> Option<f64> {
        self.geometry
            .distance_2_if_less_or_equal(point, max_distance_2)
    }
}

// --- shared helpers ---

type CoordIndex = HashMap<(u64, u64), Vec<usize>>;

fn coord_key(coord: GeodeticCoord) -> (u64, u64) {
    (coord.lon.to_bits(), coord.lat.to_bits())
}

/// The query coordinate parsed from a point-like object (a `(lon, lat)` pair, a geo
/// interface `Point`, or a mapping), range-validated.
fn query_coord(obj: &Bound<'_, PyAny>) -> PyResult<GeodeticCoord> {
    Ok(parse_point(obj)?.coord())
}

/// Turns `(index, distance)` results into a Python list of either indices or
/// `(index, distance)` pairs.
fn within_result<'py>(
    py: Python<'py>,
    pairs: Vec<(usize, f64)>,
    return_distance: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let list = if return_distance {
        PyList::new(py, pairs)?
    } else {
        PyList::new(py, pairs.into_iter().map(|(index, _)| index))?
    };
    Ok(list.into_any())
}

// --- point tree ---

/// A geodetic R-tree over points.
#[pyclass(module = "rstar_geodetic")]
pub struct GeodeticPointTree {
    tree: GeodeticRTree<GeodeticPoint>,
    geometries: Vec<Arc<GeodeticPoint>>,
    index_by_coord: CoordIndex,
}

impl GeodeticPointTree {
    fn position(&self, coord: GeodeticCoord) -> usize {
        self.index_by_coord[&coord_key(coord)][0]
    }
}

#[pymethods]
impl GeodeticPointTree {
    /// Builds a tree from points: a geopandas `GeoSeries`, a shapely `MultiPoint`, a GeoJSON
    /// collection, or any iterable of point-likes (geo-interface objects, `Point` mappings,
    /// or `(lon, lat)` pairs).
    #[new]
    fn new(py: Python<'_>, data: &Bound<'_, PyAny>) -> PyResult<Self> {
        let points = collect_points(data)?;
        let geometries: Vec<Arc<GeodeticPoint>> = points.iter().map(|p| Arc::new(*p)).collect();
        let mut index_by_coord = CoordIndex::new();
        for (index, point) in points.iter().enumerate() {
            index_by_coord
                .entry(coord_key(point.coord()))
                .or_default()
                .push(index);
        }
        let tree = py.detach(|| GeodeticRTree::bulk_load(points));
        Ok(Self {
            tree,
            geometries,
            index_by_coord,
        })
    }

    fn __len__(&self) -> usize {
        self.geometries.len()
    }

    /// The point at input position `index`.
    fn geometry(&self, index: usize) -> PyResult<Point> {
        self.geometries
            .get(index)
            .map(|arc| Point { inner: arc.clone() })
            .ok_or_else(|| PyIndexError::new_err("geometry index out of range"))
    }

    fn __getitem__(&self, index: usize) -> PyResult<Point> {
        self.geometry(index)
    }

    /// Every point, in input order.
    fn geometries(&self) -> Vec<Point> {
        self.geometries
            .iter()
            .map(|arc| Point { inner: arc.clone() })
            .collect()
    }

    /// The input position of the nearest point to `query`, or `None` if the tree is empty.
    fn nearest(&self, query: &Bound<'_, PyAny>) -> PyResult<Option<usize>> {
        let coord = query_coord(query)?;
        Ok(self
            .tree
            .nearest_neighbor_with_distance(coord)
            .map(|(point, _)| self.position(point.coord())))
    }

    /// The `(input position, distance in metres)` of the nearest point to `query`, or
    /// `None` if the tree is empty.
    fn nearest_with_distance(&self, query: &Bound<'_, PyAny>) -> PyResult<Option<(usize, f64)>> {
        let coord = query_coord(query)?;
        Ok(self
            .tree
            .nearest_neighbor_with_distance(coord)
            .map(|(point, metres)| (self.position(point.coord()), metres)))
    }

    /// The input positions of every point within `radius_metres` great-circle metres of
    /// `query`. With `return_distance=True`, `(index, distance)` pairs instead.
    #[pyo3(signature = (query, radius_metres, return_distance=false))]
    fn within_distance<'py>(
        &self,
        py: Python<'py>,
        query: &Bound<'_, PyAny>,
        radius_metres: f64,
        return_distance: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        let coord = query_coord(query)?;
        let query_vector = UnitVec::from(coord);
        let mut seen: HashSet<(u64, u64)> = HashSet::new();
        let mut pairs: Vec<(usize, f64)> = Vec::new();
        for point in self.tree.locate_within_distance(coord, radius_metres) {
            let key = coord_key(point.coord());
            if seen.insert(key) {
                let metres = squared_chord_to_metres(point.distance_2(&query_vector));
                for &index in &self.index_by_coord[&key] {
                    pairs.push((index, metres));
                }
            }
        }
        within_result(py, pairs, return_distance)
    }

    /// The input positions of every point inside the longitude/latitude rectangle whose
    /// corners are `lower` and `upper`. `lower.lon > upper.lon` denotes a window crossing
    /// the antimeridian (RFC 7946); `lower.lat <= upper.lat` is required.
    fn in_rectangle(
        &self,
        lower: &Bound<'_, PyAny>,
        upper: &Bound<'_, PyAny>,
    ) -> PyResult<Vec<usize>> {
        let lower = query_coord(lower)?;
        let upper = query_coord(upper)?;
        let mut seen: HashSet<(u64, u64)> = HashSet::new();
        let mut indices: Vec<usize> = Vec::new();
        for point in self.tree.locate_in_rectangle(lower, upper) {
            let key = coord_key(point.coord());
            if seen.insert(key) {
                indices.extend_from_slice(&self.index_by_coord[&key]);
            }
        }
        Ok(indices)
    }
}

// --- linestring tree ---

/// A geodetic R-tree over linestrings.
#[pyclass(module = "rstar_geodetic")]
pub struct GeodeticLineStringTree {
    tree: GeodeticRTree<IndexedLeaf<GeodeticLineString>>,
    geometries: Vec<Arc<GeodeticLineString>>,
}

#[pymethods]
impl GeodeticLineStringTree {
    /// Builds a tree from linestrings: a geopandas `GeoSeries`, a shapely
    /// `MultiLineString`, a GeoJSON collection, or any iterable of linestring-likes.
    #[new]
    fn new(py: Python<'_>, data: &Bound<'_, PyAny>) -> PyResult<Self> {
        let geometries: Vec<Arc<GeodeticLineString>> = collect_linestrings(data)?
            .into_iter()
            .map(Arc::new)
            .collect();
        let leaves: Vec<IndexedLeaf<GeodeticLineString>> = geometries
            .iter()
            .enumerate()
            .map(|(index, geometry)| IndexedLeaf {
                geometry: geometry.clone(),
                index,
            })
            .collect();
        let tree = py.detach(|| GeodeticRTree::bulk_load(leaves));
        Ok(Self { tree, geometries })
    }

    fn __len__(&self) -> usize {
        self.geometries.len()
    }

    /// The linestring at input position `index`.
    fn geometry(&self, index: usize) -> PyResult<LineString> {
        self.geometries
            .get(index)
            .map(|arc| LineString { inner: arc.clone() })
            .ok_or_else(|| PyIndexError::new_err("geometry index out of range"))
    }

    fn __getitem__(&self, index: usize) -> PyResult<LineString> {
        self.geometry(index)
    }

    /// Every linestring, in input order.
    fn geometries(&self) -> Vec<LineString> {
        self.geometries
            .iter()
            .map(|arc| LineString { inner: arc.clone() })
            .collect()
    }

    /// The input position of the nearest linestring to `query`, or `None` if empty.
    fn nearest(&self, query: &Bound<'_, PyAny>) -> PyResult<Option<usize>> {
        let coord = query_coord(query)?;
        Ok(self
            .tree
            .nearest_neighbor_with_distance(coord)
            .map(|(leaf, _)| leaf.index))
    }

    /// The `(input position, distance in metres)` of the nearest linestring, or `None`.
    fn nearest_with_distance(&self, query: &Bound<'_, PyAny>) -> PyResult<Option<(usize, f64)>> {
        let coord = query_coord(query)?;
        Ok(self
            .tree
            .nearest_neighbor_with_distance(coord)
            .map(|(leaf, metres)| (leaf.index, metres)))
    }

    /// The input positions of every linestring within `radius_metres` of `query`. With
    /// `return_distance=True`, `(index, distance)` pairs instead.
    #[pyo3(signature = (query, radius_metres, return_distance=false))]
    fn within_distance<'py>(
        &self,
        py: Python<'py>,
        query: &Bound<'_, PyAny>,
        radius_metres: f64,
        return_distance: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        let coord = query_coord(query)?;
        let query_vector = UnitVec::from(coord);
        let pairs: Vec<(usize, f64)> = self
            .tree
            .locate_within_distance(coord, radius_metres)
            .map(|leaf| {
                (
                    leaf.index,
                    squared_chord_to_metres(leaf.distance_2(&query_vector)),
                )
            })
            .collect();
        within_result(py, pairs, return_distance)
    }
}

// --- polygon tree ---

/// A geodetic R-tree over polygons.
#[pyclass(module = "rstar_geodetic")]
pub struct GeodeticPolygonTree {
    tree: GeodeticRTree<IndexedLeaf<GeodeticPolygon>>,
    geometries: Vec<Arc<GeodeticPolygon>>,
}

#[pymethods]
impl GeodeticPolygonTree {
    /// Builds a tree from polygons: a geopandas `GeoSeries`, a shapely `MultiPolygon`, a
    /// GeoJSON collection, or any iterable of polygon-likes.
    #[new]
    fn new(py: Python<'_>, data: &Bound<'_, PyAny>) -> PyResult<Self> {
        let geometries: Vec<Arc<GeodeticPolygon>> =
            collect_polygons(data)?.into_iter().map(Arc::new).collect();
        let leaves: Vec<IndexedLeaf<GeodeticPolygon>> = geometries
            .iter()
            .enumerate()
            .map(|(index, geometry)| IndexedLeaf {
                geometry: geometry.clone(),
                index,
            })
            .collect();
        let tree = py.detach(|| GeodeticRTree::bulk_load(leaves));
        Ok(Self { tree, geometries })
    }

    fn __len__(&self) -> usize {
        self.geometries.len()
    }

    /// The polygon at input position `index`.
    fn geometry(&self, index: usize) -> PyResult<Polygon> {
        self.geometries
            .get(index)
            .map(|arc| Polygon { inner: arc.clone() })
            .ok_or_else(|| PyIndexError::new_err("geometry index out of range"))
    }

    fn __getitem__(&self, index: usize) -> PyResult<Polygon> {
        self.geometry(index)
    }

    /// Every polygon, in input order.
    fn geometries(&self) -> Vec<Polygon> {
        self.geometries
            .iter()
            .map(|arc| Polygon { inner: arc.clone() })
            .collect()
    }

    /// The input position of the nearest polygon to `query` (zero distance if `query` is
    /// inside one), or `None` if empty.
    fn nearest(&self, query: &Bound<'_, PyAny>) -> PyResult<Option<usize>> {
        let coord = query_coord(query)?;
        Ok(self
            .tree
            .nearest_neighbor_with_distance(coord)
            .map(|(leaf, _)| leaf.index))
    }

    /// The `(input position, distance in metres)` of the nearest polygon, or `None`.
    fn nearest_with_distance(&self, query: &Bound<'_, PyAny>) -> PyResult<Option<(usize, f64)>> {
        let coord = query_coord(query)?;
        Ok(self
            .tree
            .nearest_neighbor_with_distance(coord)
            .map(|(leaf, metres)| (leaf.index, metres)))
    }

    /// The input positions of every polygon within `radius_metres` of `query`. With
    /// `return_distance=True`, `(index, distance)` pairs instead.
    #[pyo3(signature = (query, radius_metres, return_distance=false))]
    fn within_distance<'py>(
        &self,
        py: Python<'py>,
        query: &Bound<'_, PyAny>,
        radius_metres: f64,
        return_distance: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        let coord = query_coord(query)?;
        let query_vector = UnitVec::from(coord);
        let pairs: Vec<(usize, f64)> = self
            .tree
            .locate_within_distance(coord, radius_metres)
            .map(|leaf| {
                (
                    leaf.index,
                    squared_chord_to_metres(leaf.distance_2(&query_vector)),
                )
            })
            .collect();
        within_result(py, pairs, return_distance)
    }
}
