# Navegação: Marcadores (/mark, /marks)

## Objetivo

Salvar posições específicas dentro de capítulos e navegar para elas.

## Contexto

`src/storage.ts` — SQLite, tabelas existentes.
`src/commands.ts`, `src/executor.ts`, `src/tui.ts`.

## Design

### Tabela SQLite

```sql
CREATE TABLE IF NOT EXISTS bookmarks (
  id TEXT PRIMARY KEY,
  book_id TEXT NOT NULL,
  chapter_index INTEGER NOT NULL,
  block_offset INTEGER NOT NULL,
  label TEXT,           -- nome opcional do marcador
  created_at INTEGER NOT NULL
);
```

### Comandos

- `/mark` — cria marcador na posição atual (label auto: `Ch.3 §42`)
- `/mark <label>` — cria com label personalizado
- `/marks` — abre overlay listando marcadores do livro atual
- `/delmark <id|label>` — remove marcador

### Overlay de marcadores

Reutilizar o mecanismo de overlay existente (`state.overlay = "bookmarks"`).
Lista: `> Ch.3 §42 — "label do marcador"  [há 2 dias]`
Enter → navega para a posição.
`d` sobre item → deleta.

### Tecla rápida

`B` (maiúsculo) → abre overlay de marcadores (como `T` abre capítulos).
`m` já está ocupado (toggle mode), usar `B` de "bookmark".

### Storage

Adicionar métodos em `Storage`:

```ts
addBookmark(bookId, chapterIndex, blockOffset, label?): Bookmark
listBookmarks(bookId): Bookmark[]
deleteBookmark(id): void
```

## Arquivos a modificar

- `src/storage.ts`: nova tabela e métodos
- `src/types.ts`: `Bookmark`, `OverlayKind` += `"bookmarks"`, `AppState`
- `src/commands.ts`: `/mark`, `/marks`, `/delmark`
- `src/executor.ts`: implementação
- `src/tui.ts`: renderizar overlay de bookmarks
- `src/input.ts`: tecla `B`
- `src/help.ts`: atualizar KEYBOARD_SHORTCUTS

## Critérios de aceitação

- Marcadores persistem entre sessões.
- Navegar para marcador restaura capítulo e offset exatamente.
- Overlay mostra label e data relativa.

