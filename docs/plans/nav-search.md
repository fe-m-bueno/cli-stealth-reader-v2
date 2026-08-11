# Navegação: Busca (/search)

## Objetivo

Buscar um termo de texto dentro do capítulo atual ou do livro inteiro, com navegação entre resultados.

## Contexto

`src/commands.ts` — sistema de slash commands.
`src/executor.ts` — execução de comandos.
`src/types.ts` — `AppState`.
`src/tui.ts` — rendering e overlay.

## Design

### Comando

`/search <termo>` — busca no capítulo atual.
`/search -g <termo>` (ou `/search --global`) — busca em todos os capítulos.

### Estado

```ts
// types.ts
export interface SearchState {
  query: string;
  global: boolean;
  results: Array<{ chapterIndex: number; blockIndex: number; lineIndex: number }>;
  cursor: number; // resultado atual
}
// AppState: searchState: SearchState | null
```

### Fluxo

1. `/search termo` → varrer `chapter.blocks[].text` com `text.toLowerCase().includes(query)`.
2. Cada match → registrar `{ chapterIndex, blockIndex }`.
3. Navegar para o primeiro resultado: setar `state.chapterIndex` e `state.blockOffset` para o bloco correspondente.
4. Teclas `n` / `N` → próximo/anterior resultado (quando `searchState !== null`).
5. `Esc` ou novo `/search` limpa o estado de busca.

### Highlight

Em `renderPlain` e `renderCode`, quando `searchState` ativo, envolver ocorrências do termo com `bg(theme.warning, match)`.

### Status bar

Exibir `[3/12] "termo"` na status bar durante busca ativa.

### Overlay (opcional)

Para buscas globais com muitos resultados, mostrar overlay tipo chapters com lista `Chapter X: match preview`.

## Arquivos a modificar

- `src/types.ts`: `SearchState`, campo em `AppState`
- `src/commands.ts`: definição de `/search`
- `src/executor.ts`: implementação da busca
- `src/renderers.ts`: highlight de match
- `src/input.ts`: teclas `n` / `N`
- `src/tui.ts`: exibir estado de busca na status bar

## Critérios de aceitação

- Busca case-insensitive.
- `n`/`N` cicla circularmente.
- Busca global atravessa capítulos.
- Highlight visível em ambos os modos (plain e code).

