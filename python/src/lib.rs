//! Python bindings for `rstar_geodetic`, built with PyO3 and maturin.
//!
//! The compiled extension is `rstar_geodetic._rstar_geodetic`; the `rstar_geodetic`
//! package re-exports its public names. See the package `__init__.py` and the crate
//! README for the Python-level API.

use pyo3::prelude::*;

mod error;
mod geo_interface;
mod geometry;
mod tree;

/// The extension module. Its name must match the `[lib] name` and the final component of
/// `module-name` in `pyproject.toml`.
#[pymodule]
fn _rstar_geodetic(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add("__version__", env!("CARGO_PKG_VERSION"))?;
    module.add(
        "GeodeticError",
        module.py().get_type::<error::GeodeticError>(),
    )?;
    module.add_class::<geometry::Point>()?;
    module.add_class::<geometry::LineString>()?;
    module.add_class::<geometry::Polygon>()?;
    module.add_class::<tree::GeodeticPointTree>()?;
    module.add_class::<tree::GeodeticLineStringTree>()?;
    module.add_class::<tree::GeodeticPolygonTree>()?;
    Ok(())
}
