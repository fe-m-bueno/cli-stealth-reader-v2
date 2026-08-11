# Navegação: Jump para Porcentagem (/goto)

## Objetivo

Pular para uma posição específica no livro ou capítulo usando porcentagem ou número de capítulo.

## Contexto

`src/commands.ts`, `src/executor.ts`.
`src/screen.ts` — `computeBookProgress`, `computeChapterMaxOffset`.

## Design

### Variantes do comando

- `/goto 42%` — pula para 42% do livro (calcula capítulo + offset proporcional)
- `/goto 42%c` ou `/goto 42% --chapter` — pula para 42% do capítulo atual
- `/goto 5` — pula para o capítulo 5 (atalho para `/chapters` + Enter)

### Lógica de cálculo

**Porcentagem do livro:**

1. Calcular total de palavras do livro: `sum(chapter.wordCount)`.
2. `targetWord = totalWords * 0.42`.
3. Iterar capítulos acumulando wordCount até encontrar o capítulo que contém `targetWord`.
4. Dentro do capítulo: `blockOffset = Math.floor((targetWord - accumulated) / chapterWordCount * chapterMaxOffset)`.

**Porcentagem do capítulo:**

1. `blockOffset = Math.floor(percentage * chapterMaxOffset)`.

**Número de capítulo:**

1. Validar range, setar `chapterIndex = n - 1`, `blockOffset = 0`.

### Feedback

Após o salto: `status = "Jumped to 42% (Ch.7 · §123)"`.

### Push no histórico de navegação

Chamar `pushNavHistory` antes do salto (integra com o plano de histórico).

## Arquivos a modificar

- `src/commands.ts`: definição de `/goto` com arg `position`
- `src/executor.ts`: implementar os três modos de goto
- `src/input.ts`: não requer mudança de tecla (via command bar)

## Critérios de aceitação

- `/goto 0%` → início do livro, `/goto 100%` → último bloco do último capítulo.
- `/goto 3` em livro com 2 capítulos → status de erro amigável.
- Funciona corretamente com livros de capítulos com wordCount = 0.