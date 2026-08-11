# Leitura: Highlight de Diálogos e Citações

## Objetivo

No modo plain, detectar diálogos (texto entre aspas ou travessão) e citações (blockquotes e inline quotes) e renderizá-los com cor diferente para facilitar a leitura.

## Contexto

`src/renderers.ts` — `renderPlain`.
`src/types.ts` — `ThemePreset` (campos de cor disponíveis).

## Design

### Detecção de diálogo

Padrões a detectar no texto do bloco:

- `"texto entre aspas duplas"` (inglês)
- `"texto entre aspas curvas" / 'texto'`
- `— texto até fim de linha ou próxima pontuação` (travessão narrativo)
- `«texto»` (francês/russo)

### Implementação

Criar função `renderWithDialogueHighlight(line: string, theme: ThemePreset): string` que:

1. Usa regex para encontrar spans de diálogo/citação.
2. Aplica `fg(theme.accent, dialoguePart)` ou uma nova cor `theme.dialogue` (se adicionada) nos trechos de diálogo.
3. Mantém o restante da linha com `fg(theme.foreground, narrativePart)`.

### Cores a usar (sem adicionar campo novo ao tema)

- Diálogo: `theme.accent` (já existe) — cria contraste claro com narrativa.
- Pensamento (itálico seria ideal, mas terminal suporta `\x1b[3m`): `fg(theme.accentMuted, ...)`.

### Blockquotes

Já renderizados com `theme.subtle` — manter, mas adicionar ícone diferente: `❝` ou `"` no início.

### Ativação

Sempre ativo no modo plain. Não afeta modo code.
Possível flag: `/highlight off` para desabilitar se o usuário preferir texto uniforme.

### Casos limítrofes

- Aspas dentro de diálogo (escaped): não abrir novo span.
- Linha que começa com `—` mas não é diálogo (listas, enumerações): heurística — só aplicar se `—` é o primeiro caractere não-espaço da linha.

## Arquivos a modificar

- `src/renderers.ts`: nova função `renderWithDialogueHighlight`, chamar dentro de `renderPlain`
- `src/types.ts` (opcional): adicionar `dialogue?: string` ao `ThemePreset` com fallback para `accent`
- `src/themes.ts` (opcional): adicionar valor `dialogue` nos 4 temas

## Critérios de aceitação

- Diálogos entre aspas duplas detectados corretamente em português e inglês.
- Travessão narrativo detectado no início de parágrafo.
- Não quebra blockquotes já estilizados.
- Não aplica highlight em modo code.