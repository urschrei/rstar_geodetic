//! WebAssembly bindings over the geodetic trees, built with `wasm-bindgen`.
//!
//! Coordinates cross the boundary as interleaved `[lon0, lat0, lon1, lat1, ...]`
//! `Float64Array`s (the layout the C API and GeoArrow use); query results come back as
//! input positions (`Uint32Array`) and distances in metres (`Float64Array`). See the
//! package README for the JavaScript-side API.

use wasm_bindgen::prelude::wasm_bindgen;

mod csr;
mod error;
mod indexed_query;
mod line_tree;
mod point_tree;
mod polygon_tree;
mod result;

pub use error::geodetic_error;
pub use line_tree::LineStringTree;
pub use point_tree::PointTree;
pub use polygon_tree::PolygonTree;
pub use result::{Neighbor, Neighbors};

/// Installs the panic hook once, when the module is instantiated.
#[wasm_bindgen(start)]
fn start() {
    console_error_panic_hook::set_once();
}
