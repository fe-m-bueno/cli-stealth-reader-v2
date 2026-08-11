# Stealth: TypeScript Avançado

## Objetivo
Tornar o modo code mais convincente adicionando construções TypeScript reais além de simples assignments.

## Contexto
`src/renderers.ts` — funções `renderCode`, `disguiseLine`, `LINE_PATTERNS`.
Atualmente há 12 padrões de linha (patConst, patLet, patComment, etc.) e 4 blocos estruturais (import, interface, function, async function).

## O que adicionar

### Novos padrões de linha
- `patCast`: `const x = value as TypeName;`
- `patGenericCall`: `const x = processItems<TypeName>(arg);`
- `patDestructure`: `const { prop1, prop2 } = state;`
- `patSpread`: `const next = { ...ctx, key: "value" };`
- `patTernary`: `const x = cond ? "value" : fallback;`

### Novos blocos estruturais
- **Enum** (ex: `blockIndex % 17`):
  ```ts
  enum StateName { Active, Pending, Resolved }
  ```
- **Decorator + class method** (ex: `blockIndex % 31`):
  ```ts
  @Injectable()
  class ServiceName { … }
  ```
- **Generic function** (ex: `blockIndex % 37`):
  ```ts
  function process<T extends TypeName>(item: T): Promise<T> { … }
  ```
- **Conditional block** (`if/else`) com texto distribuído nos dois branches.

### Nome de tipos genéricos
Expandir `toTypeName` para sufixar `<T>`, `<T, K>`, `<T extends Base>` em ~30% dos usos.

## Arquivos a modificar
- `src/renderers.ts`: adicionar novas funções `pat*` e blocos estruturais, incluir nos arrays `LINE_PATTERNS` e na lógica de seleção de estrutura.

## Critérios de aceitação
- Nenhuma linha ultrapassa `width` colunas (usar `TEXT_OVERHEAD` revisado se necessário).
- Padrão é determinístico: mesmo `blockIndex` → mesmo output.
- Não quebra plain mode.

# DONE
