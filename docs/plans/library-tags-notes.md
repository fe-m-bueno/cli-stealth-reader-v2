# Biblioteca: Tags e Anotações (/tag, /note)

## Objetivo
Permitir categorizar livros com tags e adicionar notas livres por livro ou por posição.

## Contexto
`src/storage.ts` — SQLite.
`src/commands.ts`, `src/executor.ts`.
`src/tui.ts` — overlay de books.

## Design

### Tabelas SQLite

```sql
CREATE TABLE IF NOT EXISTS book_tags (
  book_id TEXT NOT NULL,
  tag TEXT NOT NULL,
  PRIMARY KEY (book_id, tag)
);

CREATE TABLE IF NOT EXISTS notes (
  id TEXT PRIMARY KEY,
  book_id TEXT NOT NULL,
  chapter_index INTEGER,  -- null = nota do livro inteiro
  block_offset INTEGER,
  content TEXT NOT NULL,
  created_at INTEGER NOT NULL
);
```

### Comandos de tags
- `/tag <tag>` — adiciona tag ao livro atual (ex: `/tag ficção`, `/tag lendo`)
- `/tag -d <tag>` — remove tag
- `/tags` — lista tags do livro atual

### Comandos de notas
- `/note <texto>` — adiciona nota na posição atual (chapterIndex + blockOffset)
- `/note -l` — lista notas do livro atual em overlay
- `/note -d <id>` — deleta nota

### Overlay de notas
`state.overlay = "notes"` — lista:
```
> Ch.3 §42  "Passagem muito boa sobre..."   [há 3 dias]
  Ch.1 §0   "Contexto histórico relevante"  [há 1 semana]
```
Enter → navega para a posição da nota.

### Filtro na biblioteca por tag
No overlay de `books`, adicionar linha de filtro: `/books ficção` → filtra por tag.
Ou tecla `f` dentro do overlay de books para entrar em modo filtro.

### Exibição no overlay de books
Adicionar tags ao lado do título:
```
> Dom Casmurro — Machado de Assis  [Ch.3 · 42%]  #clássico #lendo
```

## Arquivos a modificar
- `src/storage.ts`: novas tabelas, métodos `addTag`, `removeTag`, `listTags`, `addNote`, `listNotes`, `deleteNote`
- `src/types.ts`: `Note`, `OverlayKind` += `"notes"`, campos em `AppState`
- `src/commands.ts`: `/tag`, `/tags`, `/note`
- `src/executor.ts`: implementação
- `src/tui.ts`: overlay de notes, exibir tags no overlay de books

## Critérios de aceitação
- Tags e notas persistem entre sessões.
- Navegar para nota restaura posição exata.
- `/tag` sem arg exibe lista de tags do livro atual.
