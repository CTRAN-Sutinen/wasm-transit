// generate-population.mjs — rebuild testdata/clark-population.geojson
// from public-domain census sources:
//
//   * ACS 5-year total population (B01003) per census tract, Clark
//     County WA (FIPS 53011), via the Census Reporter API
//   * 2020 Census Gazetteer tract centroids (INTPTLAT/INTPTLONG)
//
// The layer is the real-data dogfood asset for
// `analysis.type_coverage` — the committed counterpart to the
// synthetic web/e2e/fixtures/population.geojson: a point
// FeatureCollection, one point per tract (the Gazetteer's own
// centroid), properties { POP, GEOID }. Deterministic: features
// sorted by GEOID. The committed file is the pin; regenerating
// picks up the current ACS release.
//
// Run: node testdata/generate-population.mjs

import { writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { inflateRawSync } from "node:zlib";

const COUNTY = "53011"; // Clark County, WA
const GAZ_URL =
  "https://www2.census.gov/geo/docs/maps-data/data/gazetteer" +
  "/2020_Gazetteer/2020_Gaz_tracts_national.zip";
const POP_URL =
  "https://api.censusreporter.org/1.0/data/show/latest" +
  `?table_ids=B01003&geo_ids=140|05000US${COUNTY}`;

const HERE = dirname(fileURLToPath(import.meta.url));
const OUT = join(HERE, "clark-population.geojson");

async function getJSON(url) {
  const res = await fetch(url, { headers: { "User-Agent": "rustwasm-testdata/1.0" } });
  if (!res.ok) throw new Error(`${url}: HTTP ${res.status}`);
  return res.json();
}

// Minimal zip reader: walk the central directory, return the one
// deflated entry's bytes. No dependencies.
function unzipEntry(zipBuf, nameMatch) {
  const zip = new Uint8Array(zipBuf);
  const view = new DataView(zipBuf);
  let eocd = -1;
  for (let i = zip.length - 22; i >= 0 && eocd < 0; i--) {
    if (view.getUint32(i, true) === 0x06054b50) eocd = i;
  }
  if (eocd < 0) throw new Error("zip: no end-of-central-directory");
  const count = view.getUint16(eocd + 10, true);
  let ptr = view.getUint32(eocd + 16, true);
  for (let n = 0; n < count; n++) {
    if (view.getUint32(ptr, true) !== 0x02014b50) throw new Error("zip: bad central directory");
    const method = view.getUint16(ptr + 10, true);
    const csize = view.getUint32(ptr + 20, true);
    const nameLen = view.getUint16(ptr + 28, true);
    const extraLen = view.getUint16(ptr + 30, true);
    const commentLen = view.getUint16(ptr + 32, true);
    const localOffset = view.getUint32(ptr + 42, true);
    const name = new TextDecoder().decode(zip.subarray(ptr + 46, ptr + 46 + nameLen));
    if (nameMatch.test(name)) {
      const lNameLen = view.getUint16(localOffset + 26, true);
      const lExtraLen = view.getUint16(localOffset + 28, true);
      const start = localOffset + 30 + lNameLen + lExtraLen;
      const data = zip.subarray(start, start + csize);
      if (method !== 8) return data;
      return new Uint8Array(inflateRawSync(data));
    }
    ptr += 46 + nameLen + extraLen + commentLen;
  }
  throw new Error("zip: entry not found");
}

const pop = await getJSON(POP_URL);
const release = pop.release?.name ?? "unknown ACS release";

const zipBuf = await (await fetch(GAZ_URL)).arrayBuffer();
const gazText = new TextDecoder().decode(unzipEntry(zipBuf, /\.txt$/));
const gazLines = gazText.trim().split(/\r?\n/);
const header = gazLines[0].split("\t").map((s) => s.trim());
const iG = header.indexOf("GEOID");
const iLat = header.indexOf("INTPTLAT");
const iLon = header.indexOf("INTPTLONG");
if (iG < 0 || iLat < 0 || iLon < 0) throw new Error("gazetteer: expected columns missing");

const features = [];
let missing = 0;
for (const line of gazLines.slice(1)) {
  const cols = line.split("\t").map((s) => s.trim());
  const geoid = cols[iG];
  if (!geoid.startsWith(COUNTY)) continue;
  const p = pop.data[`14000US${geoid}`]?.B01003?.estimate?.B01003001;
  if (p == null) {
    missing++;
    continue;
  }
  features.push({
    type: "Feature",
    properties: { POP: p, GEOID: geoid },
    geometry: {
      type: "Point",
      coordinates: [parseFloat(cols[iLon]), parseFloat(cols[iLat])],
    },
  });
}
features.sort((a, b) => a.properties.GEOID.localeCompare(b.properties.GEOID));

const layer = { type: "FeatureCollection", features };
writeFileSync(OUT, JSON.stringify(layer, null, 1) + "\n");
const total = features.reduce((s, f) => s + f.properties.POP, 0);
console.log(
  `${OUT}: ${features.length} tract points, total POP ${total}` +
    ` (${release}${missing ? `, ${missing} tracts without population skipped` : ""})`,
);
