# Biblioteca: Suporte a CBZ e PDF

## Objetivo

Importar arquivos `.cbz` (quadrinhos) e `.pdf` (documentos) além de EPUB, extraindo texto para leitura no terminal.

## Contexto

`src/parser/epub.ts` — pipeline de importação atual.
`src/commands.ts` — `/add`.
`src/storage.ts` — armazenamento.

## Design

### CBZ (Comic Book ZIP)

CBZ é um ZIP contendo imagens (JPG/PNG). Para leitura em terminal:

- Não há texto extraível → tratar cada página como um bloco `image`.
- Exibir metadados: nome do arquivo, número de páginas.
- Em modo plain: mostrar `[Página X/Y: nome-da-imagem.jpg]`.
- Em modo code: `// page_X: "nome-da-imagem.jpg"`.
- Navegação por capítulo = navegação por página.
- **Limitação**: sem OCR, não há texto real. Registrar como `diagnostic: warning`.

Biblioteca necessária: `jszip` (já em uso para EPUB).

### PDF

PDF com texto extraível → usar biblioteca de extração de texto:

- Opção A: `pdf-parse` (npm, sem dependências nativas) — extrai texto página a página.
- Opção B: `pdfjs-dist` — mais completo mas pesado.

Pipeline:

1. Detectar `.pdf` pelo header (`%PDF`).
2. Extrair texto por página usando `pdf-parse`.
3. Cada página → um `CanonicalChapter` com título `"Page N"`.
4. Quebrar texto da página em parágrafos por linhas em branco → `CanonicalBlock[]`.
5. Registrar diagnostic se página não tiver texto (PDF de imagem).

### Detecção de formato

```ts
// parser/index.ts (novo dispatcher)
export async function importFile(path: string): Promise<CanonicalBook> {
  if (path.endsWith(".epub")) return importEpub(path);
  if (path.endsWith(".cbz")) return importCbz(path);
  if (path.endsWith(".pdf")) return importPdf(path);
  throw new Error(`Unsupported format: ${path}`);
}
```

### Discovery

`src/discovery.ts` — expandir glob para incluir `*.cbz` e `*.pdf`.

## Arquivos a criar/modificar

- Criar `src/parser/cbz.ts`
- Criar `src/parser/pdf.ts`
- Criar `src/parser/index.ts` (dispatcher)
- `src/discovery.ts`: incluir `.cbz` e `.pdf` na busca
- `src/executor.ts`: usar dispatcher ao invés de chamar epub diretamente
- `package.json`: adicionar `pdf-parse` como dependência

## Critérios de aceitação

- `/add arquivo.pdf` importa e exibe texto extraível.
- `/add arquivo.cbz` importa e navega por páginas como capítulos.
- Arquivos sem texto extraível importam com diagnostic de warning mas não falham.
- `discoverEpubs` renomeado para `discoverBooks` (ou mantido com nome genérico).

