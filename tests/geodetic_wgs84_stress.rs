//! High-volume stress tests for the wgs84 spherical-prefilter soundness margin.
//!
//! The hegel property suite runs 100 cases per test; these run hundreds of thousands,
//! with radii biased onto point-distance boundaries where an unsound margin would drop
//! an in-range point. Ignored by default; run explicitly with
//! `cargo nextest r --features wgs84 --run-ignored all -E 'binary(geodetic_wgs84_stress)'`.
#![cfg(feature = "wgs84")]

use rand::RngExt;
use rand::SeedableRng;
use rand::rngs::StdRng;
use rstar_geodetic::{GeodeticCoord, GeodeticPoint, GeodeticRTree, geodesic_distance_wgs84};

fn sphere_point(rng: &mut StdRng) -> GeodeticCoord {
    GeodeticCoord {
        lon: rng.random_range(-180.0..=180.0),
        lat: rng.random_range(-1.0f64..=1.0).asin().to_degrees(),
    }
}

/// The radius-query set must equal a linear geodesic scan, including when the radius
/// sits exactly on, or within one ulp of, a point's geodesic distance.
#[test]
#[ignore = "stress test: hundreds of thousands of cases, run explicitly"]
fn wgs84_radius_query_matches_scan_at_boundaries() {
    let mut failures = 0usize;
    for case in 0u64..200_000 {
        let mut rng = StdRng::seed_from_u64(case);
        let n = rng.random_range(1..=40);
        let points: Vec<GeodeticPoint> = (0..n)
            .map(|_| {
                let c = sphere_point(&mut rng);
                GeodeticPoint::new(c.lon, c.lat)
            })
            .collect();
        let query = sphere_point(&mut rng);
        // Bias the radius onto a boundary: exactly a point's distance, one ulp
        // below it, or uniform up to 10,000 km.
        let target = &points[rng.random_range(0..n)];
        let d = geodesic_distance_wgs84(query, target.coord());
        let radius = match case % 4 {
            0 => d,
            1 => f64::from_bits(d.to_bits().saturating_sub(1)),
            2 => d * rng.random_range(0.999..=1.001),
            _ => rng.random_range(0.0..=10_000_000.0),
        };

        let tree = GeodeticRTree::bulk_load(points.clone());
        let mut from_tree: Vec<(u64, u64)> = tree
            .locate_within_distance_wgs84(query, radius)
            .map(|p| (p.coord().lon.to_bits(), p.coord().lat.to_bits()))
            .collect();
        let mut from_scan: Vec<(u64, u64)> = points
            .iter()
            .filter(|p| geodesic_distance_wgs84(query, p.coord()) <= radius)
            .map(|p| (p.coord().lon.to_bits(), p.coord().lat.to_bits()))
            .collect();
        from_tree.sort_unstable();
        from_scan.sort_unstable();
        if from_tree != from_scan {
            failures += 1;
            eprintln!(
                "case {case}: query=({},{}) radius={radius} tree={} scan={}",
                query.lon,
                query.lat,
                from_tree.len(),
                from_scan.len()
            );
        }
    }
    assert_eq!(failures, 0, "{failures} mismatching cases");
}

/// The margin claim itself: for random pairs, the embedded spherical distance never
/// exceeds the geodesic distance by more than the fetch inflation implies.
#[test]
#[ignore = "stress test: millions of pairs, run explicitly"]
fn spherical_geodesic_ratio_stays_within_margin() {
    let mut rng = StdRng::seed_from_u64(42);
    let mut max_ratio = 0.0f64;
    let mut arg = (
        GeodeticCoord { lon: 0.0, lat: 0.0 },
        GeodeticCoord { lon: 0.0, lat: 0.0 },
    );
    for _ in 0..2_000_000 {
        let a = sphere_point(&mut rng);
        let b = sphere_point(&mut rng);
        let geodesic = geodesic_distance_wgs84(a, b);
        if geodesic < 1.0 {
            continue;
        }
        let spherical = rstar_geodetic::haversine_distance(a, b);
        let ratio = spherical / geodesic;
        if ratio > max_ratio {
            max_ratio = ratio;
            arg = (a, b);
        }
    }
    // The WGS84 fetch inflation is 1 / (1 - margin) with margin about 1.117%.
    let limit = 1.0 / (1.0 - 0.011_17);
    assert!(
        max_ratio < limit,
        "max spherical/geodesic ratio {max_ratio} at ({},{})-({},{}) exceeds fetch inflation {limit}",
        arg.0.lon,
        arg.0.lat,
        arg.1.lon,
        arg.1.lat
    );
    eprintln!("max spherical/geodesic ratio: {max_ratio} (limit {limit})");
}
