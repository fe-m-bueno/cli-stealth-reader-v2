// Creates a library database with the v1 implementation, for the v2
// compatibility test.
//
//   V1_DIR=~/Development/stealth-reader-v0 node tools/generate-storage-fixture.mjs
//
// Writes crates/reader-storage/tests/fixtures/v1-library.db (plus a JSON
// description of what it contains). The database is committed so the Rust suite
// can prove it opens a real v1 file, reads every table, and redacts the Toggl
// key that v1 deliberately left in place for this fixture.
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

const root = path.join(path.dirname(new URL(import.meta.url).pathname), "..");
const fixtureDir = path.join(root, "crates", "reader-storage", "tests", "fixtures");
fs.mkdirSync(fixtureDir, { recursive: true });

// v1 resolves its paths from the environment at import time.
const scratch = fs.mkdtempSync(path.join(os.tmpdir(), "v1-storage-fixture-"));
process.env.XDG_DATA_HOME = scratch;
process.env.XDG_CACHE_HOME = scratch;

const { Storage } = await import(pathToFileURL(path.join(distDir, "storage.js")).href);
const storage = new Storage();

const IMPORT_HASH = "0000000000000000000000000000000000000000000000000000000000000001";
const NOW = 1_700_000_000_000;

storage.saveBook(
  {
    id: "fixture-book",
    title: "Fixture Book",
    author: "Fixture Author",
    sourcePath: "/books/fixture.epub",
    importHash: IMPORT_HASH,
    parserVersion: 3,
    diagnostics: [
      { severity: "warning", message: "Navigation document is missing or unreadable. Falling back to spine order.", context: "navigation" },
      { severity: "error", message: "A recorded error", context: null }
    ],
    chapters: [
      {
        id: "fixture-ch1",
        index: 0,
        title: "Chapter One",
        href: "text/ch1.xhtml",
        depth: 0,
        wordCount: 7,
        blocks: [
          { id: "b0", type: "heading", text: "Chapter One", level: 1 },
          { id: "b1", type: "paragraph", text: "The lantern swung over the quiet harbour." }
        ]
      },
      {
        id: "fixture-ch2",
        index: 1,
        title: "Chapter Two",
        href: "text/ch2.xhtml",
        depth: 1,
        wordCount: 3,
        blocks: [
          { id: "b2", type: "image", text: "Map of the harbour", imageSource: "images/map.png" },
          { id: "b3", type: "scene-break", text: "Scene break" },
          { id: "b4", type: "list-item", text: "First item" },
          { id: "b5", type: "blockquote", text: "Remember the harbour." }
        ]
      }
    ]
  },
  "plain"
);

// A second book with no position, to exercise the null-progress path.
storage.saveBook(
  {
    id: "second-book",
    title: "Unstarted Book",
    author: "Another Author",
    sourcePath: "/books/second.epub",
    importHash: "0000000000000000000000000000000000000000000000000000000000000002",
    parserVersion: 1,
    diagnostics: [],
    chapters: [
      {
        id: "second-ch1",
        index: 0,
        title: "Only Chapter",
        href: "text/only.xhtml",
        depth: 0,
        wordCount: 2,
        blocks: [{ id: "c0", type: "paragraph", text: "Two words" }]
      }
    ]
  },
  "code"
);

storage.savePosition({
  bookId: "fixture-book",
  chapterIndex: 1,
  chapterProgress: 0.25,
  bookProgress: 0.6,
  blockOffset: 12
});
storage.addBookmark("fixture-book", 1, 12, "Halfway");
storage.addBookmark("fixture-book", 0, 0, null);
storage.addNote("fixture-book", "Remember this passage", 1, 12);
storage.addTag("fixture-book", "fiction");
storage.addTag("fixture-book", "favorite");
storage.saveReadingPace({
  bookId: "fixture-book",
  wpm: 242.5,
  activeMs: 300_000,
  updatedAt: NOW
});
storage.setRawSetting("globalWpm", "231.75");
storage.setRawSetting("globalActiveMs", "540000");
storage.setSetting("renderMode", "plain");
storage.setSetting("fontScale", 1.3);
storage.setSetting("lineSpacing", "relaxed");
storage.saveCommandHistory("/next 2", "next");

// Written straight into the table so it is *not* redacted on write: opening this
// database must redact it.
storage.db
  .prepare("INSERT INTO command_history (raw_command, normalized_name, created_at) VALUES (?, 'toggl', ?)")
  .run("/toggl auth toggl_sk_legacy_secret", NOW);

// The last-opened timestamps must be stable for the export comparison.
storage.db.prepare("UPDATE books SET last_opened_at = ? WHERE id = 'fixture-book'").run(NOW);
storage.db.prepare("UPDATE books SET last_opened_at = ? WHERE id = 'second-book'").run(NOW - 60_000);
storage.db.prepare("UPDATE bookmarks SET created_at = ?").run(NOW - 1_000);
storage.db.prepare("UPDATE notes SET created_at = ?").run(NOW - 2_000);

const expected = {
  settings: storage.getSettings(),
  rawSettings: {
    globalWpm: storage.getSetting("globalWpm"),
    globalActiveMs: storage.getSetting("globalActiveMs")
  },
  books: storage.listBooks(),
  position: storage.getPosition("fixture-book"),
  bookmarks: storage.listBookmarks("fixture-book"),
  notes: storage.listNotes("fixture-book"),
  tags: storage.listTags("fixture-book"),
  readingPace: storage.getReadingPace("fixture-book"),
  book: storage.getBook("fixture-book"),
  needsReimport: {
    "fixture-book": storage.needsEpubReimport("fixture-book"),
    "second-book": storage.needsEpubReimport("second-book")
  },
  latestBookId: storage.getLatestBookId(),
  exportAll: storage.exportAll()
};

storage.db.close();

// WAL contents must be folded into the main file before it is copied.
const sourceDb = path.join(scratch, "cli-stealth-reader", "library.db");
const reopen = await import(pathToFileURL(path.join(distDir, "storage.js")).href);
const checkpointing = new reopen.Storage();
checkpointing.db.pragma("wal_checkpoint(TRUNCATE)");
checkpointing.db.close();

fs.copyFileSync(sourceDb, path.join(fixtureDir, "v1-library.db"));
fs.writeFileSync(
  path.join(fixtureDir, "v1-library.json"),
  `${JSON.stringify(expected, null, 1)}\n`
);
fs.rmSync(scratch, { recursive: true, force: true });
process.stdout.write(
  `v1 database written to ${path.relative(process.cwd(), path.join(fixtureDir, "v1-library.db"))}\n`
);
