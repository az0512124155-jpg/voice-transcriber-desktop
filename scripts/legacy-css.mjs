// Rewrites the built stylesheets into CSS that an old WebKit can actually
// apply, without changing how they look on a current engine.
//
// Tailwind 4 targets Safari 16.4+. On the WKWebView that macOS 10.15–12 ship,
// a handful of constructs are either unknown or unsupported, and the failures
// are not cosmetic:
//
//   :is() / :where()   Safari 14+        A rule whose selector uses one is
//                                        dropped whole. Tailwind emits
//                                        `:where(.space-y-4 > :not(:last-child))`
//                                        for every space-y utility and
//                                        `:is(:where(.group):hover …)` for every
//                                        group-hover, so vertical rhythm and
//                                        hover states disappear entirely.
//   @property          Safari 16.4+      Ignored, which leaves every --tw-*
//                                        variable undefined. `border-style:
//                                        var(--tw-border-style)` then resolves
//                                        to nothing, so no border is ever drawn.
//   inset              Safari 14.1+      Positioning shorthand.
//   backdrop-filter    needs -webkit-    on Safari.
//
// So: expand the functional pseudo-classes into plain selector lists, drop the
// @property registrations, spell out `inset`, prefix backdrop-filter, and give
// `--tw-border-style` a literal fallback. Plus an opaque document background on
// the main window, so a stylesheet that fails for any other reason shows a dark
// window instead of a transparent one.
//
// Runs as part of `npm run build`, on every platform: the output was diffed
// against the untouched stylesheet in both Chromium and WebKit and only
// antialiasing moved (0.08% of pixels), so there is nothing to gain from
// keeping two code paths.
import { readdir, readFile, writeFile } from "node:fs/promises";
import { flattenLayers } from "./flatten-css-layers.mjs";
import { join } from "node:path";

const OUT_DIR = process.argv[2] ?? "dist/assets";
const INDEX_HTML = process.argv[3] ?? "dist/index.html";

/** Splits on `sep` at nesting depth 0 (parentheses and brackets respected). */
function splitTopLevel(text, sep = ",") {
  const parts = [];
  let depth = 0;
  let start = 0;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (c === "(" || c === "[") depth++;
    else if (c === ")" || c === "]") depth--;
    else if (c === sep && depth === 0) {
      parts.push(text.slice(start, i));
      start = i + 1;
    }
  }
  parts.push(text.slice(start));
  return parts;
}

/** Index of the `)` that closes the `(` at `open`, or -1. */
function closingParen(text, open) {
  let depth = 0;
  for (let i = open; i < text.length; i++) {
    if (text[i] === "(") depth++;
    else if (text[i] === ")") {
      depth--;
      if (depth === 0) return i;
    }
  }
  return -1;
}

/**
 * Expands every `:is(…)` / `:where(…)` in one selector into plain selectors.
 * A comma list inside the pseudo becomes several selectors, so
 * `:where(.a, .b) > .c` yields `.a > .c` and `.b > .c`.
 */
export function expandSelector(selector) {
  const lower = selector.toLowerCase();
  let at = -1;
  for (const name of [":is(", ":where("]) {
    const i = lower.indexOf(name);
    if (i !== -1 && (at === -1 || i < at)) at = i;
  }
  if (at === -1) return [selector];

  const open = selector.indexOf("(", at);
  const close = closingParen(selector, open);
  if (close === -1) return [selector];

  const prefix = selector.slice(0, at);
  const inner = selector.slice(open + 1, close);
  const suffix = selector.slice(close + 1);

  const out = [];
  for (const alternative of splitTopLevel(inner)) {
    const replaced = prefix + alternative.trim() + suffix;
    for (const expanded of expandSelector(replaced)) out.push(expanded);
  }
  return out;
}

/** Rewrites one selector list, de-duplicated and order-preserving. */
function expandSelectorList(list) {
  const seen = new Set();
  const out = [];
  for (const one of splitTopLevel(list)) {
    for (const expanded of expandSelector(one.trim())) {
      if (!seen.has(expanded)) {
        seen.add(expanded);
        out.push(expanded);
      }
    }
  }
  return out.join(",");
}

const AT_RULES_WITHOUT_RULES = new Set([
  "@font-face",
  "@keyframes",
  "@-webkit-keyframes",
  "@counter-style",
  "@font-feature-values",
  "@page",
]);

const DROPPED_AT_RULES = new Set(["@property"]);

/**
 * Walks the stylesheet, rewriting selectors and dropping @property blocks.
 * `@keyframes` bodies are copied verbatim — their `0% { … }` children are not
 * selectors and must not be expanded.
 */
function rewriteRules(css) {
  let out = "";
  let i = 0;
  while (i < css.length) {
    const brace = css.indexOf("{", i);
    if (brace === -1) {
      out += css.slice(i);
      break;
    }
    const semi = css.indexOf(";", i);
    const nextClose = css.indexOf("}", i);

    // A stray `;` or `}` before the next block: copy through it and move on.
    // Without this the loop can stall on the tail of a block.
    const stray = Math.min(
      semi === -1 ? Infinity : semi,
      nextClose === -1 ? Infinity : nextClose,
    );
    if (stray < brace) {
      out += css.slice(i, stray + 1);
      i = stray + 1;
      continue;
    }

    const prelude = css.slice(i, brace).trim();
    let depth = 0;
    let end = brace;
    for (; end < css.length; end++) {
      if (css[end] === "{") depth++;
      else if (css[end] === "}") {
        depth--;
        if (depth === 0) break;
      }
    }
    const body = css.slice(brace + 1, end);
    i = end + 1;

    if (prelude.startsWith("@")) {
      const name = prelude.split(/[\s({]/)[0].toLowerCase();
      if (DROPPED_AT_RULES.has(name)) continue;
      if (AT_RULES_WITHOUT_RULES.has(name)) {
        out += `${prelude}{${body}}`;
        continue;
      }
      out += `${prelude}{${rewriteRules(body)}}`;
      continue;
    }

    out += `${expandSelectorList(prelude)}{${body}}`;
  }
  return out;
}

/** Old-Safari-safe shorthands, applied to declarations only. */
function fixDeclarations(css) {
  return (
    css
      // @property is unknown there, so --tw-border-style is never initialised
      // and every `border` utility silently draws nothing.
      .replace(/var\(--tw-border-style\)/g, "solid")
      // `inset: 0` → the longhands Safari 14.1 understands.
      .replace(/(^|[;{])inset:([^;}]+)/g, (_m, lead, value) => {
        const [top, right = top, bottom = top, left = right] = value
          .split(/\s+/)
          .filter(Boolean);
        return `${lead}top:${top};right:${right};bottom:${bottom};left:${left}`;
      })
      .replace(/(^|[;{])-webkit-backdrop-filter:/g, "$1-webkit-backdrop-filter:")
      .replace(/(^|[;{])backdrop-filter:([^;}]+)/g, "$1-webkit-backdrop-filter:$2;backdrop-filter:$2")
  );
}

/** The stylesheet with every @supports block cut out. */
function stripSupportsBlocks(css) {
  let out = "";
  let i = 0;
  for (;;) {
    const at = css.indexOf("@supports", i);
    if (at < 0) {
      out += css.slice(i);
      break;
    }
    out += css.slice(i, at);
    const open = css.indexOf("{", at);
    if (open < 0) break;
    let depth = 0;
    let j = open;
    for (; j < css.length; j += 1) {
      if (css[j] === "{") depth += 1;
      else if (css[j] === "}") {
        depth -= 1;
        if (depth === 0) break;
      }
    }
    i = j + 1;
  }
  return out;
}

/** Occurrences of `needle` that no @supports guard is protecting. */
function findOutsideSupports(css, needle) {
  const bare = stripSupportsBlocks(css);
  const hits = [];
  let i = 0;
  for (;;) {
    const at = bare.indexOf(needle, i);
    if (at < 0) break;
    hits.push(
      bare
        .slice(Math.max(0, at - 60), at + needle.length + 40)
        .replace(/\s+/g, " "),
    );
    i = at + needle.length;
  }
  return hits;
}

async function main() {
  // Only the main window gets the opaque fallback; the recording overlay is a
  // transparent panel and must stay one.
  let mainSheets = [];
  try {
    const html = await readFile(INDEX_HTML, "utf8");
    mainSheets = [...html.matchAll(/href="[^"]*?([^/"]+\.css)"/g)].map((m) => m[1]);
  } catch {
    console.warn("legacy-css: no index.html, the fallback background is skipped");
  }

  const names = (await readdir(OUT_DIR)).filter((n) => n.endsWith(".css"));
  if (names.length === 0) throw new Error(`no CSS found in ${OUT_DIR}`);

  for (const name of names) {
    const file = join(OUT_DIR, name);
    const css = await readFile(file, "utf8");
    let next = fixDeclarations(rewriteRules(flattenLayers(css)));

    if (mainSheets.includes(name)) {
      // Last-resort paint: if the theme failed to apply, the window is dark
      // rather than showing the desktop through it. `:root` overrides this.
      next = `html{background-color:#08090d;color:#f3f4f9}${next}`;
    }

    for (const banned of [":is(", ":where(", "@property", "@layer"]) {
      if (next.includes(banned)) {
        throw new Error(`${name} still contains ${banned} after the legacy pass`);
      }
    }

    // color-mix() is allowed only inside a feature query, where the plain
    // value above it is what an older engine paints. Anywhere else it is a
    // declaration that engine throws away — and when it sits in a custom
    // property it takes every var() that reads it down with it, which is how
    // the dictation panel once turned transparent.
    const stray = findOutsideSupports(next, "color-mix(");
    if (stray.length > 0) {
      throw new Error(
        `${name} uses color-mix() outside @supports near: ${stray[0]}`,
      );
    }

    await writeFile(file, next, "utf8");
    console.log(
      `legacy-css: ${name} ${Buffer.byteLength(css)} → ${Buffer.byteLength(next)} bytes`,
    );
  }
}

await main();
