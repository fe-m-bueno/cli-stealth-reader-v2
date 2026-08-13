# Reading: Dialogue and Quote Highlighting

## Goal

In plain mode, detect dialogue (text in quotation marks or after an em dash) and quotes (blockquotes and inline quotes) and render them in a different color to make reading easier.

## Context

`src/renderers.ts` — `renderPlain`.
`src/types.ts` — `ThemePreset` (the available color fields).

## Design

### Dialogue Detection

Patterns to detect in a block's text:

- `"text in double quotes"` (English)
- `"text in curly quotes" / 'text'`
- `— text up to the end of the line or the next punctuation mark` (narrative em dash)
- `«text»` (French/Russian)

### Implementation

Create a `renderWithDialogueHighlight(line: string, theme: ThemePreset): string` function that:

1. Uses a regex to find dialogue/quote spans.
2. Applies `fg(theme.accent, dialoguePart)` — or a new `theme.dialogue` color, if added — to the dialogue segments.
3. Keeps the rest of the line as `fg(theme.foreground, narrativePart)`.

### Colors to Use (without adding a new theme field)

- Dialogue: `theme.accent` (already exists) — creates clear contrast with the narrative.
- Thought (italics would be ideal, and the terminal does support `\x1b[3m`): `fg(theme.accentMuted, ...)`.

### Blockquotes

Already rendered with `theme.subtle` — keep that, but add a different icon: `❝` or `"` at the start.

### Activation

Always on in plain mode. Does not affect code mode.
Possible flag: `/highlight off` to disable it for users who prefer uniform text.

### Edge Cases

- Quotes inside dialogue (escaped): do not open a new span.
- A line starting with `—` that is not dialogue (lists, enumerations): heuristic — apply only if `—` is the first non-space character on the line.

## Files to Modify

- `src/renderers.ts`: new `renderWithDialogueHighlight` function, called from `renderPlain`
- `src/types.ts` (optional): add `dialogue?: string` to `ThemePreset` with a fallback to `accent`
- `src/themes.ts` (optional): add a `dialogue` value to the 4 themes

## Acceptance Criteria

- Dialogue in double quotes is detected correctly in both Portuguese and English.
- The narrative em dash is detected at the start of a paragraph.
- Already-styled blockquotes are not broken.
- No highlighting is applied in code mode.
