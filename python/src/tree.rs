//! The three tree classes: `GeodeticPointTree`, `GeodeticLineStringTree`, and
//! `GeodeticPolygonTree`.
//!
//! Queries return integer input positions (the shapely `STRtree` convention). Each point
//! tree leaf is an [`IndexedPoint`] carrying its position; the linestring and polygon
//! trees wrap each geometry in an [`IndexedLeaf`] that carries its position, sharing one
//! `Arc` with the by-index geometry store.

use std::sync::Arc;

use pyo3::exceptions::PyIndexError;
use pyo3::prelude::*;
use pyo3::types::PyList;

use rstar::PointDistance;
use rstar_geodetic::indexed::{
    IndexedLeaf, IndexedPoint, indexed_points, nearest_position, nearest_position_wgs84,
};
use rstar_geodetic::{
    GeodeticCoord, GeodeticLineString, GeodeticPoint, GeodeticPolygon, GeodeticRTree, PointLeaf,
    UnitVec, geodesic_distance_wgs84, squared_chord_to_metres,
};

use crate::geo_interface::{collect_linestrings, collect_points, collect_polygons, parse_point};
use crate::geometry::{LineString, Point, Polygon};

// --- shared helpers ---

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
    tree: GeodeticRTree<IndexedPoint>,
    geometries: Vec<Arc<GeodeticPoint>>,
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
        let tree = py.detach(|| GeodeticRTree::bulk_load(indexed_points(points)));
        Ok(Self { tree, geometries })
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
        Ok(nearest_position(&self.tree, coord).map(|(index, _)| index))
    }

    /// The `(input position, distance in metres)` of the nearest point to `query`, or
    /// `None` if the tree is empty.
    fn nearest_with_distance(&self, query: &Bound<'_, PyAny>) -> PyResult<Option<(usize, f64)>> {
        let coord = query_coord(query)?;
        Ok(nearest_position(&self.tree, coord))
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
        let pairs: Vec<(usize, f64)> = self
            .tree
            .locate_within_distance(coord, radius_metres)
            .map(|leaf| {
                (
                    leaf.data,
                    squared_chord_to_metres(leaf.distance_2(&query_vector)),
                )
            })
            .collect();
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
        Ok(self
            .tree
            .locate_in_rectangle(lower, upper)
            .map(|leaf| leaf.data)
            .collect())
    }

    /// The input position of the nearest point to `query` by WGS84-ellipsoid geodesic
    /// distance, or `None` if the tree is empty.
    fn nearest_wgs84(&self, query: &Bound<'_, PyAny>) -> PyResult<Option<usize>> {
        let coord = query_coord(query)?;
        Ok(nearest_position_wgs84(&self.tree, coord).map(|(index, _)| index))
    }

    /// The `(input position, geodesic distance in metres)` of the nearest point to `query`
    /// on the WGS84 ellipsoid, or `None` if empty.
    fn nearest_with_distance_wgs84(
        &self,
        query: &Bound<'_, PyAny>,
    ) -> PyResult<Option<(usize, f64)>> {
        let coord = query_coord(query)?;
        Ok(nearest_position_wgs84(&self.tree, coord))
    }

    /// The input positions of every point within `radius_metres` WGS84-ellipsoid geodesic
    /// metres of `query`. With `return_distance=True`, `(index, distance)` pairs instead.
    #[pyo3(signature = (query, radius_metres, return_distance=false))]
    fn within_distance_wgs84<'py>(
        &self,
        py: Python<'py>,
        query: &Bound<'_, PyAny>,
        radius_metres: f64,
        return_distance: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        let coord = query_coord(query)?;
        let pairs: Vec<(usize, f64)> = self
            .tree
            .locate_within_distance_wgs84(coord, radius_metres)
            .map(|leaf| (leaf.data, geodesic_distance_wgs84(coord, leaf.coord())))
            .collect();
        within_result(py, pairs, return_distance)
    }
}

// --- linestring tree ---

/// A geodetic R-tree over linestrings.
#[pyclass(module = "rstar_geodetic")]
pub struct GeodeticLineStringTree {
    tree: GeodeticRTree<IndexedLeaf<Arc<GeodeticLineString>>>,
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
        let leaves: Vec<IndexedLeaf<Arc<GeodeticLineString>>> = geometries
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
    tree: GeodeticRTree<IndexedLeaf<Arc<GeodeticPolygon>>>,
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
        let leaves: Vec<IndexedLeaf<Arc<GeodeticPolygon>>> = geometries
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
