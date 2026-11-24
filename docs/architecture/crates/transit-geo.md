# transit-geo

The one distance between two coordinates. The crate depends on no
other crate.

- `distance_m` is the great-circle distance in meters on a sphere of
  the mean earth radius, 6,371,000 m (haversine). The stop directory,
  the stops, the router's footpaths, the street graph's segment
  lengths, a diversion's detour length, and the coverage analysis
  use it.
- `METERS_PER_DEGREE` and `meters_per_degree_lon` are the local frame
  on the same sphere, for code that works in flat coordinates: the
  street graph's snap, which projects a point onto a segment, and the
  coverage analysis's grid and areas. Within 5 km, the frame agrees
  with `distance_m` to 0.1%.

## Why

**One distance (D-250).** Two answers about the same two points must
agree. When the libraries each had their own distance, the street
graph and a diversion used a flat-earth metric (110,540 m for each
degree of latitude), and the stop directory used the great circle
(111,195 m). A walk measured north to south differed by about 0.6%
between them. The great circle won because three libraries already
used it, so their answers did not change, and because it has no error
that grows with distance. The flat frame stays for code that must work
in flat coordinates, and it uses the same sphere, so it agrees at the
scale it is used at.

The alternatives were a flat-earth metric for all (faster, but wrong
over long distances and at high latitudes) and an ellipsoid (WGS 84,
more exact, but a transit walk does not need better than 0.5%, and no
other answer in the repo uses one).

**`asin`, not `atan2`.** The final step is `2·asin(√a)`, as the three
libraries that already used the great circle wrote it, so their answers
keep the same bytes. Near the antipodes it loses a few decimeters in
20,000 km, which no transit answer reaches.
