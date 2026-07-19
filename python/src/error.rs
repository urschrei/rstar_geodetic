//! The Python exception raised on invalid coordinates or geometry.

use pyo3::PyErr;
use pyo3::create_exception;
use pyo3::exceptions::PyValueError;

create_exception!(
    rtree_geodetic,
    GeodeticError,
    PyValueError,
    "Raised when coordinates or a geometry fail validation (out-of-range longitude or \
     latitude, a non-finite value, too few vertices, an edge spanning half the sphere, or \
     an unclosed ring)."
);

/// Converts a core [`rstar_geodetic::GeodeticError`] into the Python `GeodeticError`,
/// carrying its human-readable message.
pub(crate) fn map_error(error: rstar_geodetic::GeodeticError) -> PyErr {
    GeodeticError::new_err(error.to_string())
}
