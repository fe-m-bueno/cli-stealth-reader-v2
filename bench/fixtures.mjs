// Deterministic benchmark fixtures shared by the v1 (Node) and v2 (Rust) harnesses.
// Generated content is pseudo-random with a fixed seed so both runtimes measure
// exactly the same bytes.
import fs from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";

// `jszip` belongs to the v1 checkout; resolve it from there so this repository
// stays free of a Node dependency tree.
function loadJsZip() {
  const v1Dir = process.env.V1_DIR;
  if (!v1Dir) {
    throw new Error("Set V1_DIR to the cli-stealth-reader (v1) checkout.");
  }
  const require = createRequire(path.join(path.resolve(v1Dir), "package.json"));
  return require("jszip");
}

const JSZip = loadJsZip();

const WORDS = [
  "lantern", "harbour", "quiet", "verdict", "mirror", "sandstone", "whisper", "cradle",
  "orbit", "fathom", "kindle", "solder", "brittle", "meridian", "tunnel", "gossamer",
  "anvil", "cascade", "pallid", "thistle", "ravine", "lattice", "spindle", "hollow"
];

function makeRng(seed) {
  let state = seed >>> 0;
  return () => {
    state = (state * 1664525 + 1013904223) >>> 0;
    return state / 0x1_0000_0000;
  };
}

function sentence(rng, words) {
  const parts = [];
  for (let index = 0; index < words; index += 1) {
    parts.push(WORDS[Math.floor(rng() * WORDS.length)]);
  }
  const text = parts.join(" ");
  return `${text.charAt(0).toUpperCase()}${text.slice(1)}.`;
}

function paragraph(rng) {
  const sentences = [];
  const count = 3 + Math.floor(rng() * 3);
  for (let index = 0; index < count; index += 1) {
    sentences.push(sentence(rng, 8 + Math.floor(rng() * 10)));
  }
  if (rng() < 0.25) {
    sentences.push(`"${sentence(rng, 6)}" she said.`);
  }
  return sentences.join(" ");
}

export const LARGE_EPUB_CHAPTERS = 40;
export const LARGE_EPUB_PARAGRAPHS = 120;
export const CBZ_PAGES = 200;
export const PDF_PAGES = 50;

function epubChapterHtml(rng, chapterIndex, paragraphs) {
  const body = [`<h1 id="start">Chapter ${chapterIndex + 1}</h1>`];
  for (let index = 0; index < paragraphs; index += 1) {
    const kind = index % 17;
    if (kind === 5) {
      body.push(`<blockquote>${paragraph(rng)}</blockquote>`);
    } else if (kind === 9) {
      body.push(`<ul><li>${paragraph(rng)}</li><li>${paragraph(rng)}</li></ul>`);
    } else if (kind === 13) {
      body.push("<hr/>");
      body.push(`<h2>Section ${index}</h2>`);
      body.push(`<p>${paragraph(rng)}</p>`);
    } else {
      body.push(`<p>${paragraph(rng)}</p>`);
    }
  }
  return `<!doctype html><html xmlns="http://www.w3.org/1999/xhtml"><head><title>Chapter ${chapterIndex + 1}</title></head><body>${body.join("")}</body></html>`;
}

async function writeEpub(filePath, chapters, paragraphs, seed) {
  const rng = makeRng(seed);
  const zip = new JSZip();
  zip.file("mimetype", "application/epub+zip", { compression: "STORE" });
  zip.file(
    "META-INF/container.xml",
    `<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>`
  );

  const navItems = [];
  const manifestItems = [
    `<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>`
  ];
  const spineItems = [];
  for (let index = 0; index < chapters; index += 1) {
    const href = `text/ch${index + 1}.xhtml`;
    zip.file(`OEBPS/${href}`, epubChapterHtml(rng, index, paragraphs));
    navItems.push(`<li><a href="${href}">Chapter ${index + 1}</a></li>`);
    manifestItems.push(`<item id="ch${index + 1}" href="${href}" media-type="application/xhtml+xml"/>`);
    spineItems.push(`<itemref idref="ch${index + 1}"/>`);
  }

  zip.file(
    "OEBPS/nav.xhtml",
    `<!doctype html><html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body><nav epub:type="toc"><ol>${navItems.join("")}</ol></nav></body></html>`
  );
  zip.file(
    "OEBPS/content.opf",
    `<?xml version="1.0"?>
<package version="3.0" xmlns="http://www.idpf.org/2007/opf">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>Benchmark Book</dc:title>
    <dc:creator>Benchmark Author</dc:creator>
  </metadata>
  <manifest>${manifestItems.join("")}</manifest>
  <spine>${spineItems.join("")}</spine>
</package>`
  );

  const buffer = await zip.generateAsync({ type: "nodebuffer" });
  fs.writeFileSync(filePath, buffer);
  return filePath;
}

async function writeCbz(filePath, pages, seed) {
  const rng = makeRng(seed);
  const zip = new JSZip();
  for (let index = 0; index < pages; index += 1) {
    const name = `page-${String(index + 1).padStart(4, "0")}.jpg`;
    const size = 2048 + Math.floor(rng() * 512);
    const bytes = Buffer.alloc(size);
    for (let offset = 0; offset < size; offset += 1) {
      bytes[offset] = Math.floor(rng() * 256);
    }
    zip.file(name, bytes);
  }
  zip.file("ComicInfo.xml", "<ComicInfo><Series>Benchmark</Series></ComicInfo>");
  const buffer = await zip.generateAsync({ type: "nodebuffer" });
  fs.writeFileSync(filePath, buffer);
  return filePath;
}

function writePdf(filePath, pages, seed) {
  const rng = makeRng(seed);
  const objects = [];
  const pageObjectIds = [];
  // 1: catalog, 2: pages, 3: font, then page/content pairs.
  let nextId = 4;
  for (let index = 0; index < pages; index += 1) {
    const pageId = nextId;
    const contentId = nextId + 1;
    nextId += 2;
    pageObjectIds.push(pageId);

    const lines = [];
    let y = 740;
    for (let line = 0; line < 28; line += 1) {
      const text = sentence(rng, 9).replace(/[()\\]/g, "");
      lines.push(`BT /F1 11 Tf 60 ${y} Td (${text}) Tj ET`);
      y -= 24;
    }
    const content = `${lines.join("\n")}\n`;
    objects.push({
      id: pageId,
      body: `<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> >> /Contents ${contentId} 0 R >>`
    });
    objects.push({
      id: contentId,
      body: `<< /Length ${Buffer.byteLength(content, "utf8")} >>\nstream\n${content}endstream`
    });
  }

  objects.unshift({ id: 3, body: `<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>` });
  objects.unshift({
    id: 2,
    body: `<< /Type /Pages /Kids [${pageObjectIds.map((id) => `${id} 0 R`).join(" ")}] /Count ${pages} >>`
  });
  objects.unshift({ id: 1, body: `<< /Type /Catalog /Pages 2 0 R >>` });
  objects.sort((left, right) => left.id - right.id);

  let pdf = "%PDF-1.4\n";
  const offsets = new Map();
  for (const object of objects) {
    offsets.set(object.id, Buffer.byteLength(pdf, "utf8"));
    pdf += `${object.id} 0 obj\n${object.body}\nendobj\n`;
  }
  const xrefOffset = Buffer.byteLength(pdf, "utf8");
  const size = objects.length + 1;
  pdf += `xref\n0 ${size}\n0000000000 65535 f \n`;
  for (let id = 1; id < size; id += 1) {
    pdf += `${String(offsets.get(id) ?? 0).padStart(10, "0")} 00000 n \n`;
  }
  pdf += `trailer<</Size ${size}/Root 1 0 R>>\nstartxref\n${xrefOffset}\n%%EOF\n`;

  fs.writeFileSync(filePath, pdf, "utf8");
  return filePath;
}

/**
 * Create (or reuse) the benchmark corpus inside `directory`.
 * Regeneration is skipped when every file already exists, so repeated
 * benchmark runs measure identical inputs.
 */
export async function ensureFixtures(directory) {
  fs.mkdirSync(directory, { recursive: true });
  const files = {
    smallEpub: path.join(directory, "small.epub"),
    largeEpub: path.join(directory, "large.epub"),
    cbz: path.join(directory, "comic.cbz"),
    pdf: path.join(directory, "doc.pdf")
  };
  if (Object.values(files).every((file) => fs.existsSync(file))) {
    return files;
  }
  await writeEpub(files.smallEpub, 3, 12, 12345);
  await writeEpub(files.largeEpub, LARGE_EPUB_CHAPTERS, LARGE_EPUB_PARAGRAPHS, 987654321);
  await writeCbz(files.cbz, CBZ_PAGES, 555);
  writePdf(files.pdf, PDF_PAGES, 4242);
  return files;
}
