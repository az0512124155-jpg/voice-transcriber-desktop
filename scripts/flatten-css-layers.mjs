// Flattens CSS cascade layers out of the built stylesheets.
//
// Tailwind 4 emits every rule inside `@layer theme / base / utilities`. A
// browser that does not understand `@layer` (Safari before 15.4, which is what
// macOS 11 and 12.0–12.2 ship) skips those blocks entirely, so the app arrives
// with no colours, no spacing and no layout — a wall of grey boxes.
//
// Removing the wrappers costs nothing on a modern engine: the blocks are
// already emitted in cascade order (properties → theme → base → utilities), so
// source order reproduces exactly what the layer order expressed.
//
// Opt-in via FLATTEN_CSS_LAYERS=1 so the Windows build, which has never had a
// problem here, keeps its layers and its byte-identical output.
import { readdir, readFile, writeFile, stat } from "node:fs/promises";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const OUT_DIR = process.argv[2] ?? "dist/assets";

/** Index just past the `}` that closes the block opened at `open`. */
function closingBrace(css, open) {
  let depth = 0;
  for (let i = open; i < css.length; i++) {
    const c = css[i];
    if (c === "{") depth++;
    else if (c === "}") {
      depth--;
      if (depth === 0) return i;
    }
  }
  throw new Error("unbalanced braces while flattening @layer");
}

/** Drops every `@layer name { … }` wrapper, keeping the rules inside. */
export function flattenLayers(css) {
  let out = "";
  let i = 0;
  while (i < css.length) {
    const at = css.indexOf("@layer", i);
    if (at === -1) {
      out += css.slice(i);
      break;
    }
    out += css.slice(i, at);

    // Prelude runs to the first `{` (a block) or `;` (a bare order statement).
    let j = at + "@layer".length;
    while (j < css.length && css[j] !== "{" && css[j] !== ";") j++;

    if (j >= css.length) {
      // Malformed tail; keep it rather than silently dropping rules.
      out += css.slice(at);
      break;
    }
    if (css[j] === ";") {
      // `@layer a, b, c;` — the order statement is meaningless once flattened.
      i = j + 1;
      continue;
    }

    const end = closingBrace(css, j);
    out += flattenLayers(css.slice(j + 1, end));
    i = end + 1;
  }
  return out;
}

async function main() {
  if (process.env.FLATTEN_CSS_LAYERS !== "1") {
    console.log("flatten-css-layers: skipped (FLATTEN_CSS_LAYERS is not 1)");
    return;
  }

  const names = (await readdir(OUT_DIR)).filter((n) => n.endsWith(".css"));
  if (names.length === 0) throw new Error(`no CSS found in ${OUT_DIR}`);

  for (const name of names) {
    const file = join(OUT_DIR, name);
    const before = await stat(file);
    const css = await readFile(file, "utf8");
    const flat = flattenLayers(css);
    if (flat.includes("@layer")) {
      throw new Error(`${name} still contains @layer after flattening`);
    }
    await writeFile(file, flat, "utf8");
    console.log(
      `flatten-css-layers: ${name} ${before.size} → ${Buffer.byteLength(flat)} bytes`,
    );
  }
}

// Only run when invoked directly; legacy-css.mjs imports flattenLayers.
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  await main();
}
