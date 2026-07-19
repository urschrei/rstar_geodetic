//! Immutable geometry view classes returned by the trees, each exposing
//! `__geo_interface__` so shapely and geopandas can consume them.
//!
//! Each wraps an `Arc` shared with the tree's leaves, so `tree.geometry(i)` hands back a
//! view without copying the coordinates.

use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::PyDict;

use rstar_geodetic::{GeodeticLineString, GeodeticPoint, GeodeticPolygon};

use crate::geo_interface::{linestring_mapping, point_mapping, polygon_mapping};

/// A geodetic point `(lon, lat)` in degrees.
#[pyclass(module = "rstar_geodetic", frozen)]
pub struct Point {
    pub(crate) inner: Arc<GeodeticPoint>,
}

#[pymethods]
impl Point {
    #[getter]
    fn x(&self) -> f64 {
        self.inner.coord().lon
    }

    #[getter]
    fn y(&self) -> f64 {
        self.inner.coord().lat
    }

    /// The `(lon, lat)` pair.
    #[getter]
    fn coordinates(&self) -> (f64, f64) {
        let coord = self.inner.coord();
        (coord.lon, coord.lat)
    }

    #[getter]
    fn __geo_interface__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        point_mapping(py, self.inner.coord())
    }

    fn __repr__(&self) -> String {
        let coord = self.inner.coord();
        format!("Point(lon={}, lat={})", coord.lon, coord.lat)
    }
}

/// A geodetic linestring: a sequence of `(lon, lat)` vertices joined by great-circle arcs.
#[pyclass(module = "rstar_geodetic", frozen)]
pub struct LineString {
    pub(crate) inner: Arc<GeodeticLineString>,
}

#[pymethods]
impl LineString {
    /// The `(lon, lat)` vertices.
    #[getter]
    fn coordinates(&self) -> Vec<(f64, f64)> {
        self.inner.coords().iter().map(|c| (c.lon, c.lat)).collect()
    }

    #[getter]
    fn __geo_interface__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        linestring_mapping(py, self.inner.coords())
    }

    fn __len__(&self) -> usize {
        self.inner.coords().len()
    }

    fn __repr__(&self) -> String {
        format!("LineString({} vertices)", self.inner.coords().len())
    }
}

/// A filled geodetic polygon: an exterior ring and any interior rings (holes).
#[pyclass(module = "rstar_geodetic", frozen)]
pub struct Polygon {
    pub(crate) inner: Arc<GeodeticPolygon>,
}

#[pymethods]
impl Polygon {
    /// The exterior ring's `(lon, lat)` vertices (closed; first == last).
    #[getter]
    fn exterior(&self) -> Vec<(f64, f64)> {
        self.inner
            .exterior()
            .coords()
            .iter()
            .map(|c| (c.lon, c.lat))
            .collect()
    }

    /// The interior rings, each a list of `(lon, lat)` vertices.
    #[getter]
    fn interiors(&self) -> Vec<Vec<(f64, f64)>> {
        self.inner
            .interiors()
            .iter()
            .map(|ring| ring.coords().iter().map(|c| (c.lon, c.lat)).collect())
            .collect()
    }

    #[getter]
    fn __geo_interface__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        polygon_mapping(py, &self.inner)
    }

    fn __repr__(&self) -> String {
        format!(
            "Polygon(exterior {} vertices, {} holes)",
            self.inner.exterior().coords().len(),
            self.inner.interiors().len()
        )
    }
}
