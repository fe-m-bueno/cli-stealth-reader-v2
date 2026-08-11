# Stealth: Indentação Aninhada e Linhas em Branco

## Objetivo
Fazer o output do modo code parecer um arquivo real, com funções aninhadas, blocos condicionais e linhas em branco estratégicas — não uma sequência uniforme de statements.

## Contexto
`src/renderers.ts` — `renderCode`, blocos estruturais (import/interface/function/async).
Atualmente todos os blocos têm profundidade 0 ou 1 de indentação.

## Design

### Linhas em branco entre blocos
Em `renderBlocks`, ao invés de sempre emitir `""` entre blocos, variar:
- 70% → uma linha em branco (atual)
- 20% → nenhuma linha em branco (statements dentro de função)
- 10% → duas linhas em branco (entre "funções")

Usar `lineHash(index, 999) % 10` para decidir deterministicamente.

### Indentação variável dentro de blocos estruturais
Quando um bloco estrutural abre função/if:
- Primeiros N/2 wrapped lines recebem indent `  ` (dentro da função)
- Última linha: `}` de fechamento

Adicionar bloco condicional:
```ts
// blockIndex % 41
if (condName) {
  // primeiras linhas com indent
} else {
  // últimas linhas com indent
}
```

### Blocos aninhados opcionais
Quando bloco está dentro de função (`structLines.length > 0`), ~30% das linhas body recebem indent `    ` (duplo) para simular código dentro de if/for interno.

Usar `lineHash(blockIndex, lineIndex + 50) % 3 === 0` para decidir.

### Padrão "for loop" (novo bloco estrutural, `blockIndex % 43`)
```ts
for (const item of items) {
  // linhas do bloco
}
```

### Padrão "try/catch" (novo bloco estrutural, `blockIndex % 47`)
```ts
try {
  // primeiras linhas
} catch (err) {
  // últimas linhas
}
```

## Arquivos a modificar
- `src/renderers.ts`: lógica de espaçamento em `renderBlocks`, novos blocos estruturais, indentação variável em `renderCode`

## Critérios de aceitação
- Output parece um arquivo JS/TS real quando scrollado rapidamente.
- Nenhuma linha excede `width` (indentação adicional desconta do `textWidth`).
- Determinístico.
