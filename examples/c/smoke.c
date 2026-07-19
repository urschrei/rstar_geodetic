/*
 * A minimal smoke test of the rstar_geodetic C API: build a point tree, run one
 * nearest-neighbour and one radius query, then release everything. Compiled and run by
 * `just ffi-smoke`.
 */

#include <stdio.h>
#include <stdlib.h>

#include "rstar_geodetic.h"

int main(void) {
    /* London, Paris, Berlin, as interleaved [lon, lat] degrees. */
    double coords[] = {
        -0.1278, 51.5074,
        2.3522, 48.8566,
        13.4050, 52.5200,
    };

    RsgPointTree *tree = NULL;
    RsgStatus status = rsg_point_tree_new(coords, 3, &tree);
    if (status != RSG_OK) {
        fprintf(stderr, "construct failed: %s\n", rsg_status_message(status));
        return 1;
    }

    size_t size = 0;
    rsg_point_tree_size(tree, &size);
    printf("tree size: %zu\n", size);

    /* Nearest city to Amsterdam: expect London, at input index 0. */
    RsgNeighbor nearest;
    bool found = false;
    status = rsg_point_tree_nearest_neighbor(tree, 4.9041, 52.3676, &nearest, &found);
    if (status != RSG_OK || !found) {
        fprintf(stderr, "nearest failed: %s\n", rsg_status_message(status));
        rsg_point_tree_free(tree);
        return 1;
    }
    printf("nearest index %zu at %.1f m\n", nearest.index, nearest.distance_metres);
    if (nearest.index != 0) {
        fprintf(stderr, "unexpected nearest index %zu (wanted 0)\n", nearest.index);
        rsg_point_tree_free(tree);
        return 1;
    }

    /* Radius query: cities within 1000 km of the same point. */
    RsgNeighbor *items = NULL;
    size_t len = 0;
    status = rsg_point_tree_within_distance(tree, 4.9041, 52.3676, 1000000.0, &items, &len);
    if (status != RSG_OK) {
        fprintf(stderr, "within failed: %s\n", rsg_status_message(status));
        rsg_point_tree_free(tree);
        return 1;
    }
    printf("within 1000 km: %zu points\n", len);
    rsg_neighbors_free(items, len);

#ifdef RSG_HAVE_WGS84
    status = rsg_point_tree_nearest_neighbor_wgs84(tree, 4.9041, 52.3676, &nearest, &found);
    if (status != RSG_OK || !found) {
        fprintf(stderr, "wgs84 nearest failed: %s\n", rsg_status_message(status));
        rsg_point_tree_free(tree);
        return 1;
    }
    printf("wgs84 nearest index %zu at %.1f m\n", nearest.index, nearest.distance_metres);
#endif

    rsg_point_tree_free(tree);
    printf("smoke ok\n");
    return 0;
}
