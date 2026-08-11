# Biblioteca: Exportar Progresso/Posição como JSON

## Objetivo
Exportar e importar o estado de leitura (posições, marcadores, notas, tags) como JSON para sincronizar entre máquinas sem dependência de serviço externo.

## Contexto
`src/storage.ts` — fonte de verdade SQLite.
`src/commands.ts`, `src/executor.ts`.

## Design

### Formato de exportação
```json
{
  "version": 1,
  "exportedAt": "2026-04-14T12:00:00Z",
  "positions": [
    {
      "bookImportHash": "sha256...",
      "bookTitle": "Dom Casmurro",
      "chapterIndex": 3,
      "blockOffset": 42,
      "bookProgress": 0.31
    }
  ],
  "bookmarks": [...],
  "notes": [...],
  "tags": [...]
}
```

Usar `importHash` (já existente em `CanonicalBook`) como chave portável — não depende de path local.

### Comandos
- `/export` — escreve `stealth-reader-export.json` no diretório atual (`state.cwd`)
- `/export <path>` — escreve no caminho especificado
- `/import <path>` — lê JSON, faz merge das posições (não sobrescreve se local for mais recente)

### Estratégia de merge no import
- Para cada entrada do JSON: se `importHash` bater com livro local → atualizar posição se `exportedAt` for mais recente que a posição local salva.
- Bookmarks/notas/tags: adicionar se não existirem (merge aditivo).
- Livros do JSON que não existem localmente: pular (não é possível importar o livro em si).

### Feedback
```
Exported 3 books to ./stealth-reader-export.json
Imported: 2 positions updated, 5 bookmarks added, 0 conflicts
```

## Arquivos a modificar
- `src/storage.ts`: métodos `exportAll(): ExportData` e `importMerge(data: ExportData): ImportResult`
- `src/commands.ts`: `/export`, `/import`
- `src/executor.ts`: implementação (usar `fs.writeFileSync` / `fs.readFileSync`)
- `src/types.ts`: tipos `ExportData`, `ImportResult`

## Critérios de aceitação
- JSON gerado é legível por humanos e válido.
- Import não apaga dados locais mais recentes.
- `/export` sem permissão de escrita → status de erro amigável.
- `importHash` garante matching sem depender de paths de arquivo.
