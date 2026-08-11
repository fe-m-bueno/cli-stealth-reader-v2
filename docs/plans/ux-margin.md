# UX: Margem Lateral Ajustável (/margin)

## Objetivo
Permitir definir margem lateral (padding horizontal) para tornar linhas de texto mais curtas e confortáveis em terminais largos.

## Contexto
`src/screen.ts` — `getViewportLayout` calcula `contentWidth` e `mainWidth`.
`src/types.ts` — `AppSettings`, `AppState`.

## Design

### Comando
`/margin <N>` — define margem de N colunas em cada lado (total 2N subtraído da largura).
`/margin 0` — sem margem (comportamento atual).
Range válido: 0–30.

### Estado e persistência
```ts
// AppSettings / AppState
marginSize: number; // default 0
```
Persiste via `storage.setSetting("marginSize", String(n))`.

### Implementação
Em `getViewportLayout`:
```ts
const effectiveWidth = width - state.marginSize * 2;
// contentWidth, mainWidth calculados sobre effectiveWidth
// mas o conteúdo é centralizado: offset = marginSize colunas à esquerda
```

Ao renderizar linha no body, prefixar com `" ".repeat(marginSize)`.
Scrollbar aparece em `mainWidth + marginSize` (coluna correta).

### Visualização imediata
Ao executar `/margin 8`, re-renderizar imediatamente.
Status: `"Margin set to 8 (16 columns total padding)"`.

### Footer
Exibir `margin:8` no footer quando `marginSize > 0` (similar ao progressVisibility).

### Tecla rápida (opcional)
`<` e `>` para decrementar/incrementar margem em 2 unidades.

## Arquivos a modificar
- `src/types.ts`: `marginSize` em `AppSettings` e `AppState`
- `src/storage.ts`: ler/gravar `marginSize`
- `src/screen.ts`: `getViewportLayout` aplica margem
- `src/tui.ts`: inicializar `marginSize` de settings, aplicar padding no render
- `src/commands.ts`: `/margin`
- `src/executor.ts`: implementação do comando

## Critérios de aceitação
- `/margin 8` com terminal de 120 colunas → conteúdo em 104 colunas, centralizado.
- Margem persiste entre sessões.
- Scrollbar continua alinhado corretamente.
- `/margin 0` restaura comportamento padrão.
