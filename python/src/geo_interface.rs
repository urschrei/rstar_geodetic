//! Parsing Python geometries into the geodetic types, and building the `__geo_interface__`
//! mappings that go back out.
//!
//! Inbound, an object is read as (in order): its `__geo_interface__` attribute if present,
//! otherwise a mapping with a `"type"` key, otherwise a bare coordinate sequence. The
//! GeoJSON-style `"type"` selects how the coordinates are interpreted. Feature and the
//! collection types (`FeatureCollection`, `GeometryCollection`, `Multi*`) are expanded to
//! their member geometries for the tree constructors.

// The inbound parsers are wired up by the tree constructors in a later step.
#![allow(dead_code)]

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use rstar_geodetic::{
    GeodeticCoord, GeodeticLineString, GeodeticPoint, GeodeticPolygon, GeodeticRing,
};

use crate::error::map_error;

// --- inbound: Python object -> geodetic geometry ---

/// The geo mapping to interpret: the object's `__geo_interface__` dict, or the object
/// itself if it is a mapping carrying a `"type"`. `None` means the object is a bare
/// coordinate sequence.
fn geo_mapping<'py>(obj: &Bound<'py, PyAny>) -> PyResult<Option<Bound<'py, PyDict>>> {
    if let Ok(interface) = obj.getattr("__geo_interface__") {
        let dict = interface
            .cast_into::<PyDict>()
            .map_err(|_| PyValueError::new_err("__geo_interface__ did not return a mapping"))?;
        return Ok(Some(dict));
    }
    if let Ok(dict) = obj.cast::<PyDict>()
        && dict.contains("type")?
    {
        return Ok(Some(dict.clone()));
    }
    Ok(None)
}

fn get_type(dict: &Bound<'_, PyDict>) -> PyResult<String> {
    dict.get_item("type")?
        .ok_or_else(|| PyValueError::new_err("geo mapping is missing 'type'"))?
        .extract()
}

fn item<'py>(dict: &Bound<'py, PyDict>, key: &str) -> PyResult<Bound<'py, PyAny>> {
    dict.get_item(key)?
        .ok_or_else(|| PyValueError::new_err(format!("geo mapping is missing '{key}'")))
}

fn type_mismatch(wanted: &str, found: &str) -> PyErr {
    PyValueError::new_err(format!("expected a {wanted} geometry, found '{found}'"))
}

/// A single `[lon, lat]` (a longer sequence's elevation and beyond are ignored).
fn coord_pair(obj: &Bound<'_, PyAny>) -> PyResult<(f64, f64)> {
    let values: Vec<f64> = obj
        .extract()
        .map_err(|_| PyValueError::new_err("a coordinate must be a sequence of numbers"))?;
    if values.len() < 2 {
        return Err(PyValueError::new_err(
            "a coordinate needs at least a longitude and a latitude",
        ));
    }
    Ok((values[0], values[1]))
}

/// A sequence of `[lon, lat]` coordinate pairs.
fn coord_list(obj: &Bound<'_, PyAny>) -> PyResult<Vec<(f64, f64)>> {
    let mut coords = Vec::new();
    for pair in obj.try_iter()? {
        coords.push(coord_pair(&pair?)?);
    }
    Ok(coords)
}

/// Parses one point from a geo-interface object, a `Point`/`Feature` mapping, or a bare
/// `[lon, lat]` sequence.
pub(crate) fn parse_point(obj: &Bound<'_, PyAny>) -> PyResult<GeodeticPoint> {
    match geo_mapping(obj)? {
        Some(dict) => match get_type(&dict)?.as_str() {
            "Point" => {
                let (lon, lat) = coord_pair(&item(&dict, "coordinates")?)?;
                GeodeticPoint::try_new(lon, lat).map_err(map_error)
            }
            "Feature" => parse_point(&item(&dict, "geometry")?),
            other => Err(type_mismatch("Point", other)),
        },
        None => {
            let (lon, lat) = coord_pair(obj)?;
            GeodeticPoint::try_new(lon, lat).map_err(map_error)
        }
    }
}

/// Parses one linestring from a geo-interface object, a `LineString`/`Feature` mapping, or
/// a bare sequence of coordinate pairs.
pub(crate) fn parse_linestring(obj: &Bound<'_, PyAny>) -> PyResult<GeodeticLineString> {
    match geo_mapping(obj)? {
        Some(dict) => match get_type(&dict)?.as_str() {
            "LineString" => {
                GeodeticLineString::try_from_lonlat(coord_list(&item(&dict, "coordinates")?)?)
                    .map_err(map_error)
            }
            "Feature" => parse_linestring(&item(&dict, "geometry")?),
            other => Err(type_mismatch("LineString", other)),
        },
        None => GeodeticLineString::try_from_lonlat(coord_list(obj)?).map_err(map_error),
    }
}

/// Parses one polygon from a geo-interface object, a `Polygon`/`Feature` mapping, or a
/// bare sequence of rings (exterior first).
pub(crate) fn parse_polygon(obj: &Bound<'_, PyAny>) -> PyResult<GeodeticPolygon> {
    match geo_mapping(obj)? {
        Some(dict) => match get_type(&dict)?.as_str() {
            "Polygon" => polygon_from_rings(&item(&dict, "coordinates")?),
            "Feature" => parse_polygon(&item(&dict, "geometry")?),
            other => Err(type_mismatch("Polygon", other)),
        },
        None => polygon_from_rings(obj),
    }
}

fn polygon_from_rings(obj: &Bound<'_, PyAny>) -> PyResult<GeodeticPolygon> {
    let mut rings = Vec::new();
    for ring in obj.try_iter()? {
        rings.push(GeodeticRing::try_from_lonlat(coord_list(&ring?)?).map_err(map_error)?);
    }
    let mut rings = rings.into_iter();
    let exterior = rings
        .next()
        .ok_or_else(|| PyValueError::new_err("a polygon needs an exterior ring"))?;
    GeodeticPolygon::try_new(exterior, rings.collect()).map_err(map_error)
}

/// Expands a constructor argument into the individual element objects to parse: the
/// members of a `FeatureCollection`/`GeometryCollection`/`Multi*`, a one-element list for a
/// single geometry or `Feature`, or the items of a plain iterable of geometry-likes.
fn expand<'py>(obj: &Bound<'py, PyAny>) -> PyResult<Vec<Bound<'py, PyAny>>> {
    match geo_mapping(obj)? {
        Some(dict) => match get_type(&dict)?.as_str() {
            "FeatureCollection" => sequence(&item(&dict, "features")?),
            "GeometryCollection" => sequence(&item(&dict, "geometries")?),
            "MultiPoint" | "MultiLineString" | "MultiPolygon" => {
                sequence(&item(&dict, "coordinates")?)
            }
            _ => Ok(vec![obj.clone()]),
        },
        None => sequence(obj),
    }
}

fn sequence<'py>(obj: &Bound<'py, PyAny>) -> PyResult<Vec<Bound<'py, PyAny>>> {
    let mut items = Vec::new();
    for element in obj.try_iter()? {
        items.push(element?);
    }
    Ok(items)
}

/// Collects every element of `obj` as a point (see [`expand`] and [`parse_point`]).
pub(crate) fn collect_points(obj: &Bound<'_, PyAny>) -> PyResult<Vec<GeodeticPoint>> {
    expand(obj)?.iter().map(parse_point).collect()
}

/// Collects every element of `obj` as a linestring.
pub(crate) fn collect_linestrings(obj: &Bound<'_, PyAny>) -> PyResult<Vec<GeodeticLineString>> {
    expand(obj)?.iter().map(parse_linestring).collect()
}

/// Collects every element of `obj` as a polygon.
pub(crate) fn collect_polygons(obj: &Bound<'_, PyAny>) -> PyResult<Vec<GeodeticPolygon>> {
    expand(obj)?.iter().map(parse_polygon).collect()
}

// --- outbound: geodetic geometry -> __geo_interface__ mapping ---

fn pairs(coords: &[GeodeticCoord]) -> Vec<(f64, f64)> {
    coords.iter().map(|c| (c.lon, c.lat)).collect()
}

pub(crate) fn point_mapping(py: Python<'_>, coord: GeodeticCoord) -> PyResult<Bound<'_, PyDict>> {
    let dict = PyDict::new(py);
    dict.set_item("type", "Point")?;
    dict.set_item("coordinates", (coord.lon, coord.lat))?;
    Ok(dict)
}

pub(crate) fn linestring_mapping<'py>(
    py: Python<'py>,
    coords: &[GeodeticCoord],
) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    dict.set_item("type", "LineString")?;
    dict.set_item("coordinates", pairs(coords))?;
    Ok(dict)
}

pub(crate) fn polygon_mapping<'py>(
    py: Python<'py>,
    polygon: &GeodeticPolygon,
) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    dict.set_item("type", "Polygon")?;
    let mut rings = vec![pairs(polygon.exterior().coords())];
    for interior in polygon.interiors() {
        rings.push(pairs(interior.coords()));
    }
    dict.set_item("coordinates", rings)?;
    Ok(dict)
}
