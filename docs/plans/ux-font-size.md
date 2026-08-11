# UX: Ajuste de Text Overhead para Fontes Grandes (/fontsize)

## Objetivo
Permitir que o usuário informe o fator de escala da fonte do terminal (ex: fonte grande = caracteres mais largos visualmente),
ajustando o `TEXT_OVERHEAD` dinamicamente para evitar overflow em fontes maiores.

## Contexto
`src/renderers.ts` — constante `TEXT_OVERHEAD = 42`.
`src/screen.ts` — `getViewportLayout` usa `process.stdout.columns`.

## Problema
`TEXT_OVERHEAD` é fixo em 42. Se o usuário usa fonte grande e o terminal reporta 80 colunas mas linhas parecem mais estreitas visualmente, o texto pode parecer quebrado num ponto estranho. O oposto também: fonte monoespaçada estreita em 200 colunas → linhas muito longas.

## Design

### Comando
`/fontsize <scale>` onde scale é `1.0` (default), `1.5`, `2.0`, `0.75` etc.
Aceitar também valores inteiros: `/fontsize 2` = scale 2.0.

### O que ajusta
Não é possível controlar a fonte do terminal via escape codes.
O que o app pode controlar:
1. **`textWidth`** em `renderCode`: `Math.max(width / scale - TEXT_OVERHEAD, 20)` — reduz largura efetiva para fontes maiores.
2. **Margem implícita** — com scale > 1, aplicar `Math.floor((scale - 1) * 10)` colunas de margem automática.
3. **Wrap em plain mode** — `wrapText(text, width / scale)` para quebrar mais cedo.

### Estado
```ts
// AppSettings / AppState
fontScale: number; // default 1.0
```

### Implementação
Passar `fontScale` para `renderBlocks` e `getViewportLayout`:
```ts
// renderers.ts
const textWidth = Math.max(Math.floor(width / state.fontScale) - TEXT_OVERHEAD, 20);

// screen.ts
const effectiveColumns = Math.floor(columns / fontScale);
```

### Feedback
`/fontsize 1.5` → `"Font scale set to 1.5x (effective width: 80 → 53 columns)"`.

### Persistência
`storage.setSetting("fontScale", String(scale))`.

### Teclas rápidas (opcional)
`+` / `-` para incrementar/decrementar scale em 0.25.
> Cuidado: `+` não está em uso atualmente — verificar colisão.

## Arquivos a modificar
- `src/types.ts`: `fontScale` em `AppSettings` e `AppState`
- `src/storage.ts`: ler/gravar `fontScale`
- `src/renderers.ts`: usar `fontScale` no cálculo de `textWidth`
- `src/screen.ts`: `getViewportLayout` recebe/usa `fontScale`
- `src/tui.ts`: inicializar e passar `fontScale`
- `src/commands.ts`: `/fontsize`
- `src/executor.ts`: implementação

## Critérios de aceitação
- `/fontsize 2` reduz efetivamente a largura do conteúdo à metade.
- `/fontsize 1` restaura comportamento padrão.
- Persiste entre sessões.
- Funciona em conjunto com `/margin`.
