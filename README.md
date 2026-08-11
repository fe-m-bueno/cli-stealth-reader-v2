# stealth-reader

Leitor de livros em tela cheia para o terminal, escrito em Rust. O
`stealth-reader` abre EPUB, CBZ e PDF e oferece dois estilos de leitura:

- **Plain**: prosa limpa, com títulos, citações, listas e quebras de cena
  formatados para leitura;
- **Stealth**: o mesmo conteúdo disfarçado como código plausível de TypeScript,
  Python ou Rust.

Esta é a implementação nativa que substitui o projeto anterior, aqui chamado
de [`stealth-reader-v0`](https://github.com/fe-m-bueno/cli-stealth-reader).
Ela mantém o formato do banco de dados do v0: posições, bookmarks, notas, tags
e configurações continuam disponíveis sem uma migração manual.

![Modo stealth em TypeScript](docs/screenshots/stealth-code-mode.png)

## Recursos

- leitura em TUI de tela cheia, com Ratatui e suporte a mouse;
- importação de EPUB3, com fallback para NCX e ordem do spine;
- suporte a CBZ e PDF, incluindo diagnósticos para páginas sem texto;
- descoberta recursiva de `.epub`, `.cbz` e `.pdf`;
- modo plain com destaque opcional de diálogos;
- modos stealth de TypeScript, Python e Rust;
- densidade do código stealth configurável de 1 a 5;
- modo foco, que centraliza um único bloco de leitura;
- busca no capítulo atual ou no livro inteiro;
- navegação por capítulos, histórico, tabela de conteúdos e progresso;
- bookmarks, notas e tags por livro;
- biblioteca SQLite persistente, com ordenação por título, autor, progresso ou
  última abertura;
- exportação e importação do estado de leitura em JSON;
- cinco color schemes e seis variantes de aparência;
- ritmo de leitura aprendido e estimativa de tempo restante;
- integração opcional com Toggl Track Focus;
- manual de comandos e atalhos disponível dentro do próprio leitor.

## Instalação

### Requisitos

- Linux ou macOS;
- [Rust](https://www.rust-lang.org/tools/install) 1.85 ou mais recente;
- `rustup` é recomendado. O repositório fixa a toolchain `1.97.1` em
  [`rust-toolchain.toml`](rust-toolchain.toml), que o `rustup` instala e usa
  automaticamente;
- terminal interativo com suporte a Unicode e cores ANSI. Os temas em RGB
  funcionam melhor em terminais com suporte a 24-bit color.

O uso normal não requer Node.js. Node.js 20+ só é necessário para regenerar
fixtures de paridade ou executar os benchmarks que comparam com o
`stealth-reader-v0`.

### Instalar a partir do código-fonte

```bash
git clone https://github.com/fe-m-bueno/cli-stealth-reader-v2.git stealth-reader
cd stealth-reader
cargo install --path crates/stealth-reader --locked
stealth-reader --version
```

`cargo install` compila e instala o binário `stealth-reader` em `~/.cargo/bin`.
Depois disso, a execução normal não usa Cargo:

```bash
stealth-reader
stealth-reader --resume
stealth-reader ./livros/dune.epub
```

Se `~/.cargo/bin` ainda não estiver no `PATH`, adicione-o ao shell antes de
abrir um terminal novo.

### Instalar o binário da release

Depois que uma release for publicada, a instalação recomendada baixa o
binário pronto para sua plataforma, valida o checksum e não requer Rust:

```bash
curl -fsSL https://raw.githubusercontent.com/fe-m-bueno/cli-stealth-reader-v2/main/install.sh | bash
```

Por padrão, o binário fica em `~/.local/bin`. Para escolher outro diretório:

```bash
curl -fsSL https://raw.githubusercontent.com/fe-m-bueno/cli-stealth-reader-v2/main/install.sh \
  | STEALTH_READER_INSTALL_DIR="$HOME/bin" bash
```

### Publicar uma release

Ao criar e enviar uma tag no formato `v*`, o GitHub Actions compila e publica
artefatos para Linux x86_64, macOS Intel e macOS Apple Silicon:

```bash
git tag v0.1.0
git push origin v0.1.0
```

Cada release inclui um `.tar.gz` por plataforma e seu arquivo `.sha256`.

## Uso rápido

```bash
stealth-reader                         # abre a biblioteca
stealth-reader --resume                 # retoma o livro aberto mais recentemente
stealth-reader ./livros/dune.epub      # importa e abre um arquivo
stealth-reader ./quadrinhos/comic.cbz
stealth-reader ./artigo.pdf
```

Também estão disponíveis as opções globais do binário:

```text
stealth-reader --help
stealth-reader --version
stealth-reader --resume
stealth-reader [--resume] <FILE>
```

Se um arquivo for informado junto com `--resume`, o arquivo explícito tem
precedência.

### O que acontece ao iniciar

Sem argumentos, o leitor **nunca reabre um livro sozinho** — ele oferece uma
escolha. Na ordem:

1. um arquivo passado na linha de comando é importado e aberto;
2. com `--resume`, o livro mais recente reabre na posição salva;
3. caso contrário, o diretório configurado em `/librarydir` é varrido
   recursivamente e:
   - se a biblioteca já tem livros, o picker da biblioteca abre para você
     escolher;
   - se ela está vazia mas há EPUB/CBZ/PDF no diretório, o file picker abre com
     os arquivos encontrados;
   - se não há nada, o rodapé explica como apontar para outro diretório.

Abrir automaticamente o último livro é comportamento **opt-in** via `--resume`,
e não o padrão: uma sessão que começa no lugar errado custa mais do que uma
tecla a mais.

### Primeira sessão

1. Inicie `stealth-reader`.
2. Pressione `/` e execute `/librarydir ~/Books` para escolher a raiz da sua
   biblioteca, ou use `/add /caminho/para/livro.epub` para importar um arquivo
   diretamente.
3. Sem caminho, `/add` abre um picker recursivo a partir da raiz configurada.
4. Use `j`/`k`, as setas, `Space`/`b` ou a roda do mouse para navegar.
5. Pressione `m` para alternar entre plain, TypeScript, Python e Rust.
6. Pressione `f` para ativar o modo foco.
7. Pressione `?` para consultar os atalhos.
8. Pressione `q` para sair. A posição é salva automaticamente.

O comando `/help` abre o manual completo dentro do leitor. `/help <comando>`
mostra a referência de um comando específico.

## Formatos suportados

### EPUB

O importador valida o container, lê o OPF e resolve a tabela de conteúdos nesta
ordem:

1. EPUB3 `nav.xhtml`;
2. NCX;
3. ordem do spine, quando não há navegação utilizável.

Fragmentos de âncora, capítulos que compartilham arquivos, front matter e
títulos marcados apenas como parágrafos são tratados durante a normalização.
Problemas recuperáveis ficam registrados como diagnósticos sem impedir a
abertura do restante do livro.

### CBZ

Cada imagem do arquivo CBZ vira uma página navegável. O leitor não faz OCR: as
páginas são exibidas como placeholders de imagem e o livro recebe um diagnóstico
avisando que não há texto disponível para os modos de leitura.

### PDF

Cada página vira um capítulo. O importador extrai texto dos content streams e
separa parágrafos por linhas em branco; ele não faz OCR. Páginas somente com
imagem recebem um placeholder e um diagnóstico, em vez de desaparecerem.

## Modos de leitura

### Plain

O modo plain prioriza legibilidade:

```text
CAPÍTULO 1 — ABAIXO PELA TOCA DO COELHO

Uma vez que Alice começou a se entediar de ficar ao lado de sua irmã
no banco, sem nada fazer.

▏ O dia era muito quente e sonolento para ela. Alice começou a sentir
▏ muito sono e preguiça.

· · · · · · ·
```

Títulos recebem destaque, citações usam uma barra lateral, listas preservam a
indentação e quebras de cena são visualmente separadas. O destaque de diálogos
pode ser ligado ou desligado com `/highlight on` e `/highlight off`.

### Stealth

O texto é reformatado como código de uma linguagem escolhida. A sequência da
tecla `m` é:

```text
plain → typescript → python → rust → plain
```

Use `/mode typescript`, `/mode python`, `/mode rust` ou `/mode plain` para
selecionar diretamente. A escolha fica salva nas configurações.

A densidade controla quanto de estrutura sintética aparece no código:

```text
/density 1    # mais comentários e texto explicativo
/density 3    # equilíbrio padrão
/density 5    # mais código e menos comentários
```

`d` alterna rapidamente entre 1, 3 e 5 enquanto um modo stealth está ativo.

## Atalhos de teclado

Os atalhos dependem da tela atual. No modo de comando, as letras são inseridas
no texto; em overlays, `j` e `k` movem a seleção.

### Navegação

| Tecla | Ação |
| --- | --- |
| `j` / `↑` | Rolar para cima |
| `k` / `↓` | Rolar para baixo |
| `Space` / `PgDn` | Avançar uma página |
| `b` / `PgUp` | Voltar uma página |
| `Home` | Ir ao início do capítulo |
| `End` | Ir ao fim do capítulo |
| `←` / `→` | Capítulo anterior / próximo |
| `Shift+T` | Abrir a tabela de conteúdos |
| `Shift+B` | Abrir bookmarks |
| `[` / `]` | Voltar / avançar no histórico de navegação |
| `wheel` | Rolar a página |
| `g` | Ir ao topo |
| `Shift+G` | Ir ao fim |

### Comandos e overlays

| Tecla | Ação |
| --- | --- |
| `/` | Focar a barra de comandos |
| `Enter` | Executar comando ou confirmar item selecionado |
| `Tab` | Completar comando ou alternar sugestões |
| `Esc` | Fechar overlay, cancelar busca ou desfocar a barra |
| `n` / `Shift+N` | Próximo / anterior resultado de busca |
| `d` | Excluir bookmark ou nota selecionada |
| `s` (biblioteca) | Alternar critério de ordenação |
| `r` (biblioteca) | Inverter direção da ordenação |
| `?` / `Ctrl+.` / `Ctrl+X` | Abrir atalhos |
| `Ctrl+C` | Sair do leitor |

### Visualização

| Tecla | Ação |
| --- | --- |
| `m` | Alternar modo de renderização |
| `f` | Alternar modo foco |
| `c` | Abrir picker de color schemes |
| `Shift+C` | Abrir picker de temas |
| `Shift+S` | Abrir configurações |
| `p` | Alternar a informação de progresso |
| `q` | Sair do leitor ou fechar um overlay |

No painel de atalhos, `z` recolhe ou expande todos os grupos. No painel de
configurações, `←`/`h` e `→`/`l` trocam de aba e `Space` altera o campo atual.

## Slash commands

Pressione `/` para abrir a barra. Comandos podem ser digitados com ou sem a
barra inicial depois que a barra está ativa; argumentos com espaços devem usar
aspas simples ou duplas. `Tab` completa nomes e flags.

### Navegação

```text
/prev [count]                         capítulo anterior
/next [count]                         próximo capítulo
/chapters [query] [--current] [--flat] tabela de conteúdos
/goto <n|%> [--chapter]               posição por capítulo ou porcentagem
/search [-g|--global] <term>         busca no capítulo ou no livro inteiro
```

Exemplos:

```text
/prev 2
/chapters introduction --current
/goto 5
/goto 42%
/goto 30% --chapter
/goto 30%c
/search "chapter one"
/search -g mordor
```

Por padrão, `/search` pesquisa apenas o capítulo atual. Use `-g` ou `--global`
para pesquisar o livro inteiro e depois `n`/`Shift+N` para percorrer os
resultados.

### Biblioteca e livros

```text
/changebook [query] [--recent] [--cwd] [--sort <key>]
/book [query] [--recent] [--cwd] [--sort <key>]
/resume [book-query] [--latest]
/add [path] [--cwd] [--force]
/librarydir [path] [--cwd]
/bookdir [path] [--cwd]
/remove [book-query] [--current]
/removecurrent [--confirm]
```

`/book` é alias de `/changebook` e `/bookdir` é alias de `/librarydir`.
`--sort` aceita `lastOpened`, `title`, `author` ou `progress`.

```text
/changebook dune
/changebook --recent
/changebook --sort progress
/resume --latest
/add ./livros/exemplo.epub
/add ./quadrinhos/comic.cbz --force
/add --cwd
/librarydir ~/Books
/librarydir --cwd
/remove dune
/remove --current
/removecurrent --confirm
```

`/remove` remove apenas o livro da biblioteca local; nunca apaga o arquivo
original no disco.

### Aparência e leitura

```text
/mode [plain|typescript|python|rust]
/density [1-5]
/highlight <on|off>
/toggleprogress [time-chapter|time-book|book|both|chapter|hidden]
/colorscheme [scheme] [--preview] [--list]
/theme [theme] [--list]
/mouse [on|off]
/settings
```

#### Mouse e seleção de texto

A captura de mouse vem **desligada** por padrão, e é assim de propósito: com ela
desligada o terminal continua dono do ponteiro, então arrastar o cursor sobre o
texto seleciona e copia como em qualquer outra saída de terminal. Só a roda
chega ao leitor, e ela rola a página normalmente.

Com `/mouse on` o leitor passa a receber cliques e arrastos:

- clicar na trilha da barra de rolagem salta para o ponto correspondente do
  capítulo, e arrastar o cursor da barra move a leitura continuamente;
- clicar em uma linha de um overlay move a seleção para ela; nos grupos do painel
  de atalhos, clicar no cabeçalho dobra ou desdobra o grupo;
- clicar em `[×]` fecha o modal e clicar na linha de busca começa a filtrar.

Qualquer arrasto fora da barra de rolagem continua sendo do terminal. Nos
terminais que reservam o arrasto simples para a aplicação (a maioria), a seleção
nativa segue disponível com **Shift+arrasto** enquanto a captura está ligada;
onde o terminal não oferece esse atalho, `/mouse off` devolve a seleção normal.

Color schemes:

```text
codex     claude     graphite     amber     forest
```

Temas de aparência:

```text
dark     light     dark-colorblind     light-colorblind
dark-ansi     light-ansi
```

`/colorscheme` e `/theme` sem argumento abrem os respectivos pickers. A flag
`--preview` de `/colorscheme` é aceita para compatibilidade; `--list` mostra a
lista completa.

Progresso pode mostrar tempo estimado ou porcentagens. A ordem de ciclo da tecla
`p` é:

```text
time-chapter → time-book → book → both → chapter → hidden
```

`/settings` abre um painel com preview transacional e quatro abas: `Themes`,
`Reading`, `Layout` e `More`. `Enter` salva; `Esc` cancela e restaura o estado
anterior. As opções incluem escala de texto, margens, espaçamento, destaque de
diálogos e captura do mouse.

### Bookmarks, notas e tags

```text
/mark [label]
/marks
/delmark <id|label>

/note [text]
/note -l
/note -d <id>

/tag [tag]
/tag -d <tag>
/tags
```

Exemplos:

```text
/mark "voltar nesta passagem"
/marks
/delmark "voltar nesta passagem"
/note "verificar esta citação"
/note -l
/tag favorite
/tag -d favorite
/tags
```

Bookmarks e notas podem ser selecionados nos overlays e abertos com `Enter`.
Dentro deles, `d` remove o item selecionado.

### Exportação e importação

```text
/export [path]
/import [path]
```

`/export` salva posições, bookmarks, notas e tags em JSON. Sem caminho, usa o
arquivo padrão da aplicação. `/import` faz merge do arquivo exportado; a
identidade dos livros usa o hash do conteúdo, não o caminho absoluto, tornando o
arquivo adequado para sincronização entre máquinas.

### Toggl Track

A integração é opcional e usa a API Focus do Toggl Track 2.0:

```text
/toggl auth
/toggl auth <toggl_sk_...>
/toggl setup
/toggl sync
/toggl recent
/toggl start "Livro" --project "Reading books"
/toggl stop
/toggl log "Livro" --duration 45m --project "Reading books"
/toggl --disconnect
/toggl auth --open
```

Durações aceitam formatos como `25m`, `1.5h` e `900s`. O token fica no banco
local de configurações; o histórico de comandos substitui credenciais por
`<redacted>`.

### Ajuda

```text
/help
/help mode
/help --all
/keyboardshortcuts
/keys
/keys --category navigation
/keyboardshortcuts --category commands
```

`/keys` é alias de `/keyboardshortcuts`. As categorias aceitas são
`navigation`, `commands` e `view`.

## Dados e compatibilidade com o v0

O comando atual é `stealth-reader`, mas o nome do diretório de dados continua
`cli-stealth-reader` de propósito. Esse é o contrato que permite ao v2 abrir o
mesmo banco do [`stealth-reader-v0`](https://github.com/fe-m-bueno/cli-stealth-reader)
sem copiar ou converter a biblioteca.

| Dado | Caminho padrão |
| --- | --- |
| Banco SQLite | `~/.local/share/cli-stealth-reader/library.db` |
| Cache de capítulos | `~/.cache/cli-stealth-reader/books/` |

Quando definidos, `XDG_DATA_HOME` e `XDG_CACHE_HOME` substituem os respectivos
diretórios padrão:

```text
$XDG_DATA_HOME/cli-stealth-reader/library.db
$XDG_CACHE_HOME/cli-stealth-reader/books/
```

O cache é reconstruível. O banco contém configurações, livros, capítulos,
posições, bookmarks, notas, tags, diagnósticos e histórico de comandos.

### Usar a biblioteca existente

Se o checkout antigo foi renomeado para `~/Development/stealth-reader-v0`, o
uso é direto:

```bash
stealth-reader
```

Para ter um backup antes da primeira execução:

```bash
cp ~/.local/share/cli-stealth-reader/library.db ~/library.db.backup
```

O v2 abre o banco no lugar e aplica somente alterações compatíveis e idempotentes
(índices e a correção da chave composta de capítulos). O v0 continua capaz de
abrir o banco. `/export` e `/import` são a forma recomendada de transportar o
estado entre máquinas.

Não há necessidade de importar novamente todos os livros já presentes. Use
`/add --force` apenas quando quiser reprocessar um arquivo ou quando o parser
for atualizado.

## Arquitetura do código

O workspace é dividido por responsabilidade:

```text
stealth-reader
└── reader-tui             terminal, Ratatui, input e overlays
    └── reader-app         estado, layout e execução dos comandos
        ├── reader-core    domínio, renderização, temas, ritmo e parser de comandos
        ├── reader-formats EPUB, CBZ, PDF, HTML, XML e descoberta de arquivos
        ├── reader-storage SQLite, caminhos XDG, compatibilidade e export/import
        └── reader-integrations  integração Toggl Track Focus
```

O binário `stealth-reader` é apenas a composition root: lê argumentos, abre o
storage, monta o estado inicial e inicia a TUI. `reader-core` não depende de
terminal, banco, ZIP, PDF ou HTTP, o que mantém a lógica principal determinística
e testável.

## Desenvolvimento

### Verificações locais

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Durante o desenvolvimento, é possível testar apenas um crate:

```bash
cargo test -p reader-core
cargo test -p reader-formats
cargo test -p reader-storage
cargo test -p reader-tui
```

### Benchmarks

```bash
cargo build --release -p reader-bench -p stealth-reader
cargo run --release -p reader-bench -- --json
```

Os benchmarks de comparação exigem um build do `stealth-reader-v0` e Node.js:

```bash
cd ~/Development/stealth-reader-v0
npm install
npm run build

cd ~/Development/stealth-reader
V1_DIR=~/Development/stealth-reader-v0 node bench/v1-baseline.mjs --json
cargo run --release -p reader-bench -- --json
```

O nome `V1_DIR` é mantido pelos scripts de comparação por compatibilidade com o
histórico da migração; o caminho apontado agora é o checkout `stealth-reader-v0`.
Os resultados e o procedimento completo estão em
[`docs/migration/performance-baseline.md`](docs/migration/performance-baseline.md).

### Fixtures de paridade

As fixtures versionadas permitem executar a suíte Rust sem instalar Node.js.
Para regenerá-las usando o v0:

```bash
V1_DIR=~/Development/stealth-reader-v0 node tools/generate-render-golden.mjs
V1_DIR=~/Development/stealth-reader-v0 node tools/generate-command-golden.mjs
V1_DIR=~/Development/stealth-reader-v0 node tools/generate-import-golden.mjs
V1_DIR=~/Development/stealth-reader-v0 node tools/generate-storage-fixture.mjs
```

A regeneração é intencional e deve ser revisada junto com as mudanças. As
principais coberturas são:

| Fixture | Cobertura |
| --- | --- |
| `reader-core/tests/golden/render-parity.json` | renderização em modos, linguagens, densidades, larguras e espaçamentos |
| `reader-core/tests/golden/command-parity.json` | parsing, erros, sugestões, ajuda e aliases |
| `reader-formats/tests/golden/import-parity.json` | importação canônica de EPUBs e CBZs |
| `reader-storage/tests/fixtures/v1-library.db` | leitura campo a campo de um banco do v0 |

Mais detalhes estão em [`docs/migration/compatibility-contract.md`](docs/migration/compatibility-contract.md).

## Desempenho

No corpus de referência, medido na mesma máquina, a implementação Rust reduziu
substancialmente o custo do leitor:

| Medição | v0 | `stealth-reader` |
| --- | ---: | ---: |
| Inicialização | 339 ms | 0,9 ms |
| Importação de EPUB de 266 mil palavras | 192 ms | 27 ms |
| Renderização de capítulo em stealth | 5,0 ms | 1,8 ms |
| Memória máxima | 157 MB | 15 MB |

Os números completos, corpus e limitações da comparação estão em
[`docs/migration/performance-baseline.md`](docs/migration/performance-baseline.md).

## Documentação adicional

- [Arquitetura da migração](docs/migration/architecture.md)
- [Contrato de compatibilidade](docs/migration/compatibility-contract.md)
- [Dados, backup e rollback](docs/migration/data-migration.md)
- [Melhorias deliberadas sobre o v0](docs/migration/improvements.md)
- [Baseline de desempenho](docs/migration/performance-baseline.md)
- [Como testar](docs/testing.md)
- [Pesquisa de arquitetura](docs/research/codex-and-grok-build-architecture.md)

## Contribuindo

Contribuições são bem-vindas. Preserve a separação entre domínio, adapters,
storage e terminal; adicione testes para mudanças de comportamento e execute as
verificações do workspace antes de abrir um pull request.
