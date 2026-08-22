//! Query results as plain JavaScript objects.
//!
//! Results are built as ordinary objects rather than `wasm-bindgen` classes so that the
//! caller owns them outright: nothing to `free()`, and the typed arrays are copies in
//! JavaScript memory that stay valid after the tree is released.

use js_sys::{Float64Array, Object, Reflect, Uint32Array};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen(typescript_custom_section)]
const TS_RESULT_TYPES: &str = r#"
/** A nearest-neighbour result: the input position of the geometry and the distance to it. */
export interface Neighbor {
  /** The geometry's position in the input sequence. */
  index: number;
  /** Distance from the query point to the geometry, in metres. */
  distance: number;
}

/**
 * A within-distance result: the input positions of the matching geometries and, at the
 * same offsets, the distance to each. The arrays are in no particular order.
 */
export interface Neighbors {
  /** The matching geometries' positions in the input sequence. */
  indices: Uint32Array;
  /** Distance from the query point to each geometry, in metres. */
  distances: Float64Array;
}
"#;

#[wasm_bindgen]
extern "C" {
    /// A `{ index, distance }` object; see the TypeScript `Neighbor` interface.
    #[wasm_bindgen(typescript_type = "Neighbor")]
    pub type Neighbor;

    /// A `{ indices, distances }` object; see the TypeScript `Neighbors` interface.
    #[wasm_bindgen(typescript_type = "Neighbors")]
    pub type Neighbors;
}

/// Builds a `{ index, distance }` object.
pub fn neighbor(index: usize, distance_metres: f64) -> Neighbor {
    let object = Object::new();
    set(&object, "index", &(index as u32).into());
    set(&object, "distance", &distance_metres.into());
    object.unchecked_into()
}

/// Builds a `{ indices, distances }` object from parallel vectors. The typed arrays are
/// copies into JavaScript memory.
pub fn neighbors(indices: &[u32], distances: &[f64]) -> Neighbors {
    debug_assert_eq!(indices.len(), distances.len());
    let object = Object::new();
    set(&object, "indices", &Uint32Array::from(indices));
    set(&object, "distances", &Float64Array::from(distances));
    object.unchecked_into()
}

/// Sets a property on a freshly created plain object. `Reflect::set` fails only on
/// frozen or exotic objects, neither of which applies here.
fn set(object: &Object, key: &str, value: &wasm_bindgen::JsValue) {
    Reflect::set(object, &key.into(), value).expect("setting a property on a new plain object");
}
