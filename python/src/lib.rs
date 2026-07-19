//! Python bindings for `rstar_geodetic`, built with PyO3 and maturin.
//!
//! The compiled extension is `rstar_geodetic._rstar_geodetic`; the `rstar_geodetic`
//! package re-exports its public names. See the package `__init__.py` and the crate
//! README for the Python-level API.

use pyo3::prelude::*;

/// The extension module. Its name must match the `[lib] name` and the final component of
/// `module-name` in `pyproject.toml`.
#[pymodule]
fn _rstar_geodetic(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
