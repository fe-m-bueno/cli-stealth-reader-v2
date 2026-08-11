// Generates the render parity fixture from the v1 implementation.
//
//   V1_DIR=~/Development/stealth-reader-v0 node tools/generate-render-golden.mjs
//
// Output: crates/reader-core/tests/golden/render-parity.json — the stripped text
// of every rendered line for a matrix of blocks, modes, languages, densities,
// and spacings. The Rust suite asserts against it, so regenerating it is an
// explicit act that shows up in review.
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";

const v1Dir = path.resolve(
  process.env.V1_DIR ?? path.join(os.homedir(), "Development", "stealth-reader-v0")
);
const distDir = path.join(v1Dir, "dist");
if (!fs.existsSync(distDir)) {
  throw new Error(`Missing ${distDir}. Run "npm run build" in the v1 checkout first.`);
}
const load = (name) => import(pathToFileURL(path.join(distDir, name)).href);

const { renderBlocks } = await load("renderers.js");
const { stripAnsi } = await load("screen.js");
const { DEFAULT_THEME } = await load("themes.js");

const BLOCKS = {
  prose: {
    id: "prose",
    type: "paragraph",
    text: 'She said "hello" before crossing the narrow bridge under C:\\moonlight'
  },
  longProse: {
    id: "long",
    type: "paragraph",
    text: "The lantern swung once over the quiet harbour, and the whole sandstone town leaned in to listen while the tide argued with the breakwater about who had arrived first."
  },
  shortProse: { id: "short", type: "paragraph", text: "Dawn." },
  emptyProse: { id: "empty", type: "paragraph", text: "" },
  dialogue: {
    id: "dialogue",
    type: "paragraph",
    text: "— Come in, she said. “The night is «cold» and he didn't answer.”"
  },
  heading: { id: "heading", type: "heading", text: "A quiet chapter", level: 2 },
  sceneBreak: { id: "break", type: "scene-break", text: "" },
  image: { id: "image", type: "image", text: "Map of Arrakis", imageSource: "images/1.jpg" },
  bareImage: { id: "bare-image", type: "image", text: "" },
  listItem: { id: "list", type: "list-item", text: "first item of the list" },
  blockquote: {
    id: "quote",
    type: "blockquote",
    text: "Remember the harbour, and remember the cold that came after it."
  },
  anchor: { id: "anchor", type: "anchor", text: "anchor text", anchorId: "top" }
};

// Block indices that reach every structural branch, plus a plain sweep.
const BLOCK_INDICES = [
  ...Array.from({ length: 50 }, (_, index) => index),
  137, 169, 209, 247, 299, 377, 403, 437, 481, 529, 611, 703, 799, 851, 899, 961
];
const WIDTHS = [40, 80, 120];
const LANGUAGES = ["typescript", "python", "rust"];
const DENSITIES = [1, 2, 3, 4, 5];
const SPACINGS = ["compact", "normal", "relaxed"];

const cases = [];

function record(name, lines) {
  cases.push({ name, lines: lines.map(stripAnsi) });
}

// Code mode: every language at every density, over the structural sweep.
for (const language of LANGUAGES) {
  for (const density of DENSITIES) {
    for (const [blockName, block] of Object.entries(BLOCKS)) {
      for (const blockIndex of BLOCK_INDICES) {
        record(
          `code/${language}/d${density}/${blockName}/${blockIndex}`,
          renderBlocks([block], "code", 80, DEFAULT_THEME, language, density, undefined, true, blockIndex, false)
        );
      }
    }
  }
}

// Width sensitivity, one density per language to keep the fixture focused.
for (const language of LANGUAGES) {
  for (const width of WIDTHS) {
    for (const blockIndex of [0, 1, 13, 19, 41, 43, 47, 137]) {
      record(
        `code-width/${language}/${width}/${blockIndex}`,
        renderBlocks([BLOCKS.longProse], "code", width, DEFAULT_THEME, language, 3, undefined, true, blockIndex, false)
      );
    }
  }
}

// Plain mode: highlighting on and off, every width.
for (const highlight of [true, false]) {
  for (const width of WIDTHS) {
    for (const [blockName, block] of Object.entries(BLOCKS)) {
      record(
        `plain/${highlight ? "highlight" : "flat"}/${width}/${blockName}`,
        renderBlocks([block], "plain", width, DEFAULT_THEME, "typescript", 3, undefined, highlight, 0, false)
      );
    }
  }
}

// Multi-block spacing in both modes, including trailing spacing.
const runOfBlocks = [
  BLOCKS.heading,
  BLOCKS.prose,
  BLOCKS.blockquote,
  BLOCKS.listItem,
  BLOCKS.sceneBreak,
  BLOCKS.longProse,
  BLOCKS.image
];
for (const spacing of SPACINGS) {
  for (const trailing of [true, false]) {
    record(
      `spacing/plain/${spacing}/${trailing}`,
      renderBlocks(runOfBlocks, "plain", 80, DEFAULT_THEME, "typescript", 3, undefined, true, 0, trailing, spacing)
    );
    for (const language of LANGUAGES) {
      record(
        `spacing/code-${language}/${spacing}/${trailing}`,
        renderBlocks(runOfBlocks, "code", 80, DEFAULT_THEME, language, 3, undefined, true, 3, trailing, spacing)
      );
    }
  }
}

// Search highlighting must not change the stripped text.
for (const mode of ["plain", "code"]) {
  record(
    `search/${mode}`,
    renderBlocks([BLOCKS.longProse], mode, 80, DEFAULT_THEME, "typescript", 3, "harbour", true, 5, false)
  );
}

const outputPath = path.join(
  path.dirname(new URL(import.meta.url).pathname),
  "..",
  "crates",
  "reader-core",
  "tests",
  "golden",
  "render-parity.json"
);
fs.mkdirSync(path.dirname(outputPath), { recursive: true });
// One line per case keeps the file diffable without the size of full pretty-printing.
const body = cases.map((entry) => `  ${JSON.stringify(entry)}`).join(",\n");
fs.writeFileSync(
  outputPath,
  `{\n "source": "cli-stealth-reader v1",\n "theme": "codex/dark",\n "cases": [\n${body}\n ]\n}\n`
);
process.stdout.write(`${cases.length} cases written to ${path.relative(process.cwd(), outputPath)}\n`);
