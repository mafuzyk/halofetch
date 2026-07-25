# AtlasFetch — guia técnico

Este documento explica como o AtlasFetch funciona por dentro. Para instalação e uso diário, comece pelo [README](README.md).

## Escopo

O AtlasFetch é um fetch de informações para terminais Linux desktop.

Objetivos:

- composição visual centralizada e legível;
- personalização sem exigir edição manual de JSON;
- comportamento previsível em diferentes larguras de terminal;
- binário Rust autocontido;
- configuração versionada, migrável e gravada atomicamente.

## Fluxo de execução

```text
argumentos CLI
     │
     ├── ação imediata ── preset / reset / update / setup
     │
     └── renderização
             │
             ├── coleta SysInfo
             ├── carregamento da configuração
             ├── carregamento do logo
             ├── criação dos componentes
             └── composição da cena → ANSI no terminal
```

A entrada está em `src/main.rs`. As opções são declaradas em `src/cli.rs`. `main` coordena módulos; regras de cena, persistência e atualização ficam nos respectivos módulos.

## Módulos

| Caminho | Responsabilidade |
|---|---|
| `src/main.rs` | Orquestra a execução e os caminhos da CLI |
| `src/cli.rs` | Define opções com `clap` |
| `src/config.rs` | Schema v2, defaults, migração e gravação atômica |
| `src/info.rs` | Coleta informações do Linux usando bancos locais quando possível |
| `src/ascii.rs` | Resolve logos embutidos e arquivos personalizados |
| `src/theme.rs` | Cores, presets e gradientes |
| `src/layout.rs` | Presets de espaçamento usados pelo editor |
| `src/render.rs` | Primitivas ANSI compartilhadas |
| `src/output.rs` | Schema JSON versionado |
| `src/benchmark.rs` | Medição integrada da coleta |
| `src/widget.rs` | Converte `FieldDef` em segmentos renderizáveis |
| `src/component/` | Componentes e composição das cenas |
| `src/tui/state.rs` | Estado e navegação testáveis do editor |
| `src/tui/events.rs` | Eventos e mutações interativas |
| `src/tui/editor.rs` | Desenho e ciclo de vida do terminal |
| `src/update.rs` | Atualização baseada em um checkout Git |
| `build.rs` | Incorpora o diretório `logos/` no binário |

## CLI

A referência autoritativa é gerada pelo próprio programa:

```bash
cargo run -- --help
```

Opções atuais:

| Opção | Efeito |
|---|---|
| `-i, --setup` | Abre o editor TUI |
| `--preset NOME` | Aplica uma paleta e encerra |
| `--list-presets` | Lista as paletas |
| `--update` | Atualiza, compila com lockfile e instala |
| `--reset` | Exclui a configuração ativa e abre o editor |
| `--just-ascii` | Mostra somente o logo |
| `--scene CENA` | Sobrescreve a cena apenas nesta execução |
| `-c, --config ARQUIVO` | Usa outro arquivo de configuração |
| `--format json` | Emite o schema JSON v1 sem campos vazios |

Os comandos estruturados são `config`, `preset`, `logos`, `update` e `benchmark`. As flags anteriores continuam funcionando para não quebrar configurações de shell existentes.

Cenas aceitas: `classic`, `dashboard`, `cockpit` e `classicfetch`. Os aliases `classic-fetch` e `classic_fetch` também são aceitos.

## Componentes e cenas

`component::Scene` é um enum serializável e a fonte única dos nomes, descrições e identificadores persistidos. Isso evita divergência entre CLI, TUI e configuração e rejeita cenas desconhecidas antes de salvar.

Cada componente implementa:

```rust
pub trait Component: Send + Sync {
    fn name(&self) -> &str;
    fn render_ansi(&self, ctx: &RenderCtx) -> String;
    fn render_styled(&self, ctx: &RenderCtx) -> Vec<Vec<StyledSpan>>;
    fn min_width(&self) -> usize;
    fn min_height(&self) -> usize;
    fn as_any(&self) -> &dyn Any;
}
```

Componentes disponíveis:

- `AsciiComponent`: logo colorido;
- `SystemComponent`: campos configurados nos painéis;
- `MonitorComponent`: métricas ao vivo;
- `CompanionComponent`: bloco compacto de estado.

A cena decide como combinar os componentes. O mesmo modelo estilizado alimenta a saída ANSI e o preview da TUI.

## Configuração

Local padrão:

```text
~/.config/atlasfetch/config.json
```

`--config` aponta para um arquivo, inclusive quando ele ainda não existe. Logos copiados e outros dados auxiliares ficam ao lado desse arquivo.

Principais estruturas:

```text
Config
├── version
├── scene
├── live
│   ├── enabled
│   └── interval_ms
├── logo
│   ├── key
│   ├── path
│   ├── colors
│   └── color_dir
├── title
├── separator
├── panel
├── display
│   ├── left: Vec<FieldDef>
│   └── right: Vec<FieldDef>
├── palette
└── custom_palettes
```

Um `FieldDef` possui `field`, `icon`, `label` e `enabled`. A deduplicação mantém a primeira ocorrência do campo entre os dois painéis.

### Persistência

`Config::save`:

1. serializa para JSON formatado;
2. grava um arquivo temporário no mesmo diretório;
3. renomeia o temporário para o destino.

A renomeação no mesmo sistema de arquivos evita configurações parcialmente escritas.

### Migração

O formato atual é a versão 2. Configurações antigas geradas pela implementação Python são reconhecidas e convertidas. Os arrays posicionais antigos viram objetos `FieldDef`.

Uma configuração inválida é movida para `config.json.invalid` (ou para o próximo sufixo livre) antes de os defaults serem criados. Assim, um erro de sintaxe nunca destrói a única cópia disponível.

## Coleta de informações

`info::collect` reúne um `SysInfo`. A preferência é por interfaces do kernel, como `/proc` e `/sys`; comandos externos são usados quando são a fonte prática disponível, por exemplo para determinados gerenciadores de pacotes e ambientes gráficos.

Falhas individuais devem produzir campo vazio ou fallback, não impedir o fetch inteiro. A contagem dos gerenciadores principais consulta bancos locais antes de executar processos. Ao adicionar um coletor:

1. limite leituras ao necessário;
2. trate arquivos e comandos ausentes;
3. não assuma uma distribuição;
4. não bloqueie esperando rede;
5. adicione um teste à parte puramente textual.

## Logos

`build.rs` lê `logos/` e gera uma tabela incorporada durante a compilação. Assim, o binário conhece centenas de logos sem depender do diretório do repositório em tempo de execução.

Convenções:

- o nome do arquivo é a chave do logo;
- variantes compactas usam normalmente o sufixo `_small`;
- mantenha a arte sem códigos ANSI;
- teste largura e alinhamento com caracteres Unicode.

## Editor TUI

O editor possui cinco abas: Theme, Mode, Panels, ASCII e Save. Alterações são feitas sobre uma cópia da configuração e só substituem a configuração original quando a pessoa escolhe salvar.

A interface usa duas disposições responsivas: controles e preview ficam lado a lado a partir de 100 colunas e empilhados abaixo disso. O tamanho mínimo suportado é 52 × 16; abaixo dele, o editor mostra uma orientação de redimensionamento.

Atalhos globais:

- `Tab` e `Shift+Tab`: avançar e voltar entre seções;
- `Ctrl+S`: salvar e sair de qualquer aba; um campo de texto aberto deve ser confirmado antes com `Enter`;
- `?`: abrir o guia completo de teclado;
- `q` ou `Esc`: solicitar saída. Se houver alterações, um diálogo oferece salvar, descartar ou continuar.

O estado `changed` representa alterações ainda não salvas e é separado de `dirty`, que indica apenas que o preview precisa ser redesenhado. Essa separação evita alertas falsos causados pela atualização automática do monitor.

O modo Monitor atualiza métricas periodicamente. Fora dele, eventos são processados de forma bloqueante para evitar consumo desnecessário de CPU.

O toggle Monitor da aba Mode é persistido em `live.enabled`. Quando ativo, `atlasfetch` abre o workspace contínuo com a cena configurada na parte superior e um shell real, conectado por PTY e interpretado por VT100, na parte inferior. `Ctrl+Q` fecha o workspace; teclas como `Ctrl+C`, `Esc` e `Ctrl+D` pertencem ao shell. `atlasfetch fetch` força uma renderização estática.

`atlasfetch monitor` força o workspace independentemente da preferência salva. `--interval`/`-i` controla o período em milissegundos e `--scene` sobrescreve a cena. O loop preserva o componente de CPU entre frames e atualiza apenas informações voláteis, evitando redetectar pacotes, fontes e ambiente gráfico a cada ciclo.

As métricas Linux são obtidas de interfaces locais: CPU em duas amostras de `/proc/stat`, memória em `/proc/meminfo`, GPU em DRM sysfs ou `nvidia-smi`, temperatura em `thermal_zone` e `hwmon`, bateria em `power_supply` e disco via `statvfs`. Zero é exibido somente quando a leitura realmente retorna zero; sensores indisponíveis são omitidos ou marcados como `N/A`.

Os testes com o backend virtual do Ratatui verificam o layout completo, a mensagem para terminal estreito e a confirmação de saída. Pontos sensíveis para testes futuros:

- busca e seleção de logos;
- cancelamento sem salvar;
- reordenação e deduplicação de campos;
- largura Unicode;
- cenas em terminais estreitos;
- persistência de paletas personalizadas.

## Atualizador

`src/update.rs` recusa checkouts com alterações locais e procura um checkout válido nesta ordem:

1. `ATLASFETCH_SRC`;
2. diretório atual;
3. ancestrais do executável;
4. diretórios de desenvolvimento comuns dentro da home.

Depois executa `git pull --rebase --autostash`, `cargo build --release --locked` e instala em `~/.local/bin/atlasfetch`.

O atualizador é conveniente para instalações a partir do código-fonte; releases empacotadas devem futuramente usar checksums e assinaturas.

## Qualidade e CI

Antes de enviar uma mudança:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-targets --all-features --locked
cargo build --release --locked
```

`.github/workflows/ci.yml` executa essa sequência em pushes e pull requests. `release.yml` repete testes e publica variantes GNU e musl acompanhadas de SHA-256.

Os testes atuais cobrem snapshots determinísticos das quatro cenas, larguras diferentes, navegação e reordenação na TUI, parsing de cenas, validação e deduplicação de config, schema JSON, detecção do checkout, benchmark, normalização de CPU/GPU e dedent de ASCII.

## Como contribuir

Uma mudança está pronta quando:

- a interface documentada corresponde à ajuda da CLI;
- `rustfmt`, Clippy e testes passam sem avisos;
- caminhos de erro não apagam dados silenciosamente;
- a TUI e a saída normal continuam usando a mesma regra;
- novos nomes persistidos possuem migração ou compatibilidade;
- README e este guia foram atualizados quando necessário.
