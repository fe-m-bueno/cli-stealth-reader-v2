// Measures the v1 (TypeScript/Node) performance baseline for issue #32.
//
// Usage:
//   V1_DIR=~/Development/cli-stealth-reader node bench/v1-baseline.mjs [--json]
//
// Requires the v1 build output (`npm run build` in V1_DIR). Every measurement is
// repeated and reported as median plus min/max so a rerun is comparable.
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { pathToFileURL } from "node:url";
import { ensureFixtures } from "./fixtures.mjs";

const v1Dir = path.resolve(process.env.V1_DIR ?? path.join(os.homedir(), "Development", "cli-stealth-reader"));
const distDir = path.join(v1Dir, "dist");
if (!fs.existsSync(distDir)) {
  throw new Error(`Missing ${distDir}. Run "npm run build" in the v1 checkout first.`);
}

const fixtureDir = process.env.BENCH_FIXTURE_DIR
  ?? path.join(os.tmpdir(), "stealth-reader-bench-fixtures");
const fixtures = await ensureFixtures(fixtureDir);

const STARTUP_RUNS = Number(process.env.BENCH_STARTUP_RUNS ?? 10);
const IMPORT_RUNS = Number(process.env.BENCH_IMPORT_RUNS ?? 5);
const RENDER_RUNS = Number(process.env.BENCH_RENDER_RUNS ?? 20);

function stats(samples) {
  const sorted = [...samples].sort((left, right) => left - right);
  const middle = Math.floor(sorted.length / 2);
  const median = sorted.length % 2 === 0
    ? (sorted[middle - 1] + sorted[middle]) / 2
    : sorted[middle];
  return {
    runs: sorted.length,
    median: Number(median.toFixed(2)),
    min: Number(sorted[0].toFixed(2)),
    max: Number(sorted[sorted.length - 1].toFixed(2))
  };
}

function moduleUrl(name) {
  return pathToFileURL(path.join(distDir, name)).href;
}

async function timeAsync(runs, task) {
  const samples = [];
  for (let index = 0; index < runs; index += 1) {
    const start = performance.now();
    await task(index);
    samples.push(performance.now() - start);
  }
  return stats(samples);
}

// ── startup: a cold Node process that loads the whole TUI module graph ────────
function measureColdStartup() {
  const script = `
    const start = performance.now();
    await import(${JSON.stringify(moduleUrl("tui.js"))});
    process.stdout.write(String(performance.now() - start));
  `;
  const wall = [];
  const inProcess = [];
  const dataDir = fs.mkdtempSync(path.join(os.tmpdir(), "stealth-bench-xdg-"));
  for (let index = 0; index < STARTUP_RUNS; index += 1) {
    const start = performance.now();
    const result = spawnSync(process.execPath, ["--input-type=module", "-e", script], {
      encoding: "utf8",
      env: { ...process.env, XDG_DATA_HOME: dataDir, XDG_CACHE_HOME: dataDir }
    });
    const elapsed = performance.now() - start;
    if (result.status !== 0) {
      throw new Error(`startup probe failed: ${result.stderr}`);
    }
    wall.push(elapsed);
    inProcess.push(Number(result.stdout));
  }
  fs.rmSync(dataDir, { recursive: true, force: true });
  return { processWallMs: stats(wall), moduleGraphMs: stats(inProcess) };
}

// ── storage open + library discovery: the rest of a real startup ──────────────
async function measureStorageAndDiscovery() {
  const dataDir = fs.mkdtempSync(path.join(os.tmpdir(), "stealth-bench-store-"));
  process.env.XDG_DATA_HOME = dataDir;
  process.env.XDG_CACHE_HOME = dataDir;
  const { Storage } = await import(moduleUrl("storage.js"));
  const { discoverBooks } = await import(moduleUrl("discovery.js"));

  const openSamples = [];
  for (let index = 0; index < STARTUP_RUNS; index += 1) {
    const start = performance.now();
    const storage = new Storage();
    openSamples.push(performance.now() - start);
    storage.db.close();
  }
  const discovery = await timeAsync(STARTUP_RUNS, () => discoverBooks(fixtureDir));
  fs.rmSync(dataDir, { recursive: true, force: true });
  return { storageOpenMs: stats(openSamples), discoveryMs: discovery };
}

// ── import: parse each fixture into the canonical model ──────────────────────
async function measureImports() {
  const { importFile } = await import(moduleUrl("parser/index.js"));
  const results = {};
  let largeBook = null;
  for (const [name, file] of Object.entries(fixtures)) {
    const measured = await timeAsync(IMPORT_RUNS, async () => {
      const book = await importFile(file);
      if (name === "largeEpub") {
        largeBook = book;
      }
    });
    results[name] = {
      ...measured,
      fileBytes: fs.statSync(file).size
    };
  }
  return { results, largeBook };
}

// ── render: full-chapter line generation in both modes ────────────────────────
async function measureRender(book) {
  const { renderBlocks } = await import(moduleUrl("renderers.js"));
  const { DEFAULT_THEME } = await import(moduleUrl("themes.js"));
  const theme = DEFAULT_THEME;
  const chapter = book.chapters.reduce(
    (widest, candidate) => (candidate.blocks.length > widest.blocks.length ? candidate : widest),
    book.chapters[0]
  );
  const width = 100;

  const plain = await timeAsync(RENDER_RUNS, () => {
    renderBlocks(chapter.blocks, "plain", width, theme, "typescript", 3, null, true);
  });
  const code = await timeAsync(RENDER_RUNS, () => {
    renderBlocks(chapter.blocks, "code", width, theme, "typescript", 3, null, true);
  });
  const wholeBookPlain = await timeAsync(Math.max(3, Math.floor(RENDER_RUNS / 4)), () => {
    for (const item of book.chapters) {
      renderBlocks(item.blocks, "plain", width, theme, "typescript", 3, null, true);
    }
  });
  return {
    chapterBlocks: chapter.blocks.length,
    plainChapterMs: plain,
    codeChapterMs: code,
    wholeBookPlainMs: wholeBookPlain
  };
}

// ── memory: peak RSS of a process that imports the large book and renders it ──
function measurePeakMemory() {
  const script = `
    const { importFile } = await import(${JSON.stringify(moduleUrl("parser/index.js"))});
    const { renderBlocks } = await import(${JSON.stringify(moduleUrl("renderers.js"))});
    const { DEFAULT_THEME } = await import(${JSON.stringify(moduleUrl("themes.js"))});
    const book = await importFile(${JSON.stringify(fixtures.largeEpub)});
    for (const chapter of book.chapters) {
      renderBlocks(chapter.blocks, "code", 100, DEFAULT_THEME, "typescript", 3, null, true);
    }
    process.stdout.write(JSON.stringify(process.memoryUsage()));
  `;
  const result = spawnSync(process.execPath, ["--input-type=module", "-e", script], {
    encoding: "utf8"
  });
  if (result.status !== 0) {
    throw new Error(`memory probe failed: ${result.stderr}`);
  }
  const usage = JSON.parse(result.stdout);
  return {
    rssMb: Number((usage.rss / 1024 / 1024).toFixed(2)),
    heapUsedMb: Number((usage.heapUsed / 1024 / 1024).toFixed(2))
  };
}

const startup = measureColdStartup();
const storage = await measureStorageAndDiscovery();
const { results: imports, largeBook } = await measureImports();
const render = await measureRender(largeBook);
const memory = measurePeakMemory();

const report = {
  runtime: "v1-typescript-node",
  nodeVersion: process.version,
  platform: `${os.platform()} ${os.release()} ${os.arch()}`,
  cpu: os.cpus()[0]?.model ?? "unknown",
  fixtureDir,
  corpus: {
    largeEpubChapters: largeBook.chapters.length,
    largeEpubWords: largeBook.chapters.reduce((total, chapter) => total + chapter.wordCount, 0)
  },
  startup,
  ...storage,
  imports,
  render,
  memory
};

if (process.argv.includes("--json")) {
  process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
} else {
  console.dir(report, { depth: null });
}
