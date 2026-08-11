# UX: Resize Adaptativo

## Objetivo
Garantir que a TUI re-renderize corretamente quando o terminal é redimensionado (SIGWINCH), sem artefatos visuais ou conteúdo cortado.

## Contexto
`src/tui.ts` — `draw`, uso de `process.stdout.columns` e `process.stdout.rows`.
`src/renderers.ts` — `renderBlocks` usa `width` passado por parâmetro.
`src/screen.ts` — `getViewportLayout`, `computeChapterMaxOffset`.

## Diagnóstico atual
- `draw()` lê `process.stdout.columns` no momento da chamada — funciona se `redraw()` for chamado após resize.
- Falta listener de `SIGWINCH` (sinal Unix de resize de terminal) ou `process.stdout.on("resize")`.
- `blockOffset` pode ficar além do novo `chapterMaxOffset` após diminuir a janela.

## Implementação

### Listener de resize
```ts
// tui.ts — dentro de runTui()
process.stdout.on("resize", () => {
  // Revalidar blockOffset (pode ter ultrapassado novo maxOffset)
  const width = process.stdout.columns || 120;
  const height = process.stdout.rows || 40;
  const layout = getViewportLayout(state, width, height);
  const newMax = computeChapterMaxOffset(state, layout.contentWidth, layout.bodyHeight);
  state.blockOffset = Math.min(state.blockOffset, newMax);
  redraw();
});
```

### Cache de layout invalidado
`state.layoutMetrics` (já existe em `AppState`) deve ser invalidado no resize:
```ts
state.layoutMetrics = null;
```

### Limpeza de tela no resize
Antes do `redraw()`, emitir clear para evitar ghost lines de linhas que eram mais longas:
`process.stdout.write("\x1b[2J\x1b[H");` (já feito pelo `renderFrame` se implementado corretamente — verificar).

### Teste manual
- Diminuir janela → texto deve quebrar em colunas menores sem overflow.
- Aumentar janela → mais conteúdo visível, sem linhas em branco artificiais.
- Resize enquanto overlay aberto → overlay re-renderiza com novo width.

## Arquivos a modificar
- `src/tui.ts`: adicionar listener `process.stdout.on("resize", ...)` dentro de `runTui`

## Critérios de aceitação
- Resize não causa artefatos visuais persistentes.
- `blockOffset` é revalidado após resize.
- `layoutMetrics` é invalidado para forçar recálculo.
- Funciona em iTerm2, kitty, Alacritty, xterm — todos emitem SIGWINCH ou `resize` event.
