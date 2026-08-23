// Downloads GeoNames populated places (cities500: every place with a population of
// 500 or more; CC BY 4.0) and writes the demo's data files into public/data/:
//
//   cities.bin   Float32 [lon, lat, lon, lat, ...], sorted by population, descending
//   cities.tsv   one "name<TAB>country code<TAB>population" line per point, same order
//   cities.json  { count, brightCount, source, fetched }
//
// Run with `npm run data`. Skips the download if the files are already present.

import { access, mkdir, writeFile } from "node:fs/promises";
import { unzipSync, strFromU8 } from "fflate";

const SOURCE = "https://download.geonames.org/export/dump/cities500.zip";
const OUT_DIR = new URL("../public/data/", import.meta.url);
const BRIGHT_POPULATION = 500_000;

async function exists(url) {
  try {
    await access(url);
    return true;
  } catch {
    return false;
  }
}

async function main() {
  await mkdir(OUT_DIR, { recursive: true });
  const outputs = ["cities.bin", "cities.tsv", "cities.json"].map(
    (name) => new URL(name, OUT_DIR),
  );
  if (!process.argv.includes("--force")) {
    const present = await Promise.all(outputs.map(exists));
    if (present.every(Boolean)) {
      console.log("public/data is present; pass --force to refetch");
      return;
    }
  }

  console.log(`fetching ${SOURCE}`);
  const response = await fetch(SOURCE);
  if (!response.ok) {
    throw new Error(`${SOURCE}: HTTP ${response.status}`);
  }
  const zip = new Uint8Array(await response.arrayBuffer());
  const files = unzipSync(zip);
  const entry = files["cities500.txt"];
  if (!entry) {
    throw new Error("cities500.txt missing from the archive");
  }
  const text = strFromU8(entry);

  // Columns: 0 geonameid, 1 name, 4 latitude, 5 longitude, 8 country code, 14 population.
  const rows = [];
  for (const line of text.split("\n")) {
    if (line === "") {
      continue;
    }
    const fields = line.split("\t");
    const lat = Number(fields[4]);
    const lon = Number(fields[5]);
    if (!Number.isFinite(lat) || !Number.isFinite(lon)) {
      continue;
    }
    rows.push({
      name: fields[1] ?? "",
      country: fields[8] ?? "",
      population: Number(fields[14]) || 0,
      lon,
      lat,
    });
  }
  rows.sort((a, b) => b.population - a.population);

  const coords = new Float32Array(2 * rows.length);
  const lines = new Array(rows.length);
  let brightCount = 0;
  rows.forEach((row, i) => {
    coords[2 * i] = row.lon;
    coords[2 * i + 1] = row.lat;
    lines[i] = `${row.name.replaceAll("\t", " ")}\t${row.country}\t${row.population}`;
    if (row.population >= BRIGHT_POPULATION) {
      brightCount = i + 1;
    }
  });

  await writeFile(outputs[0], Buffer.from(coords.buffer));
  await writeFile(outputs[1], lines.join("\n") + "\n");
  await writeFile(
    outputs[2],
    JSON.stringify(
      {
        count: rows.length,
        brightCount,
        source: SOURCE,
        fetched: new Date().toISOString().slice(0, 10),
      },
      null,
      2,
    ) + "\n",
  );
  console.log(`wrote ${rows.length} places (${brightCount} with population >= ${BRIGHT_POPULATION})`);
}

await main();
