# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Initial release: a geodetic (longitude/latitude) R-tree built on `rstar`, using a
  unit-sphere embedding and great-circle distance.
- `GeodeticRTree` with `GeodeticPoint`, `GeodeticLineString`, and `GeodeticPolygon`
  leaves; nearest-neighbour, radius, and longitude/latitude window queries, including
  across the antimeridian and the poles.
- Optional `wgs84` feature: ellipsoidal geodesic refine (Karney, via
  `geographiclib-rs`) for the point tree, plus the standalone `geodesic_distance`.
- Default `geo-types` feature: `From`/`TryFrom` conversions between the geodetic
  geometries and `geo-types` (pulled in with `default-features = false`, so the base
  crate stays no_std). Inbound conversions validate; outbound are infallible. The
  multi-geometries (`MultiPoint`, `MultiLineString`, `MultiPolygon`) convert via
  `TryFrom` into a bulk-loaded `GeodeticRTree`.
- Property tests (Hegel) and an externally-anchored arc-distance reference.

This code was extracted from a feature branch of `rstar`. It uses only the public
`rstar` API.
