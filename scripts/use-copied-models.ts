// Points Bolke's model catalog at its own Hugging Face copies.
//
//   bun scripts/use-copied-models.ts copied-models.json
//
// Keeps only the models listed in scripts/bolke-models.json, swaps each one's
// repository and revision for its copy. Only each model's default quant is
// copied; the other quants stay listed so files already on disk are still
// recognised. Writes packages/models/catalog.json, then run
// `bun run sync:catalog` to update the Rust copy.
import { readFileSync, writeFileSync } from "node:fs";

type Copied = Record<string, { repository: string; revision: string }>;
interface CatalogModel {
  id: string;
  repository: string;
  revision: string;
  previousRevisions: string[];
}
interface Catalog {
  models: CatalogModel[];
  results: Record<string, unknown>;
  [key: string]: unknown;
}

const copiedPath = process.argv[2];
if (!copiedPath) {
  console.error("usage: bun scripts/use-copied-models.ts copied-models.json");
  process.exit(1);
}

const root = new URL("../", import.meta.url);
const catalogUrl = new URL("packages/models/catalog.json", root);
const wanted: string[] = JSON.parse(
  readFileSync(new URL("scripts/bolke-models.json", root), "utf8"),
).models;
const copied: Copied = JSON.parse(readFileSync(copiedPath, "utf8"));
const catalog: Catalog = JSON.parse(readFileSync(catalogUrl, "utf8"));

const missing = wanted.filter((id) => !copied[id]);
if (missing.length) {
  console.error(`Not copied yet: ${missing.join(", ")}`);
  process.exit(1);
}

catalog.models = catalog.models
  .filter((m) => wanted.includes(m.id))
  .map((m) => ({
    ...m,
    repository: copied[m.id].repository,
    revision: copied[m.id].revision,
    previousRevisions: [],
  }));
catalog.results = Object.fromEntries(
  Object.entries(catalog.results).filter(([id]) => wanted.includes(id)),
);

writeFileSync(catalogUrl, JSON.stringify(catalog, null, 2) + "\n");
console.log(
  `Catalog now lists ${catalog.models.length} models from Bolke's own copies.`,
);
