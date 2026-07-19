//! Status codes for the C API and their static message strings.
//!
//! Every fallible entry point returns an [`RsgStatus`] and writes its result through an
//! out-parameter. The success code is `RSG_OK` (zero); every other value is an error and
//! leaves the out-parameters untouched. The [`crate::GeodeticError`] variants map one to
//! one onto the `RSG_ERR_*` codes; the three remaining codes cover conditions that only
//! exist at the boundary (a null pointer, malformed CSR offsets, or a caught panic).

use core::ffi::{CStr, c_char};

use crate::GeodeticError;

/// The outcome of a C API call. `RSG_OK` is zero; all other values are errors.
///
/// `#[repr(C)]` fixes the discriminants, so the values are a stable part of the ABI. The
/// variant names are the C constant names verbatim (hence the non-CamelCase spelling).
#[repr(C)]
#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RsgStatus {
    /// The call succeeded and the out-parameters are populated.
    RSG_OK = 0,
    /// A longitude was outside `[-180, 180]`.
    RSG_ERR_LON_OUT_OF_RANGE,
    /// A latitude was outside `[-90, 90]`.
    RSG_ERR_LAT_OUT_OF_RANGE,
    /// A coordinate was NaN or infinite.
    RSG_ERR_NOT_FINITE,
    /// A geometry had fewer vertices than its structure requires.
    RSG_ERR_TOO_FEW_POINTS,
    /// An edge spanned 180 degrees or more, so its shorter great-circle arc is undefined.
    RSG_ERR_EDGE_SPANS_HALF_CIRCLE,
    /// A polygon ring's first and last vertices differ (a ring must be explicitly closed).
    RSG_ERR_RING_NOT_CLOSED,
    /// A required pointer argument was null.
    RSG_ERR_NULL_ARGUMENT,
    /// A CSR offset array was not non-decreasing, or its final entry did not equal the
    /// count of the elements it partitions.
    RSG_ERR_INVALID_OFFSETS,
    /// A panic was caught at the boundary. The handle or buffer is left in a safe state,
    /// but the operation did not complete.
    RSG_ERR_INTERNAL_PANIC,
}

impl From<GeodeticError> for RsgStatus {
    fn from(error: GeodeticError) -> Self {
        match error {
            GeodeticError::LonOutOfRange(_) => RsgStatus::RSG_ERR_LON_OUT_OF_RANGE,
            GeodeticError::LatOutOfRange(_) => RsgStatus::RSG_ERR_LAT_OUT_OF_RANGE,
            GeodeticError::NotFinite => RsgStatus::RSG_ERR_NOT_FINITE,
            GeodeticError::TooFewPoints { .. } => RsgStatus::RSG_ERR_TOO_FEW_POINTS,
            GeodeticError::EdgeSpansHalfCircle { .. } => RsgStatus::RSG_ERR_EDGE_SPANS_HALF_CIRCLE,
            GeodeticError::RingNotClosed => RsgStatus::RSG_ERR_RING_NOT_CLOSED,
        }
    }
}

/// Returns a static, NUL-terminated description of `status`. The pointer is valid for the
/// lifetime of the process and must not be freed.
#[unsafe(no_mangle)]
pub extern "C" fn rsg_status_message(status: RsgStatus) -> *const c_char {
    let message: &CStr = match status {
        RsgStatus::RSG_OK => c"ok",
        RsgStatus::RSG_ERR_LON_OUT_OF_RANGE => c"longitude outside [-180, 180]",
        RsgStatus::RSG_ERR_LAT_OUT_OF_RANGE => c"latitude outside [-90, 90]",
        RsgStatus::RSG_ERR_NOT_FINITE => c"coordinate was NaN or infinite",
        RsgStatus::RSG_ERR_TOO_FEW_POINTS => c"too few vertices for the geometry",
        RsgStatus::RSG_ERR_EDGE_SPANS_HALF_CIRCLE => {
            c"an edge spans 180 degrees or more (densify it first)"
        }
        RsgStatus::RSG_ERR_RING_NOT_CLOSED => c"polygon ring is not closed (first vertex != last)",
        RsgStatus::RSG_ERR_NULL_ARGUMENT => c"a required pointer argument was null",
        RsgStatus::RSG_ERR_INVALID_OFFSETS => {
            c"CSR offsets are not monotonic or do not span the input"
        }
        RsgStatus::RSG_ERR_INTERNAL_PANIC => c"an internal panic was caught at the boundary",
    };
    message.as_ptr()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geodetic_error_maps_to_distinct_codes() {
        assert_eq!(
            RsgStatus::from(GeodeticError::LonOutOfRange(200.0)),
            RsgStatus::RSG_ERR_LON_OUT_OF_RANGE
        );
        assert_eq!(
            RsgStatus::from(GeodeticError::LatOutOfRange(200.0)),
            RsgStatus::RSG_ERR_LAT_OUT_OF_RANGE
        );
        assert_eq!(
            RsgStatus::from(GeodeticError::NotFinite),
            RsgStatus::RSG_ERR_NOT_FINITE
        );
        assert_eq!(
            RsgStatus::from(GeodeticError::TooFewPoints {
                found: 1,
                needed: 2
            }),
            RsgStatus::RSG_ERR_TOO_FEW_POINTS
        );
        assert_eq!(
            RsgStatus::from(GeodeticError::EdgeSpansHalfCircle { index: 0 }),
            RsgStatus::RSG_ERR_EDGE_SPANS_HALF_CIRCLE
        );
        assert_eq!(
            RsgStatus::from(GeodeticError::RingNotClosed),
            RsgStatus::RSG_ERR_RING_NOT_CLOSED
        );
    }

    #[test]
    fn ok_is_zero() {
        assert_eq!(RsgStatus::RSG_OK as i32, 0);
    }

    #[test]
    fn status_message_is_non_null_and_readable() {
        for status in [
            RsgStatus::RSG_OK,
            RsgStatus::RSG_ERR_NULL_ARGUMENT,
            RsgStatus::RSG_ERR_INVALID_OFFSETS,
            RsgStatus::RSG_ERR_INTERNAL_PANIC,
        ] {
            let ptr = rsg_status_message(status);
            assert!(!ptr.is_null());
            // Safe: the pointer is a static C string literal.
            let text = unsafe { CStr::from_ptr(ptr) };
            assert!(!text.to_bytes().is_empty());
        }
    }
}
