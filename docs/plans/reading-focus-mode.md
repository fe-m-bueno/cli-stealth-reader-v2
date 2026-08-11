# Leitura: Modo Foco

## Objetivo
Exibir apenas um parágrafo por vez, centralizado na tela, eliminando toda distração.
No modo code, parece que o usuário está debugando/lendo uma função isolada.

## Contexto
`src/tui.ts` — `currentLines`, `draw`.
`src/types.ts` — `AppState`.
`src/input.ts` — navegação.

## Design

### Estado
```ts
// types.ts — AppState
focusMode: boolean;
focusBlockIndex: number; // índice do bloco atual no capítulo
```

### Rendering em modo foco
Substituir `currentLines` quando `focusMode === true`:
1. Pegar `chapter.blocks[state.focusBlockIndex]`.
2. Renderizar com `renderBlocks([block], mode, width, theme)`.
3. Centralizar verticalmente: padding superior = `Math.floor((bodyHeight - lines.length) / 2)`.
4. Opcionalmente exibir número do bloco/total no footer: `§ 42 / 318`.

### Navegação em modo foco
- `k` / `Space` → próximo bloco (`focusBlockIndex++`), ao fim do capítulo → próximo capítulo.
- `j` → bloco anterior.
- Ao atingir o fim do bloco e pressionar k novamente → exibir transição de capítulo (mesmo mecanismo atual).
- `g` / `G` → primeiro/último bloco do capítulo.
- `Esc` ou `f` → sair do modo foco.

### Ativação
- Tecla `f` → toggle `state.focusMode`.
- Ao entrar no modo foco, `focusBlockIndex` é calculado a partir do `blockOffset` atual (mapear offset para o índice de bloco visível mais próximo).

### Status bar
Indicar `[FOCUS]` ao lado do renderMode.

### Sem scrollbar em modo foco
`renderScrollbar` retorna `[]` quando `focusMode === true`.

## Arquivos a modificar
- `src/types.ts`: campos em `AppState`
- `src/tui.ts`: `currentLines` e `draw` com branch para modo foco
- `src/input.ts`: tecla `f`, navegação por bloco
- `src/screen.ts`: `renderStatusBar` ou `renderFooter` para indicar `[FOCUS]`
- `src/help.ts`: documentar `f`

## Critérios de aceitação
- Modo foco exibe exatamente um bloco centralizado.
- `k`/`j` avança/retrocede por bloco, não por linha.
- Toggle `f` retorna à posição equivalente no modo normal (bloco visível → blockOffset).
- Funciona em ambos plain e code mode.
