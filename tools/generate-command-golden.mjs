// Generates the command parity fixture from the v1 implementation.
//
//   V1_DIR=~/Development/stealth-reader-v0 node tools/generate-command-golden.mjs
//
// Output: crates/reader-core/tests/golden/command-parity.json — parse results
// (or error messages), contextual hints, manual text at several widths, and
// suggestion lists with their completion ranges. Toggl cases run without a
// storage handle, so no database or network is involved.
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

const {
  COMMANDS,
  parseSlashCommand,
  commandContextHelp,
  commandHelp,
  listCommandSuggestions,
  applyCommandAutocomplete,
  commandAutocompleteIndex
} = await load("commands.js");

const PARSE_INPUTS = [
  "/next",
  "/next 3",
  "  /next   3  ",
  "/prev 2",
  "/book dune",
  "/keys",
  "/config",
  "/bookdir",
  "/chapters introduction --current --flat",
  "/changebook --sort progress",
  "/changebook --sort=title dune",
  "/changebook --sort",
  "/changebook --sort --recent",
  "/changebook --sort=",
  "/search -g mordor",
  '/search "chapter one"',
  "/search --global \"needle in a haystack\"",
  "/note -ld",
  "/note -l",
  "/note remember this passage",
  "/note 'single quoted note'",
  "/tag -d favorite",
  "/mark",
  "/mark -",
  "/mark \"return here\"",
  "/goto 30%c",
  "/goto 30% --chapter",
  "/toggl start \"O Nome do Vento\" --project \"Reading books\"",
  "/toggl log Choujin --duration 45m",
  "/toggl auth --open",
  "/removecurrent --confirm",
  "/highlight on",
  "/mode plain",
  "next",
  "",
  "   ",
  "/nope",
  "/next --sideways",
  "/search -z term",
  "/note -q",
  "/keyboardshortcuts --category navigation",
  "/keyboardshortcuts --category",
  "/help mode",
  "/help --all"
];

const CONTEXT_INPUTS = [
  "",
  "   ",
  "/",
  "/mode",
  "mode plain",
  "/book dune",
  "book",
  "/nope",
  "/toggl",
  "/toggl ",
  "/toggl auth",
  "/toggl AUTH",
  "/toggl setup",
  "/toggl start",
  "/toggl log",
  "/toggl sync",
  "/toggl recent",
  "/toggl stop",
  "/toggl frobnicate",
  "/search",
  "/goto",
  "/settings"
];

const SUGGESTION_INPUTS = [
  ["", undefined],
  ["m", undefined],
  ["mar", undefined],
  ["mark", undefined],
  ["marks", undefined],
  ["boo", undefined],
  ["book", undefined],
  ["key", undefined],
  ["conf", undefined],
  ["zzz", undefined],
  ["chapters --", undefined],
  ["chapters --c", undefined],
  ["chapters --current --", undefined],
  ["changebook --s", undefined],
  ["changebook --sort=title --", undefined],
  ["note --", undefined],
  ["mark --", undefined],
  ["toggl", undefined],
  ["toggl ", undefined],
  ["toggl s", undefined],
  ["toggl st", undefined],
  ["toggl start", undefined],
  ["toggl start ", undefined],
  ["toggl auth --", undefined],
  ["mar important", 3],
  ["chapters --current --flat", 11],
  ["prev 2", 6]
];

const cases = [];

for (const input of PARSE_INPUTS) {
  let result;
  try {
    const parsed = parseSlashCommand(input);
    result = { ok: true, name: parsed.name, args: parsed.args, flags: parsed.flags };
  } catch (error) {
    result = { ok: false, message: error.message };
  }
  cases.push({ kind: "parse", input, result });
}

for (const input of CONTEXT_INPUTS) {
  cases.push({ kind: "context", input, lines: commandContextHelp(input) });
}

for (const name of [undefined, ...COMMANDS.map((command) => command.name), "book", "keys", "nope"]) {
  for (const width of [0, 40, 60, 80, 120]) {
    cases.push({
      kind: "help",
      command: name ?? null,
      width,
      lines: commandHelp(name, width || undefined)
    });
  }
}

for (const [buffer, cursor] of SUGGESTION_INPUTS) {
  const position = cursor ?? buffer.length;
  const suggestions = listCommandSuggestions(buffer, undefined, position);
  cases.push({
    kind: "suggest",
    buffer,
    cursor: position,
    suggestions: suggestions.map((item) => ({
      name: item.name,
      usage: item.usage,
      description: item.description,
      category: item.category,
      detail: item.detail,
      matchedAlias: item.matchedAlias ?? null,
      completion: item.completion ?? null,
      completionStart: item.completionStart ?? null,
      completionEnd: item.completionEnd ?? null,
      applied: applyCommandAutocomplete(buffer, item)
    })),
    nextIndex: [0, 1, 5].map((index) => commandAutocompleteIndex(buffer, index, suggestions))
  });
}

const outputPath = path.join(
  path.dirname(new URL(import.meta.url).pathname),
  "..",
  "crates",
  "reader-core",
  "tests",
  "golden",
  "command-parity.json"
);
fs.mkdirSync(path.dirname(outputPath), { recursive: true });
const body = cases.map((entry) => `  ${JSON.stringify(entry)}`).join(",\n");
fs.writeFileSync(
  outputPath,
  `{\n "source": "cli-stealth-reader v1",\n "cases": [\n${body}\n ]\n}\n`
);
process.stdout.write(`${cases.length} cases written to ${path.relative(process.cwd(), outputPath)}\n`);
