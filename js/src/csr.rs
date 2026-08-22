//! Validation of the CSR (GeoArrow) offset layout the linestring and polygon
//! constructors take.

use wasm_bindgen::JsValue;

use crate::error::geodetic_error_message;

/// Checks that `coords` holds interleaved pairs and returns the vertex count.
pub fn vertex_count(coords: &[f64]) -> Result<usize, JsValue> {
    if coords.len() % 2 != 0 {
        return Err(geodetic_error_message(
            "coordinate array must have even length (interleaved lon, lat pairs)",
        ));
    }
    Ok(coords.len() / 2)
}

/// Checks that `offsets` partitions `total` elements: at least one entry, the first `0`,
/// the last `total`, and the sequence non-decreasing. `what` names the array in the error
/// message. Returns the number of parts (`offsets.len() - 1`).
pub fn validate_offsets(offsets: &[u32], total: usize, what: &str) -> Result<usize, JsValue> {
    let valid = offsets.first() == Some(&0)
        && offsets.last().is_some_and(|&last| last as usize == total)
        && offsets.windows(2).all(|pair| pair[0] <= pair[1]);
    if valid {
        Ok(offsets.len() - 1)
    } else {
        Err(geodetic_error_message(&format!(
            "{what} must start at 0, end at {total}, and be non-decreasing"
        )))
    }
}

/// The interleaved `[lon, lat]` degree pairs at vertex positions `start..end`.
pub fn vertex_pairs(coords: &[f64], start: u32, end: u32) -> impl Iterator<Item = (f64, f64)> {
    (start as usize..end as usize).map(move |i| (coords[i * 2], coords[i * 2 + 1]))
}
