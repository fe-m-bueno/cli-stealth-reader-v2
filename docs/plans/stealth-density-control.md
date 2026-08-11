# Stealth: Density Control

## Objetivo
Controlar a proporção de comentários vs código "denso" no modo code.
Densidade baixa = mais comentários (texto legível disfarçado); alta = só assignments e chamadas.

## Contexto
`src/renderers.ts` — `LINE_PATTERNS` e `disguiseLine`.
`src/types.ts` — `AppSettings`.

## Design

### Novo campo
```ts
// types.ts
export type CodeDensity = 1 | 2 | 3 | 4 | 5; // 1 = max comentários, 5 = max código
export interface AppSettings {
  codeDensity: CodeDensity; // default 3
}
```

### Lógica de seleção de padrão
Dividir `LINE_PATTERNS` em dois buckets:
```ts
const COMMENT_PATTERNS = [patComment, patReturn]; // "legíveis"
const CODE_PATTERNS = [patConst, patLet, patArrow, patConsoleLog, patExport,
                       patThrow, patAwait, patNullish, patOptional, patTypeAnnotation];
```

Em `disguiseLine`, usar densidade para ponderar a escolha:
```ts
// density 1 → 80% comment_patterns
// density 3 → 30% comment_patterns  (atual: ~16%)
// density 5 → 0%  comment_patterns
const commentChance = (6 - density) * 0.18; // aprox.
const pool = Math.random() < commentChance ? COMMENT_PATTERNS : CODE_PATTERNS;
```
> Nota: manter determinismo usando `seed` no lugar de `Math.random()` — mapear `seed % 100` contra threshold calculado pela densidade.

### Comando
`/density <1-5>` — define e persiste a densidade.
Tecla `d` cicla entre 1→3→5 (light, normal, heavy).

### Status bar
Exibir `density:3` junto ao renderMode na footer quando em modo code.

## Arquivos a modificar
- `src/renderers.ts`: lógica de seleção ponderada em `disguiseLine`, recebe `density` como parâmetro
- `src/types.ts`: novo tipo `CodeDensity` e campo em `AppSettings` e `AppState`
- `src/commands.ts`: novo comando `/density`
- `src/storage.ts`: persistir `codeDensity`
- `src/tui.ts`: passar `state.codeDensity` ao `renderBlocks`

## Critérios de aceitação
- Determinístico (seed-based, não `Math.random()`).
- Density 1 produce output legível com ~80% linhas como comentário.
- Density 5 produz zero comentários.
