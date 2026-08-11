// Builds the shared import fixtures and records what v1 extracts from them.
//
//   V1_DIR=~/Development/stealth-reader-v0 node tools/generate-import-golden.mjs
//
// Writes crates/reader-formats/tests/fixtures/*.{epub,cbz} and
// crates/reader-formats/tests/golden/import-parity.json. The fixtures are
// committed so the Rust suite runs without Node; the JSON records the canonical
// book v1 produced, minus the two fields that depend on the absolute path
// (`id` and `sourcePath`, both covered by unit tests instead).
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";

const v1Dir = path.resolve(
  process.env.V1_DIR ?? path.join(os.homedir(), "Development", "stealth-reader-v0")
);
const distDir = path.join(v1Dir, "dist");
if (!fs.existsSync(distDir)) {
  throw new Error(`Missing ${distDir}. Run "npm run build" in the v1 checkout first.`);
}
const require = createRequire(path.join(v1Dir, "package.json"));
const JSZip = require("jszip");
const { importFile } = await import(pathToFileURL(path.join(distDir, "parser/index.js")).href);

const root = path.join(path.dirname(new URL(import.meta.url).pathname), "..");
const fixtureDir = path.join(root, "crates", "reader-formats", "tests", "fixtures");
const goldenDir = path.join(root, "crates", "reader-formats", "tests", "golden");
fs.mkdirSync(fixtureDir, { recursive: true });
fs.mkdirSync(goldenDir, { recursive: true });

const CONTAINER = `<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>`;

function opf({ title = "Fixture Book", author = "Fixture Author", manifest, spine, tocAttr = "" }) {
  return `<?xml version="1.0"?>
<package version="3.0" xmlns="http://www.idpf.org/2007/opf">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>${title}</dc:title>
    <dc:creator>${author}</dc:creator>
  </metadata>
  <manifest>${manifest}</manifest>
  <spine${tocAttr}>${spine}</spine>
</package>`;
}

/** Build one EPUB fixture from a description of its entries. */
async function writeEpub(name, entries, { mimetype = "application/epub+zip" } = {}) {
  const zip = new JSZip();
  if (mimetype !== null) {
    zip.file("mimetype", mimetype, { compression: "STORE" });
  }
  zip.file("META-INF/container.xml", CONTAINER);
  for (const [entryPath, contents] of Object.entries(entries)) {
    zip.file(entryPath, contents);
  }
  const buffer = await zip.generateAsync({ type: "nodebuffer" });
  const filePath = path.join(fixtureDir, name);
  fs.writeFileSync(filePath, buffer);
  return filePath;
}

const fixtures = [];

// 1. EPUB3 nav with two chapters sharing one file through fragments.
fixtures.push(
  await writeEpub("nav-fragments.epub", {
    "OEBPS/nav.xhtml": `<!doctype html><html xmlns:epub="http://www.idpf.org/2007/ops"><body>
      <nav epub:type="toc"><ol>
        <li><a href="text/ch1.xhtml#start">Chapter One</a></li>
        <li><a href="text/ch1.xhtml#middle">Chapter Two</a></li>
      </ol></nav></body></html>`,
    "OEBPS/text/ch1.xhtml": `<!doctype html><html><body>
      <h1 id="start">One</h1>
      <p>First chapter begins here and runs long enough to count as body text for sure.</p>
      <h2 id="middle">Two</h2>
      <p>Second chapter begins here and also runs long enough to count as body text.</p>
    </body></html>`,
    "OEBPS/content.opf": opf({
      manifest: `<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
        <item id="chapter" href="text/ch1.xhtml" media-type="application/xhtml+xml"/>`,
      spine: `<itemref idref="chapter"/>`
    })
  })
);

// 2. NCX navigation, nested, with no EPUB3 nav document.
fixtures.push(
  await writeEpub("ncx-nested.epub", {
    "OEBPS/toc.ncx": `<?xml version="1.0"?>
      <ncx xmlns="http://www.daisy.org/z3986/2005/ncx/"><navMap>
        <navPoint id="p1" playOrder="1">
          <navLabel><text>Chapter One</text></navLabel>
          <content src="text/ch1.xhtml"/>
          <navPoint id="p1a" playOrder="2">
            <navLabel><text>Section A</text></navLabel>
            <content src="text/ch2.xhtml"/>
          </navPoint>
        </navPoint>
      </navMap></ncx>`,
    "OEBPS/text/ch1.xhtml": `<!doctype html><html><body><h1>One</h1>
      <p>The lantern swung once over the quiet harbour and the whole town leaned in to listen.</p>
      <blockquote>Remember the harbour.</blockquote>
      <ul><li>First item</li><li>Second item</li></ul>
      <hr/>
      <p>After the break the tide argued with the breakwater about who arrived first.</p>
      </body></html>`,
    "OEBPS/text/ch2.xhtml": `<!doctype html><html><body><h1>Two</h1>
      <p>Sandstone streets held the heat long after the sun had gone down behind the ridge.</p>
      <img src="../images/map.png" alt="Map of the harbour"/>
      </body></html>`,
    "OEBPS/content.opf": opf({
      manifest: `<item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
        <item id="ch1" href="text/ch1.xhtml" media-type="application/xhtml+xml"/>
        <item id="ch2" href="text/ch2.xhtml" media-type="application/xhtml+xml"/>`,
      spine: `<itemref idref="ch1"/><itemref idref="ch2"/>`,
      tocAttr: ` toc="ncx"`
    })
  })
);

// 3. Neither nav nor NCX: spine order with a missing-navigation diagnostic.
fixtures.push(
  await writeEpub("spine-fallback.epub", {
    "OEBPS/text/a.xhtml": `<!doctype html><html><body><p>First file text that is long enough to be body content here.</p></body></html>`,
    "OEBPS/text/b.xhtml": `<!doctype html><html><body><p>Second file text that is long enough to be body content here.</p></body></html>`,
    "OEBPS/content.opf": opf({
      manifest: `<item id="a" href="text/a.xhtml" media-type="application/xhtml+xml"/>
        <item id="b" href="text/b.xhtml" media-type="application/xhtml+xml"/>`,
      spine: `<itemref idref="a"/><itemref idref="b"/>`
    })
  })
);

// 4. Front matter that must be dropped, plus a real chapter.
fixtures.push(
  await writeEpub("front-matter.epub", {
    "OEBPS/nav.xhtml": `<!doctype html><html xmlns:epub="http://www.idpf.org/2007/ops"><body>
      <nav epub:type="toc"><ol>
        <li><a href="text/cover.xhtml">Capa</a></li>
        <li><a href="text/toc.xhtml">Sumário</a></li>
        <li><a href="text/rights.xhtml">Copyright</a></li>
        <li><a href="text/ch1.xhtml">Capítulo 1</a></li>
      </ol></nav></body></html>`,
    "OEBPS/text/cover.xhtml": `<!doctype html><html><body><p>Cover page text goes here for the fixture.</p></body></html>`,
    "OEBPS/text/toc.xhtml": `<!doctype html><html><body><p>Table of contents text goes here for the fixture.</p></body></html>`,
    "OEBPS/text/rights.xhtml": `<!doctype html><html><body><p>All rights reserved by the publisher of this fixture.</p></body></html>`,
    "OEBPS/text/ch1.xhtml": `<!doctype html><html><body><h1>Capítulo 1</h1>
      <p>O porto ficou quieto por tempo suficiente para que todos ouvissem a maré discutindo.</p>
      </body></html>`,
    "OEBPS/content.opf": opf({
      title: "Livro de Teste",
      author: "Autora de Teste",
      manifest: `<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
        <item id="cover" href="text/cover.xhtml" media-type="application/xhtml+xml"/>
        <item id="toc" href="text/toc.xhtml" media-type="application/xhtml+xml"/>
        <item id="rights" href="text/rights.xhtml" media-type="application/xhtml+xml"/>
        <item id="ch1" href="text/ch1.xhtml" media-type="application/xhtml+xml"/>`,
      spine: `<itemref idref="cover"/><itemref idref="toc"/><itemref idref="rights"/><itemref idref="ch1"/>`
    })
  })
);

// 5. Styled titles that must be promoted to headings, and a leading ornament image.
fixtures.push(
  await writeEpub("heading-promotion.epub", {
    "OEBPS/nav.xhtml": `<!doctype html><html xmlns:epub="http://www.idpf.org/2007/ops"><body>
      <nav epub:type="toc"><ol><li><a href="text/ch1.xhtml">Chapter One</a></li></ol></nav></body></html>`,
    "OEBPS/text/ch1.xhtml": `<!doctype html><html><body>
      <img src="../images/ornament.png" alt="An engraved ornament"/>
      <p>Chapter One</p>
      <p>The Harbour At Dawn</p>
      <p>The lantern swung once over the quiet harbour, and the whole sandstone town leaned in to listen while the tide argued.</p>
      </body></html>`,
    "OEBPS/content.opf": opf({
      manifest: `<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
        <item id="ch1" href="text/ch1.xhtml" media-type="application/xhtml+xml"/>`,
      spine: `<itemref idref="ch1"/>`
    })
  })
);

// 6. One table-of-contents entry standing for a run of spine files, where the
//    first file holds only a title image and the chapter needs a synthetic heading.
fixtures.push(
  await writeEpub("multi-file-chapter.epub", {
    "OEBPS/nav.xhtml": `<!doctype html><html xmlns:epub="http://www.idpf.org/2007/ops"><body>
      <nav epub:type="toc"><ol>
        <li><a href="text/part1-title.xhtml">Part One</a></li>
        <li><a href="text/part2-title.xhtml">Part Two</a></li>
      </ol></nav></body></html>`,
    "OEBPS/text/part1-title.xhtml": `<!doctype html><html><body><img src="../images/title1.png" alt="img1"/></body></html>`,
    "OEBPS/text/part1-a.xhtml": `<!doctype html><html><body><p>The first half of part one runs long enough to be body text here.</p></body></html>`,
    "OEBPS/text/part1-b.xhtml": `<!doctype html><html><body><p>The second half of part one also runs long enough to be body text.</p></body></html>`,
    "OEBPS/text/part2-title.xhtml": `<!doctype html><html><body><p>Part two opens with a paragraph that is long enough to be body text here.</p></body></html>`,
    "OEBPS/content.opf": opf({
      manifest: `<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
        <item id="p1t" href="text/part1-title.xhtml" media-type="application/xhtml+xml"/>
        <item id="p1a" href="text/part1-a.xhtml" media-type="application/xhtml+xml"/>
        <item id="p1b" href="text/part1-b.xhtml" media-type="application/xhtml+xml"/>
        <item id="p2t" href="text/part2-title.xhtml" media-type="application/xhtml+xml"/>`,
      spine: `<itemref idref="p1t"/><itemref idref="p1a"/><itemref idref="p1b"/><itemref idref="p2t"/>`
    })
  })
);

// 7. A missing mimetype entry, which is a warning rather than a failure.
fixtures.push(
  await writeEpub(
    "no-mimetype.epub",
    {
      "OEBPS/nav.xhtml": `<!doctype html><html xmlns:epub="http://www.idpf.org/2007/ops"><body>
        <nav epub:type="toc"><ol><li><a href="text/ch1.xhtml">Chapter One</a></li></ol></nav></body></html>`,
      "OEBPS/text/ch1.xhtml": `<!doctype html><html><body><p>Body text that is long enough to count as a real paragraph here.</p></body></html>`,
      "OEBPS/content.opf": opf({
        manifest: `<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
          <item id="ch1" href="text/ch1.xhtml" media-type="application/xhtml+xml"/>`,
        spine: `<itemref idref="ch1"/>`
      })
    },
    { mimetype: null }
  )
);

// 8. A chapter that resolves to nothing, which must be reported and dropped.
fixtures.push(
  await writeEpub("empty-chapter.epub", {
    "OEBPS/nav.xhtml": `<!doctype html><html xmlns:epub="http://www.idpf.org/2007/ops"><body>
      <nav epub:type="toc"><ol>
        <li><a href="text/blank.xhtml">Blank</a></li>
        <li><a href="text/ch1.xhtml">Chapter One</a></li>
      </ol></nav></body></html>`,
    "OEBPS/text/blank.xhtml": `<!doctype html><html><body><div></div></body></html>`,
    "OEBPS/text/ch1.xhtml": `<!doctype html><html><body><p>Body text that is long enough to count as a real paragraph here.</p></body></html>`,
    "OEBPS/content.opf": opf({
      manifest: `<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
        <item id="blank" href="text/blank.xhtml" media-type="application/xhtml+xml"/>
        <item id="ch1" href="text/ch1.xhtml" media-type="application/xhtml+xml"/>`,
      spine: `<itemref idref="blank"/><itemref idref="ch1"/>`
    })
  })
);

// 9. A CBZ with mixed entry names and a stray non-image file.
{
  const zip = new JSZip();
  for (const name of ["page-10.jpg", "page-2.JPG", "page-1.png", "cover/page-0.webp"]) {
    zip.file(name, Buffer.from(`fake-image-data-${name}`));
  }
  zip.file("ComicInfo.xml", "<ComicInfo><Series>Fixture</Series></ComicInfo>");
  const buffer = await zip.generateAsync({ type: "nodebuffer" });
  const filePath = path.join(fixtureDir, "comic.cbz");
  fs.writeFileSync(filePath, buffer);
  fixtures.push(filePath);
}

// 10. A CBZ with no images at all.
{
  const zip = new JSZip();
  zip.file("notes.txt", "no images here");
  const buffer = await zip.generateAsync({ type: "nodebuffer" });
  const filePath = path.join(fixtureDir, "empty.cbz");
  fs.writeFileSync(filePath, buffer);
  fixtures.push(filePath);
}

const cases = [];
for (const filePath of fixtures) {
  const name = path.basename(filePath);
  let result;
  try {
    const book = await importFile(filePath);
    // `id` and `sourcePath` depend on the absolute path, so they are excluded.
    result = {
      ok: true,
      title: book.title,
      author: book.author,
      importHash: book.importHash,
      parserVersion: book.parserVersion ?? null,
      diagnostics: book.diagnostics,
      chapters: book.chapters.map((chapter) => ({
        id: chapter.id,
        index: chapter.index,
        title: chapter.title,
        href: chapter.href,
        depth: chapter.depth,
        wordCount: chapter.wordCount,
        blocks: chapter.blocks
      }))
    };
  } catch (error) {
    result = { ok: false, message: error.message };
  }
  cases.push({ fixture: name, result });
}

const outputPath = path.join(goldenDir, "import-parity.json");
const body = cases.map((entry) => `  ${JSON.stringify(entry)}`).join(",\n");
fs.writeFileSync(
  outputPath,
  `{\n "source": "cli-stealth-reader v1",\n "cases": [\n${body}\n ]\n}\n`
);
process.stdout.write(
  `${cases.length} fixtures imported; golden written to ${path.relative(process.cwd(), outputPath)}\n`
);
