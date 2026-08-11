# UX: Modo Wide (Duas Colunas)

## Objetivo
Em terminais com largura ≥ 120 colunas, exibir o texto em duas colunas lado a lado — simula layout de IDE com dois painéis de código abertos.

## Contexto
`src/tui.ts` — `draw`, `currentLines`.
`src/screen.ts` — `getViewportLayout`, `renderBody`.
`src/types.ts` — `AppState`.

## Design

### Ativação
- `/wide` ou tecla `W` (maiúsculo) → toggle `state.wideMode`.
- Requer `process.stdout.columns >= 120`. Abaixo disso → status: `"Wide mode requires at least 120 columns"`.

### Layout de colunas
```
┌─────────────────────────────────────────────────────────┐
│ status bar (full width)                                 │
├──────────────────────┬──────────────────────────────────┤
│ Coluna A             │ Coluna B                         │
│ (blockOffset + 0..N) │ (blockOffset + bodyHeight..2N)   │
│                      │                                  │
├──────────────────────┴──────────────────────────────────┤
│ footer (full width)                                     │
└─────────────────────────────────────────────────────────┘
```

- `columnWidth = Math.floor((totalWidth - 3) / 2)` (3 = separador `│` + margens).
- Coluna A: linhas `blockOffset..blockOffset + bodyHeight`.
- Coluna B: linhas `blockOffset + bodyHeight..blockOffset + 2 * bodyHeight`.
- Scroll avança `bodyHeight * 2` linhas de uma vez (pageSize dobra).
- Scrollbar: baseado no total de linhas com `effectivePageSize = bodyHeight * 2`.

### Separador entre colunas
Uma coluna de caracteres `│` em `theme.border` separando as duas colunas.

### Renderização
Em `draw()`:
```ts
if (state.wideMode && width >= 120) {
  // renderizar duas colunas
} else {
  // renderizar coluna única (atual)
}
```

### Overlay em wide mode
Overlays (chapters, books, themes) continuam usando layout atual (full width ou lateral direito) — não divididos em colunas.

### Modo foco em wide mode
Incompatível — se `focusMode` ativo, ignorar `wideMode`.

## Arquivos a modificar
- `src/types.ts`: campo `wideMode: boolean` em `AppState`
- `src/tui.ts`: branch de renderização em `draw`
- `src/screen.ts`: `getViewportLayout` retornar `wideMode` layout quando ativo
- `src/input.ts`: tecla `W`
- `src/commands.ts`: `/wide`
- `src/help.ts`: documentar `W`

## Critérios de aceitação
- Duas colunas exibem conteúdo contíguo do livro (não duplicado).
- Scroll avança corretamente (saltando o dobro das linhas).
- Desabilitado automaticamente em terminais estreitos com aviso.
- Não quebra overlays.
