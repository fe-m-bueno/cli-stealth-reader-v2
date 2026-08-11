# Navegação: Histórico de Posições ([ e ])

## Objetivo

Navegar para frente e para trás no histórico de posições visitadas, como navegação de browser ou editor de código.

## Contexto

`src/types.ts` — `AppState`.
`src/input.ts` — handlers de teclas.

## Design

### Estado

```ts
// types.ts
export interface NavHistoryEntry {
  chapterIndex: number;
  blockOffset: number;
}

// AppState:
navHistory: NavHistoryEntry[];
navHistoryCursor: number; // índice atual no histórico
```

### Regras

- Toda mudança de posição "intencional" (mudança de capítulo, `/goto`, click em marcador, Enter em overlay) adiciona entrada ao histórico.
- Scroll normal (j/k/Space) NÃO adiciona ao histórico (geraria spam).
- Histórico limitado a 50 entradas (descartar a mais antiga ao adicionar além do limite).
- Ao adicionar nova entrada enquanto `navHistoryCursor < navHistory.length - 1`, descartar entradas após o cursor (como browser).

### Teclas

- `[` → voltar no histórico (ir para `navHistory[cursor - 1]`)
- `]` → avançar no histórico (ir para `navHistory[cursor + 1]`)
- Quando no início/fim, exibir `status = "No history to go back"` / `"No history to go forward"`.

### Função helper

```ts
function pushNavHistory(state: AppState): void {
  const entry = { chapterIndex: state.chapterIndex, blockOffset: state.blockOffset };
  // descartar forward history se cursor não está no fim
  state.navHistory = state.navHistory.slice(0, state.navHistoryCursor + 1);
  state.navHistory.push(entry);
  if (state.navHistory.length > 50) state.navHistory.shift();
  state.navHistoryCursor = state.navHistory.length - 1;
}
```

Chamar `pushNavHistory` antes de saltos em `executor.ts` (goto, chapter select, bookmark navigate).

## Arquivos a modificar

- `src/types.ts`: `NavHistoryEntry`, campos em `AppState`
- `src/input.ts`: teclas `[` e `]`
- `src/executor.ts`: chamar `pushNavHistory` em saltos
- `src/tui.ts`: inicializar `navHistory: [], navHistoryCursor: -1`
- `src/help.ts`: documentar `[` e `]`

## Critérios de aceitação

- `[` e `]` funcionam corretamente após mudanças de capítulo e goto.
- Scroll normal não polui o histórico.
- Histórico não persiste entre sessões (apenas em memória).

