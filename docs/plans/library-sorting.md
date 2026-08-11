# Biblioteca: Ordenação da Lista de Livros

## Objetivo
Permitir ordenar o overlay de livros por diferentes critérios: progresso, título, autor, data de abertura.

## Contexto
`src/storage.ts` — `listBooksWithProgress()`.
`src/tui.ts` — overlay `"books"`.
`src/input.ts` — teclas dentro do overlay.

## Design

### Critérios de ordenação
```ts
export type LibrarySortKey = "lastOpened" | "title" | "author" | "progress";
export type SortDirection = "asc" | "desc";
```

Default: `lastOpened desc` (comportamento atual).

### Estado
```ts
// AppState
librarySortKey: LibrarySortKey;
librarySortDir: SortDirection;
```

### Implementação
`listBooksWithProgress()` já retorna os dados necessários.
Adicionar `listBooksWithProgress(sort: LibrarySortKey, dir: SortDirection)` que aplica `ORDER BY` no SQLite (ou sort em memória para `progress`, que é calculado).

```sql
-- Para title/author/lastOpened: ORDER BY na query
-- Para progress: sort em memória após query
```

### Teclas dentro do overlay de books
Quando `state.overlay === "books"`:
- `s` → cicla pelo critério de sort: `lastOpened → title → author → progress → lastOpened`
- `r` → inverte direção (asc/desc)

### Header no overlay
Primeira linha do overlay de books exibe o critério atual:
```
  Sort: Last Opened ↓   (Press s to change, r to reverse)
```

### Comando
`/books --sort title` — abre overlay já com sort por título.

## Arquivos a modificar
- `src/storage.ts`: parâmetro de sort em `listBooksWithProgress`
- `src/types.ts`: `LibrarySortKey`, `SortDirection`, campos em `AppState`
- `src/tui.ts`: inicializar sort, passar para `renderOverlay`, exibir header
- `src/input.ts`: teclas `s` e `r` dentro do overlay books
- `src/commands.ts`: flag `--sort` em `/books`

## Critérios de aceitação
- Sort por título é alfabético (case-insensitive).
- Sort por progresso ordena livros não iniciados por último.
- Direção persiste enquanto o overlay está aberto, reseta ao fechar.
