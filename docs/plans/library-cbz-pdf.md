# Library: CBZ and PDF Support

## Goal

Import `.cbz` (comics) and `.pdf` (documents) files in addition to EPUB, extracting text for reading in the terminal.

## Context

`src/parser/epub.ts` — the current import pipeline.
`src/commands.ts` — `/add`.
`src/storage.ts` — storage.

## Design

### CBZ (Comic Book ZIP)

A CBZ is a ZIP containing images (JPG/PNG). For reading in the terminal:

- There is no extractable text → treat each page as an `image` block.
- Display metadata: file name, page count.
- In plain mode: show `[Page X/Y: image-name.jpg]`.
- In code mode: `// page_X: "image-name.jpg"`.
- Chapter navigation = page navigation.
- **Limitation**: without OCR, there is no real text. Record it as `diagnostic: warning`.

Required library: `jszip` (already used for EPUB).

### PDF

For PDFs with extractable text, use a text extraction library:

- Option A: `pdf-parse` (npm, no native dependencies) — extracts text page by page.
- Option B: `pdfjs-dist` — more complete but heavier.

Pipeline:

1. Detect `.pdf` by its header (`%PDF`).
2. Extract text per page using `pdf-parse`.
3. Each page → one `CanonicalChapter` titled `"Page N"`.
4. Split the page's text into paragraphs on blank lines → `CanonicalBlock[]`.
5. Record a diagnostic if a page has no text (image-only PDF).

### Format Detection

```ts
// parser/index.ts (new dispatcher)
export async function importFile(path: string): Promise<CanonicalBook> {
  if (path.endsWith(".epub")) return importEpub(path);
  if (path.endsWith(".cbz")) return importCbz(path);
  if (path.endsWith(".pdf")) return importPdf(path);
  throw new Error(`Unsupported format: ${path}`);
}
```

### Discovery

`src/discovery.ts` — expand the glob to include `*.cbz` and `*.pdf`.

## Files to Create/Modify

- Create `src/parser/cbz.ts`
- Create `src/parser/pdf.ts`
- Create `src/parser/index.ts` (dispatcher)
- `src/discovery.ts`: include `.cbz` and `.pdf` in the search
- `src/executor.ts`: use the dispatcher instead of calling epub directly
- `package.json`: add `pdf-parse` as a dependency

## Acceptance Criteria

- `/add file.pdf` imports and displays extractable text.
- `/add file.cbz` imports and navigates pages as chapters.
- Files with no extractable text import with a warning diagnostic but do not fail.
- `discoverEpubs` renamed to `discoverBooks` (or kept under a generic name).
