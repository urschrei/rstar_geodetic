//! The JavaScript error raised on invalid coordinates or geometry.

use wasm_bindgen::JsValue;

/// The name of the error thrown on validation failure, so callers can distinguish it
/// from other errors by `error.name`.
const ERROR_NAME: &str = "GeodeticError";

/// Converts a core [`rstar_geodetic::GeodeticError`] into a JavaScript `Error` named
/// `GeodeticError`, carrying its human-readable message.
pub fn geodetic_error(error: rstar_geodetic::GeodeticError) -> JsValue {
    let js_error = js_sys::Error::new(&error.to_string());
    js_error.set_name(ERROR_NAME);
    js_error.into()
}

/// A `GeodeticError` with a message that does not come from the core crate, for
/// argument checks the bindings perform themselves.
pub fn geodetic_error_message(message: &str) -> JsValue {
    let js_error = js_sys::Error::new(message);
    js_error.set_name(ERROR_NAME);
    js_error.into()
}
