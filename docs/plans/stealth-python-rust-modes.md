# Stealth: Modos Python e Rust

## Objetivo
Permitir que o usuário escolha a linguagem do disfarce: JavaScript/TypeScript (atual), Python ou Rust.

## Contexto
`src/renderers.ts` — `renderCode` e `renderBlocks`.
`src/types.ts` — `RenderMode = "code" | "plain"`.
`src/commands.ts` — comando `/mode`.
`src/storage.ts` — settings persistidos.

## Design

### Novo tipo
```ts
// types.ts
export type CodeLanguage = "typescript" | "python" | "rust";
export interface AppSettings {
  codeLanguage: CodeLanguage; // novo campo, default "typescript"
}
```

### Módulos de renderer
Criar `src/renderers/` com:
- `renderers/typescript.ts` — extrai a lógica atual de `renderCode`
- `renderers/python.ts` — padrões Python:
  - `x = "value"` / `x: str = "value"`
  - `# comment`
  - `print(f"value")`
  - `def func_name(): return "value"`
  - `raise ValueError("value")`
  - `with open(path) as f:` + corpo indentado
- `renderers/rust.ts` — padrões Rust:
  - `let x = "value";` / `let mut x: &str = "value";`
  - `// comment`
  - `println!("value");`
  - `fn func_name() -> &'static str { "value" }`
  - `Err("value")?`
  - `impl TypeName { … }`

### Seleção
`renderCode` delega para o renderer correspondente a `state.codeLanguage`.

### Comando
`/mode python` / `/mode rust` / `/mode typescript` — alterna `codeLanguage` e persiste.
Tecla `m` continua ciclando entre os três (além de plain/code).

## Arquivos a criar/modificar
- Criar `src/renderers/typescript.ts`, `python.ts`, `rust.ts`
- Modificar `src/renderers.ts` (torna-se dispatcher)
- `src/types.ts`: adicionar `CodeLanguage`
- `src/commands.ts`: atualizar `/mode` para aceitar as novas opções
- `src/storage.ts`: migrar settings para incluir `codeLanguage`

## Critérios de aceitação
- Cada linguagem usa cores do tema (keyword, codeString, etc.) de forma coerente com sua sintaxe.
- Persistido entre sessões.
- `/help` e `/keys` listam as opções.

# DONE
