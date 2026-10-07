# AtlasFetch — guia técnico

Este documento descreve a versão 3 por dentro: módulos, fluxo de execução, catálogo de campos, esquema e migração da configuração, regras de layout das cenas, arquitetura do editor e do workspace ao vivo. Para instalação e uso diário, comece pelo [README](README.md).

## Escopo

O AtlasFetch mostra informações do sistema para terminais Linux. Ele tem três saídas que usam o mesmo modelo de dados:

- a renderização estática, impressa uma vez no terminal ou em um pipe;
- a saída JSON versionada, sem layout;
- o workspace ao vivo, com métricas atualizadas acima de um shell real.

Objetivos da versão 3:

- composição centralizada e legível em larguras diferentes, com fallbacks previsíveis;
- personalização pelo editor, sem exigir edição manual do JSON;
- binário Rust autocontido, com os logos incorporados em tempo de compilação;
- configuração versionada, validada, migrável e gravada atomicamente;
- nenhum valor inventado: campo que não pode ser lido fica ausente ou aparece como `n/a`.

## Estrutura do código

| Caminho | Responsabilidade |
|---|---|
| `src/main.rs` | Despacho dos comandos, renderização estática, primeira execução e saídas auxiliares (listas, paletas, logos) |
| `src/cli.rs` | Comandos e opções com `clap`; formas antigas ocultas |
| `src/config.rs` | Esquema versão 3, valores padrão, validação, normalização, migração de v1/v2, quarentena de arquivos inválidos e gravação atômica |
| `src/field.rs` | Catálogo dos 32 campos: chaves, rótulos, ícones, grupos, descrições, gauges e campos ao vivo |
| `src/info/mod.rs` | `SysInfo`, `Gauges`, `collect` (detecção única), `refresh_live` (leituras do monitor) e formatação dos valores |
| `src/info/system.rs` | Usuário, host, kernel, arquitetura, sistema operacional (os-release), modelo do equipamento, uptime, carga e locale |
| `src/info/hardware.rs` | CPU, temperatura, memória, swap, disco, VRAM, uso e nome da GPU |
| `src/info/desktop.rs` | Shell, terminal, ambiente de desktop, gerenciador de janelas, resolução e fonte do terminal |
| `src/info/network.rs` | IPv4 principal e estado do Wi-Fi |
| `src/info/packages.rs` | Contagem de pacotes dos gerenciadores nativos, Flatpak e Snap |
| `src/info/power.rs` | Bateria e brilho da tela |
| `src/info/procs.rs` | Uma única varredura de `/proc`, usada por shell, terminal, DE, WM e contagem de processos |
| `src/render/mod.rs` | Canvas de texto estilizado (`Line`, `Span`, `Style`), largura de texto e conversões para ANSI, texto puro e ratatui |
| `src/render/scene.rs` | As cenas `classic`, `side` e `dashboard` e seus fallbacks |
| `src/render/blocks.rs` | Linhas de informação, barras, gauges, título e faixa de cores da paleta |
| `src/logo.rs` | Descoberta, limpeza, resolução e colorização dos logos |
| `src/theme.rs` | Paletas embutidas, gradientes e cor de contraste para texto sobre cor |
| `src/tui/mod.rs` | Ciclo de vida do terminal do editor e gravação do logo colado |
| `src/tui/app.rs` | Estado do editor, tratamento de eventos e regras de edição (sem desenho) |
| `src/tui/view.rs` | Desenho do editor com ratatui a partir do estado |
| `src/tui/input.rs` | Campo de texto de uma linha usado pelos popups e pelas linhas de texto |
| `src/live.rs` | Workspace ao vivo: PTY, shell, parser VT100 e atualização das métricas |
| `src/output.rs` | Saída JSON, esquema versão 2 |
| `src/benchmark.rs` | Medição da coleta e da renderização completa |
| `src/update.rs` | Atualização a partir de um checkout Git |
| `build.rs` | Copia `logos/` para o diretório de saída e gera a tabela de logos incorporados |

## Fluxo de execução

```text
atlasfetch [opções] [comando]
     │
     ├── comando
     │     fetch    → render_static: info::collect, cfg.logo_set, scene::render
     │     monitor  → live::run: PTY + info::refresh_live a cada intervalo
     │     config   → tui::run → Config::save (com confirmação no reset)
     │     preset   → lista ou aplica uma paleta e grava a configuração
     │     logos    → lista as chaves ou mostra um logo colorido
     │     benchmark → mede info::collect e a renderização completa
     │     update   → update::run
     │
     └── sem comando (run_default)
           --setup, --reset, --list-presets, --preset, --update, --just-ascii (formas antigas)
           --format json        → output::system_info_json
           terminal interativo sem arquivo de configuração → editor (primeira execução)
           terminal interativo e startup.mode = monitor    → live::run
           demais casos                                    → renderização estática
```

`main` só coordena. As regras de cena, persistência, detecção e atualização ficam nos respectivos módulos. Uma configuração é carregada com `Config::load` antes de qualquer renderização, o que pode disparar a migração ou a quarentena do arquivo.

## CLI

A referência autoritativa é a ajuda do próprio programa, `atlasfetch --help` e `atlasfetch <comando> --help`.

Comandos:

| Comando | Efeito |
|---|---|
| `fetch` | Imprime uma renderização estática e encerra |
| `monitor [-i MS]` | Abre o workspace ao vivo; intervalo de 100 a 60000 ms, padrão `startup.interval_ms` |
| `config [edit\|path\|reset]` | Abre o editor (padrão), mostra o caminho ou move o arquivo para `.bak` e abre o editor com os padrões |
| `preset list` / `preset apply NOME` | Lista paletas embutidas e personalizadas; aplica uma delas a `colors.palette` e grava |
| `logos list` / `logos show [CHAVE]` | Lista as chaves; mostra um logo colorido pela paleta atual (sem chave, o logo configurado para esta máquina) |
| `benchmark [-n N]` | Mede N execuções (1 a 100, padrão 5) |
| `update` | Atualiza o checkout e reinstala |

Opções globais, aceitas antes ou depois do comando:

| Opção | Efeito |
|---|---|
| `-c, --config CAMINHO` | Usa outro arquivo de configuração, mesmo que ele ainda não exista |
| `--format ansi\|json` | Formato da saída; `json` imprime o esquema versão 2 |
| `--scene CENA` | Cena desta execução apenas (`classic`, `side`, `dashboard`) |

Formas antigas, ocultas em `--help` mas ainda aceitas:

| Forma antiga | Equivalente |
|---|---|
| `-i`, `--setup` | `config` |
| `--preset NOME` | `preset apply NOME` |
| `--list-presets` | `preset list` |
| `--update` | `update` |
| `--reset` | `config reset` |
| `--just-ascii` | `logos show` |

Nomes de cena aceitos, sem diferenciar maiúsculas: `classic`; `side`, com os aliases `classicfetch`, `classic-fetch`, `classic_fetch` e `fastfetch`; `dashboard`, com o alias `cockpit`. Um nome desconhecido é rejeitado com a lista das cenas válidas.

Quando a saída não é um terminal, a largura vem de `COLUMNS` e, sem ela, vale 100 colunas. As cores são removidas apenas quando a saída não é um terminal e `NO_COLOR` está definido e não vazio.

## Coleta de informações

### Modelo

`info::SysInfo` guarda um texto de exibição por campo (`BTreeMap<Field, String>`), o objeto `Gauges` com os valores numéricos e a lista `os_ids` (o `ID` e as palavras de `ID_LIKE` do os-release, em minúsculas), usada para escolher o logo.

- `set` remove espaços das pontas e, se o resultado for vazio, remove o campo.
- `get` devolve o texto de exibição; `gauge` devolve a fração de 0 a 1 para os campos que têm barra.
- `SysInfo::sample()` preenche todos os campos e todos os gauges com valores fixos. Serve aos testes e à documentação; o teste `sample_fills_every_field_and_gauge` falha se um campo novo não tiver valor de exemplo.

### Duas passagens

- `collect()` faz a detecção completa uma vez. Não falha, não dorme e não trava: cada coletor devolve `None` quando não consegue ler. Parte dos coletores roda em threads (varredura de processos, pacotes, GPU, Wi-Fi e fonte).
- `refresh_live()` relê apenas o que muda entre quadros: uptime, carga, processos, memória, swap, disco, bateria, temperatura, brilho, uso de CPU e uso de GPU. Não cria processos. O uso de CPU precisa de duas amostras de `/proc/stat` no mesmo `CpuSampler`; a primeira chamada deixa o valor vazio.

### Formatação

- Tamanhos: `usado / total UNIDADE (percentual)`. A unidade é escolhida pelo total (TiB, GiB, MiB, KiB ou B) e aparece uma vez; as casas decimais dependem da ordem de grandeza. Exemplo com a build atual: `0.63 / 15.7 GiB (4%)`.
- Temperatura: graus Celsius inteiros, como `54°C`.
- Brilho e carga da bateria: porcentagem inteira.
- O percentual de memória, swap, disco e VRAM é arredondado; o JSON guarda os bytes e o percentual com duas casas.

### Fontes por módulo

| Módulo | Campos | Fonte |
|---|---|---|
| `system.rs` | `user` | `$USER`; sem ele, o banco de usuários (`getpwuid`) |
| | `host` | `/proc/sys/kernel/hostname` |
| | `kernel` | `/proc/sys/kernel/osrelease` |
| | `arch` | `uname` |
| | `os` | `/etc/os-release` ou `/usr/lib/os-release` (`PRETTY_NAME`, ou `NAME` e `VERSION_ID`); `ID` e `ID_LIKE` alimentam `os_ids` |
| | `device` | DMI em `/sys/devices/virtual/dmi/id`; valores de preenchimento do firmware são descartados; em placas ARM, a árvore de dispositivos |
| | `uptime`, `load` | `/proc/uptime`, `/proc/loadavg` |
| | `locale` | `LC_ALL`, depois `LANG`; `C` e `POSIX` não são informativos |
| `hardware.rs` | `cpu` | `/proc/cpuinfo` (modelo e threads) e `cpufreq/cpuinfo_max_freq` (clock máximo) |
| | `cpu_temp` | sensores hwmon de `k10temp`, `zenpower`, `coretemp`, `cpu_thermal` e `acpitz`, e zonas térmicas; leituras em milésimos de grau ou em graus inteiros |
| | `cpu_usage` | duas amostras de `/proc/stat`, somente no monitor |
| | `memory`, `swap` | `/proc/meminfo`; swap ausente quando não há swap |
| | `disk` | `statvfs("/")` |
| | `vram`, `gpu_usage` | sysfs do DRM, com os contadores do amdgpu (`mem_info_vram_*`, `gpu_busy_percent`) |
| | `gpu` | nomes em `/sys/class/drm`, com `pci.ids` para o nome comercial |
| `desktop.rs` | `shell` | ancestrais do processo (lista de shells conhecidos) e, depois, `$SHELL` |
| | `terminal` | ancestrais do processo e variáveis como `KITTY_WINDOW_ID` e `WEZTERM_EXECUTABLE` |
| | `de`, `wm` | `XDG_CURRENT_DESKTOP`, variáveis de sessão, sockets do WM e nomes de processos conhecidos |
| | `resolution` | modos anunciados por conectores em `/sys/class/drm` |
| | `font` | configuração do kitty, alacritty, wezterm, ghostty, foot, Xresources, Xdefaults e konsole |
| `network.rs` | `local_ip` | `getifaddrs`; interfaces físicas antes das demais, e as virtuais (docker, veth, tun, tailscale etc.) por último |
| | `wifi` | `/proc/net/wireless` (interface e nível); o nome da rede vem de `iwgetid -r` |
| `packages.rs` | `packages` | pacman (`/var/lib/pacman/local`), dpkg (`/var/lib/dpkg/status`), apk, xbps (`xbps-query -l`), rpm (`rpm -qa`), portage (`/var/db/pkg`), nix (`nix-store -qR`) |
| | `flatpak`, `snap` | diretórios `app` do Flatpak do sistema e do usuário; entradas de `/snap` |
| `power.rs` | `battery` | `/sys/class/power_supply`, dispositivo do tipo `Battery` com capacidade |
| | `brightness` | `/sys/class/backlight` |
| `procs.rs` | `processes`, cadeia de ancestrais | uma varredura de `/proc` compartilhada |

Na detecção, os únicos comandos externos são `iwgetid`, `xbps-query`, `rpm` e `nix-store`. Cada um só roda quando o diretório do gerenciador correspondente existe, ou, no caso de `iwgetid`, quando há uma interface Wi-Fi.

Regras ao adicionar um coletor:

1. leia apenas o necessário;
2. trate arquivos e comandos ausentes sem interromper a coleta;
3. não presuma uma distribuição;
4. não espere rede nem processos lentos;
5. separe a regra pura (uma função sobre texto ou sobre um ambiente simulado) para testá-la sem o host.

## Campos

`Field` é o catálogo dos 32 campos. A chave serializada (`snake_case`) é a mesma usada na configuração e no JSON. `Field::ALL` define a ordem de exibição e a ordem do JSON. Os grupos são usados pelo diálogo de adição do editor.

| Chave | Rótulo | Grupo | Descrição | Notas |
|---|---|---|---|---|
| `os` | OS | Sistema | Nome e versão da distribuição | padrão, painel esquerdo |
| `host` | Hostname | Sistema | Nome de rede da máquina | |
| `device` | Model | Sistema | Modelo de hardware informado pelo firmware | |
| `user` | User | Sistema | Nome do usuário atual | |
| `kernel` | Kernel | Sistema | Versão do kernel Linux em execução | padrão, painel esquerdo |
| `arch` | Arch | Sistema | Arquitetura do conjunto de instruções | |
| `uptime` | Uptime | Sistema | Tempo desde o último boot | padrão, painel direito |
| `locale` | Locale | Sistema | Idioma configurado na sessão | |
| `packages` | Packages | Software | Pacotes dos gerenciadores nativos | padrão, painel esquerdo |
| `flatpak` | Flatpak | Software | Número de aplicativos Flatpak instalados | |
| `snap` | Snap | Software | Número de pacotes Snap instalados | |
| `shell` | Shell | Software | Shell que iniciou a sessão | padrão, painel esquerdo |
| `terminal` | Terminal | Software | Emulador de terminal que hospeda a sessão | padrão, painel esquerdo |
| `font` | Font | Software | Fonte do terminal, lida do arquivo de configuração | |
| `de` | DE | Desktop | Ambiente de desktop em uso | padrão, painel esquerdo |
| `wm` | WM | Desktop | Gerenciador de janelas ou compositor em uso | padrão, painel esquerdo |
| `resolution` | Display | Desktop | Resolução dos monitores conectados | legado: `refresh_rate` |
| `cpu` | CPU | Hardware | Modelo, número de threads e clock máximo | padrão, painel direito |
| `cpu_temp` | CPU Temp | Hardware | Sensor de CPU mais quente | gauge |
| `gpu` | GPU | Hardware | Modelo do adaptador gráfico | padrão, painel direito |
| `vram` | VRAM | Hardware | Memória de vídeo em uso e total | gauge |
| `cpu_usage` | CPU Usage | Recursos | Carga atual da CPU | gauge, somente no monitor |
| `gpu_usage` | GPU Usage | Recursos | Carga atual da GPU | gauge, somente no monitor |
| `memory` | Memory | Recursos | Memória RAM em uso e total | gauge, padrão, painel direito |
| `swap` | Swap | Recursos | Swap em uso e total | gauge |
| `disk` | Disk | Recursos | Espaço usado e total do sistema de arquivos raiz | gauge, padrão, painel direito |
| `load` | Load | Recursos | Médias de carga de um, cinco e quinze minutos | |
| `processes` | Processes | Recursos | Número de processos em execução | |
| `local_ip` | Local IP | Rede | Endereço IPv4 principal na rede local | oculto por padrão |
| `wifi` | Wi-Fi | Rede | Nome da rede Wi-Fi e intensidade do sinal | legados: `signal`, `wifi_ssid` |
| `battery` | Battery | Energia | Nível de carga e estado de carregamento | gauge, padrão, painel direito; legados: `battery_level`, `battery_status` |
| `brightness` | Brightness | Energia | Nível do brilho da tela | gauge |

Os 9 campos com gauge são `cpu_usage`, `cpu_temp`, `gpu_usage`, `vram`, `memory`, `swap`, `disk`, `battery` e `brightness`. Só `cpu_usage` e `gpu_usage` são exclusivos do monitor (`live_only`). A marca "padrão" indica os campos visíveis na configuração inicial; `local_ip` também está no painel direito, mas desabilitado.

Os nomes legados aceitos em arquivos da versão 3 editados à mão são resolvidos por `Field::from_key`. Um campo com rótulo ou ícone omitido recebe o padrão do campo.

## Configuração

### Local e caminhos

Local padrão: `$XDG_CONFIG_HOME/atlasfetch/config.json`, ou `~/.config/atlasfetch/config.json` quando `XDG_CONFIG_HOME` está ausente ou é relativo. A opção `--config` substitui o arquivo para o processo inteiro. Os logos do usuário ficam no subdiretório `logos/` do diretório da configuração, e o logo colado pelo editor fica em `custom-logo.txt` nesse mesmo diretório.

### Esquema da versão 3

```text
Config
├── version              3
├── scene                "classic" | "side" | "dashboard"
├── startup
│   ├── mode             "fetch" | "monitor"
│   └── interval_ms      1000                 (100 a 60000)
├── logo
│   ├── source           {"kind": "auto"}
│   │                    {"kind": "builtin", "key": "arch"}
│   │                    {"kind": "file", "path": "~/logo.txt"}
│   │                    {"kind": "none"}
│   ├── gradient         "horizontal" | "vertical"
│   └── auto_small       true                 (usa a variante _small em terminais estreitos)
├── colors
│   ├── palette          ["#C084FC", ...]     (1 a 16 cores; padrão do singularityos)
│   ├── title            "#FF9A98"
│   ├── separator        "#9D85FF"
│   └── value            "#F5DCE3"
├── title
│   ├── enabled          true
│   ├── format           "{user}@{host}"      (até 64 caracteres)
│   └── separator        "─"                  (até 4 caracteres)
├── layout
│   ├── style            "powerline" | "plain"
│   ├── gap              3                    (0 a 20)
│   ├── padding          2                    (0 a 20)
│   ├── cascade          2                    (0 a 10)
│   ├── max_value_width  0                    (0 = sem limite; ou 8 a 200)
│   ├── hide_empty       true
│   └── color_blocks     true
├── fields
│   ├── left             [entrada, ...]
│   └── right            [entrada, ...]
└── custom_palettes      {"nome": ["#RRGGBB", ...]}
```

Cada entrada de campo tem `field` (obrigatório), `label` (até 24 caracteres; padrão: o rótulo do campo), `icon` (até 4 caracteres; padrão: o ícone do campo), `enabled` (padrão `true`) e `bar` (padrão `false`).

Todas as estruturas usam `#[serde(default)]`: uma chave ausente recebe o padrão. Chaves desconhecidas no nível superior são ignoradas. Uma chave de campo desconhecida, ou um valor de tipo errado, invalida o arquivo inteiro (ver quarentena).

O arquivo gravado pelo editor usa indentação de dois espaços e termina com uma quebra de linha.

### Validação e normalização

Há duas etapas, com papéis diferentes:

- **Normalização** ajusta valores fora da faixa ao carregar um arquivo da versão 3 ou migrado. Não há erro: o valor é limitado, truncado ou descartado. O arquivo em disco não é reescrito; só a próxima gravação pelo editor o altera.
- **Validação** recusa a configuração. Ela roda antes de cada gravação e depois da normalização ao carregar. Depois da normalização, uma configuração costuma passar; a validação funciona como rede de segurança, e uma falha leva o arquivo para quarentena.

| Regra | Normalização (ao carregar) | Validação (antes de gravar) |
|---|---|---|
| `version` | — | deve ser `3` |
| `colors.palette` | vazia vira o padrão; mais de 16 cores é truncada a 16 | de 1 a 16 cores |
| `startup.interval_ms` | limitado a 100..60000 | 100..60000 |
| `layout.gap`, `layout.padding` | limitados a 20 | no máximo 20 |
| `layout.cascade` | limitado a 10 | no máximo 10 |
| `layout.max_value_width` | 0 fica como está; outros valores limitados a 8..200 | 0 ou 8..200 |
| `title.format` | truncado a 64 caracteres | no máximo 64 caracteres |
| `title.separator` | truncado a 4 caracteres | no máximo 4 caracteres |
| rótulo e ícone de campo | truncados a 24 e 4 caracteres | no máximo 24 e 4 |
| campos repetidos | mantém a primeira ocorrência (painel esquerdo antes do direito) | cada campo aparece no máximo uma vez |
| `custom_palettes` | nome aparado e truncado a 32; nomes vazios e paletas vazias são descartados; nome repetido mantém o primeiro | nome de 1 a 32 caracteres; de 1 a 16 cores |

Os limites de texto e de número são as constantes de `config.rs`.

### Gravação

`Config::save` valida, cria o diretório, grava `config.json.tmp` com o JSON formatado e renomeia o temporário para o destino. A renomeação no mesmo sistema de arquivos substitui o arquivo de uma vez. Uma configuração inválida não grava nada.

### Carregamento, migração e quarentena

`Config::load_from` decide o caminho pela chave `version`:

```text
arquivo ausente              → padrões em memória; nada é criado
JSON inválido                → quarentena
sem "version" (v1)           → migração
"version": 2                 → migração
"version": 3                 → carregamento atual (padrões para o que falta, normalização, validação)
outra versão ou tipo         → quarentena
```

**Migração (v1 e v2).** A configuração antiga é convertida em uma `Config` nova, com os padrões para as chaves ausentes, e normalizada. Em seguida:

1. o arquivo original é copiado para `config.json.bak`, ou `config.json.bak.1`, `.bak.2` etc. se o nome já existir;
2. a configuração migrada é gravada no lugar do original;
3. uma linha em `stderr` informa a migração e o caminho do backup.

Correspondência principal:

| v1 e v2 | v3 |
|---|---|
| `scene` | `scene`; nomes antigos são lidos com o mesmo `FromStr`, e um nome desconhecido vira `classic` |
| `live.enabled: true` | `startup.mode: "monitor"` |
| `live.interval_ms` | `startup.interval_ms` (limitado) |
| `logo.key` (não vazio) | `logo.source` `builtin` |
| `logo.path` `disabled` | `logo.source` `none` |
| `logo.path` com arquivo existente | `logo.source` `file` |
| `logo.path` com outro valor | `logo.source` `auto` |
| `logo.color_dir: "vertical"` | `logo.gradient: "vertical"` |
| `logo.colors` (`#RRGGBB` na v1, `{r,g,b}` na v2) | `colors.palette` |
| `title.format`, `title.color` | `title.format`, `colors.title` |
| `separator.char`, `separator.color` | `title.separator`, `colors.separator` |
| `panel.val_color` | `colors.value` |
| `panel.gap`, `panel.left_pad`, `panel.max_shift` | `layout.gap`, `layout.padding`, `layout.cascade` |
| `panel.max_val_width` | `layout.max_value_width`; 999 (sem limite na v2) vira 0 |
| `display.left`, `display.right` | `fields.left`, `fields.right`; sufixo `_bar` vira `bar: true`; rótulos abreviados antigos (`Usr`, `Krn`, `Mem [bar]` etc.) viram o rótulo padrão |
| `custom_palettes` | `custom_palettes` |

Não são migrados: `panel.sep_color`, `panel.right_pad` e `separator.length`. As chaves de campo desconhecidas nas listas antigas são descartadas, assim como as entradas que não são objetos nem arrays.

**Quarentena.** Um arquivo que não pode ser usado é renomeado para `config.json.invalid` (ou `.invalid.1`, `.invalid.2` etc.), com um aviso em `stderr`, e os padrões são usados em memória. Nada é gravado de volta, então a única cópia do arquivo continua disponível. A quarentena cobre JSON inválido, versão desconhecida, chave de campo desconhecida, tipo errado e configuração que falha na validação. Se a renomeação falhar, o aviso informa o erro e o arquivo fica no lugar.

## Layout e cenas

### Peças comuns

- `RenderCtx` reúne `SysInfo`, `Config`, o `LogoSet` e a largura disponível. Toda linha devolvida por uma cena é cortada à largura informada.
- `blocks::entries(ctx, lado)` monta as entradas habilitadas, na ordem de configuração. Uma entrada cujo valor não existe some quando `hide_empty` é verdadeiro; caso contrário mostra `n/a`.
- A cor de cada entrada vem da posição entre todas as entradas habilitadas (painel esquerdo primeiro), ciclando pela paleta. Assim, a cor de um campo não muda entre cenas.
- Uma linha de informação tem um custo fixo de colunas (`row_overhead`): no estilo `powerline` são `rótulo + 4` (mais o ícone e um espaço, quando existe); no `plain`, `rótulo + 2`. O valor recebe o que sobra e é cortado com reticências.
- Um valor com barra ocupa 10 células, um espaço e o texto.
- Estilo `powerline`: o rótulo fica em um segmento colorido, seguido de uma seta (`U+E0B0`) e do valor. No painel esquerdo, o valor vem primeiro e a seta aponta para o rótulo (`U+E0B2`). Estilo `plain`: ícone e rótulo na cor da entrada, dois espaços e o valor; no painel esquerdo, valor, dois espaços e rótulo.
- Os logos são escolhidos por `LogoSet::fitting(largura)`: o logo completo se cabe, senão a variante `_small`, se existir; senão nenhum.
- A faixa de cores (`layout.color_blocks`) é uma sequência de blocos de três células, um por cor da paleta, com no máximo oito.
- Os gauges ocupam uma linha cada: rótulo de 10 colunas, barra, porcentagem de 3 dígitos com `%` (a temperatura da CPU aparece em `°C`). Cores: verde abaixo de 60 %, amarelo abaixo de 85 % e vermelho acima; a bateria inverte a regra, pois baixo nível é ruim.

### Classic

`classic` centraliza o conjunto inteiro: painel esquerdo, logo e painel direito.

1. Cada painel tem um **vão natural**: o maior custo fixo de linha (`row_overhead`) mais a maior largura de valor, limitada por `max_value_width`, mais `cascade`. Painel vazio tem vão zero.
2. A largura disponível para os painéis é `largura − 2 × padding − largura do logo − gaps`, em que cada painel não vazio desconta um `gap`.
3. Se os dois vãos naturais cabem, cada painel mantém o seu. Se não cabem, `allocate_spans` divide o espaço: cada painel recebe até metade; um painel que precisa de menos que a metade deixa a sobra para o outro.
4. O bloco `painel esquerdo + gap + logo + gap + painel direito` é centralizado na largura. O logo começa em `início + bloco esquerdo`.
5. Cada painel é desenhado com o seu vão menos `cascade` como largura de valor. Se algum painel tiver os valores cortados abaixo de 10 colunas (ou abaixo da largura natural, quando esta já for menor que 10), a cena passa ao fallback empilhado.
6. Cada linha de um painel é deslocada em direção ao logo por `inward(índice, total, cascade)`: as linhas das extremidades se aproximam o valor completo de `cascade`, a linha do meio não se move e as outras se aproximam proporcionalmente, com arredondamento. As linhas são centralizadas verticalmente em relação ao logo.
7. O título e a régua de separação ficam centralizados sobre o logo, não sobre a largura total, e são limitados às bordas.

Fallback empilhado: logo (se couber na largura menos o padding), título centralizado, linha em branco e todas as entradas em um único bloco, centralizado pela linha mais larga, seguido da faixa de cores.

### Side

`side` põe o logo à esquerda e as informações à direita.

1. O logo precisa caber em `largura − (padding + gap + 30)`. A coluna de informações começa em `padding + logo_w + gap` e tem, no mínimo, 30 colunas.
2. Título, linhas e faixa de cores formam a coluna de informações. As duas colunas são unidas linha a linha; a altura é a maior das duas.
3. Se o logo não cabe, o fallback mostra o logo alinhado após o padding, uma linha em branco e a coluna de informações abaixo, também deslocada pelo padding.

### Dashboard

`dashboard` usa caixas arredondadas. A largura útil é `largura − 2 × padding`, e abaixo de 4 colunas nada é desenhado.

1. A caixa do logo tem o título configurado (`{user}@{host}`, ou `AtlasFetch` se o título estiver desabilitado). Se o logo cabe ao lado da caixa do Sistema, as duas ficam lado a lado, com a mesma altura interna. A reserva é de 37 colunas: 4 da moldura do logo, 1 de espaço entre as caixas, 30 do interior mínimo da caixa do Sistema e 2 das bordas dela.
2. Se não cabe, a caixa do logo ocupa a largura toda acima da caixa do Sistema. Se nem o logo cabe, só a caixa do Sistema é desenhada.
3. A caixa do Sistema tem o título fixo `System` e as entradas habilitadas no estilo `plain`. Usa duas colunas quando o conteúdo tem pelo menos `2 × maior linha + 4`; a primeira metade das entradas fica à esquerda. A faixa de cores aparece no fim, alinhada aos rótulos.
4. A caixa `Resources` mostra todos os gauges que têm leitura, mesmo que o campo correspondente esteja oculto ou desabilitado. Usa duas colunas quando o interior tem 70 colunas ou mais; caso contrário, uma coluna de linhas com no máximo 46 colunas. Sem nenhum gauge, a caixa não aparece.

### Testes de cena

`every_scene_fits_every_width` renderiza cada cena, nos dois estilos e com e sem logo, em várias larguras, e verifica que nenhuma linha passa da largura pedida. `dashboard_frame_lines_are_aligned` confere que as bordas das caixas têm a largura certa. Ao mudar uma cena, esses dois testes são a primeira verificação.

## Logos

`build.rs` copia o diretório `logos/` para o diretório de saída e escreve ali um módulo Rust com uma função de busca e um `include_str!` para cada arquivo. O binário contém todos os logos, sem depender do repositório em tempo de execução.

Convenções:

- o nome do arquivo é a chave do logo;
- a variante compacta tem o mesmo nome com o sufixo `_small`; `logos list` não mostra as variantes compactas nem arquivos que começam com ponto;
- o conteúdo não tem códigos ANSI nem texto de instruções; `Logo::clean` remove os códigos, expande tabs para quatro espaços, retira a indentação comum (espaços e espaços braille contam como indentação), apara as pontas e descarta as linhas vazias das bordas;
- a largura é a largura de exibição, não o número de caracteres.

Fonte do logo (`logo::resolve`):

| Fonte | Resultado |
|---|---|
| `none` | nenhum logo |
| `auto` | a chave escolhida por `detect_key(os_ids)`: o primeiro identificador do os-release que existe entre os logos embutidos (com `-` trocado por `_` como segunda tentativa); se nenhum existir, `linux` |
| `builtin` | a chave configurada; se ela não existir, a chave detectada |
| `file` | o conteúdo do arquivo, com `~` expandido; sem variante compacta |

Para uma chave, um arquivo em `<diretório da configuração>/logos/` vence o logo embutido de mesmo nome. Chaves com `/`, `\` ou que começam com ponto são recusadas. A variante compacta só é carregada quando `logo.auto_small` está ligado.

A colorização (`logo::colorize`) pinta cada célula visível com a paleta, em faixas contíguas. Com `horizontal`, as faixas seguem as linhas; com `vertical`, as colunas. Espaços e espaços braille não recebem cor. A bandeira intersex é um caso especial (um anel roxo sobre fundo amarelo).

## Editor

### Arquitetura

O editor é dividido em três partes que não se misturam:

- **`App` (`tui/app.rs`)** guarda o estado e aplica as regras. `handle_event(Event)` recebe um evento do crossterm e altera o estado. Não desenha nada e não toca o terminal.
- **`view` (`tui/view.rs`)** desenha a partir do `App` com ratatui. Não altera a configuração; o único estado que atualiza é o de rolagem (listas e popup de ajuda), que o ratatui exige como `&mut`.
- **`tui/mod.rs`** cuida do terminal (modo bruto, tela alternativa e colagem entre colchetes, restaurados por `Drop` mesmo em pânico) e do laço principal: espera eventos por 250 ms, chama `tick` para expirar mensagens, redesenha e, ao salvar, grava o logo colado antes da configuração.

`tui/input.rs` fornece `TextInput`, um campo de uma linha com cursor por caractere, limite de tamanho e colagem em que quebras de linha viram espaços. Todos os popups de texto usam esse tipo.

### Estado

`App` guarda a configuração em edição (`cfg`), a cópia salva (`saved`, para saber se há alterações), as informações do sistema (coletadas uma vez ao abrir), a lista de logos disponíveis, a seção e o foco, o popup aberto, a mensagem de estado, o logo colado pendente, a última remoção (para `Ctrl+Z`), o resultado da sessão (`Save` ou `Discard`) e a flag de primeira execução.

Ao salvar, `request_save` valida a configuração. Se a validação falha, a mensagem `Cannot save: ...` aparece e o editor continua aberto. Se passa, a sessão termina com `Save`, e `tui::run` devolve a configuração para `edit_config`, que a grava.

### Fluxo de eventos

```text
Event::Key (sem Release)  → handle_key
    Ctrl+C                → descarta e sai, sem perguntar
    popup aberto          → handle_popup_key (Ctrl+S ainda salva)
    Ctrl+S                → request_save
    Ctrl+Z                → undo_removal
    F1, ?, F2, q          → ajuda, ajuda, tela cheia, pedido de saída
    foco no menu          → menu_key
    foco no conteúdo      → content_key → fields_key | logo_list_key | form_key
Event::Paste              → handle_paste (popups de texto, filtros ou logo)
depois de cada evento     → refresh_logos
```

Regras que valem em todo o editor:

- Teclas de liberação (`KeyEventKind::Release`) são ignoradas.
- Enquanto a lista de logos aceita texto, `q` e `?` são caracteres de filtro, não comandos.
- `q` com alterações abre a confirmação, com `Salvar` selecionado por padrão; sem alterações, sai.
- Mensagens de estado expiram após 4 s, em `tick`.
- Uma colagem com várias linhas só vira logo na seção Logo; em outras seções é ignorada. Uma colagem de uma linha, na lista de logos, vira filtro.

### Seções e linhas de configuração

- **Appearance**: Theme (abre o seletor; `←` `→` percorrem as paletas), Palette (campo de texto com cores `#RRGGBB` separadas por espaço ou vírgula, de 1 a 16), Gradient, Info style, Title color, Separator color, Value color e Save palette as… (salva a paleta atual com um nome; um nome existente pede Enter de novo para sobrescrever).
- **Logo**: Source (`auto`, `builtin`, `file`, `none`, em ciclo), Compact on narrow terminals e, quando a fonte é um arquivo, File path. Abaixo das linhas fica a lista de logos com filtro digitável. `↓` na última linha entra na lista; `↑` no topo volta às linhas.
- **Fields**: duas colunas (painel esquerdo e direito, com cabeçalhos). Veja a seção seguinte.
- **Layout**: Scene, Gap, Padding, Cascade, Max value width (`auto`, depois 8, 12, ... até 200), Hide empty fields, Color blocks, Show title, Title format, Separator.
- **Startup**: Mode (`fetch` ou `monitor`) e Refresh interval (passos de 100 ms até 1 s, de 500 ms até 5 s e de 1 s depois, entre 100 e 60000 ms).

Teclas por seção:

| Seção | Teclas |
|---|---|
| menu | `↑` `↓` escolhem a seção; `→`, `Enter` ou `Tab` abrem a seção |
| linhas | `↑` `↓` movem; `←` `→` alteram escolhas e números; `Enter` edita, abre ou ativa; `Space` alterna os interruptores; `Esc` ou `Tab` voltam ao menu |
| Fields | `↑` `↓` selecionam; `Shift`+`↑` `↓` movem dentro do painel ou cruzam a borda; `Shift`+`←` `→` levam a entrada ao outro painel; `Space` mostra ou oculta; `Enter` abre a edição de rótulo, ícone, barra e reset; `a` ou `Insert` adiciona; `Delete` ou `Backspace` remove; `Ctrl+Z` restaura a última remoção |
| lista de logos | digitar filtra; `Backspace` apaga; `↑` `↓` `PgUp` `PgDn` `Home` `End` movem; `Enter` aplica; `Esc` limpa o filtro e depois sai da lista |

Detalhes das regras:

- Remover uma entrada guarda a posição. `Ctrl+Z` restaura a entrada na mesma posição, se o campo ainda não estiver visível. Só a última remoção fica guardada.
- Adicionar abre o seletor de campos agrupado pelos grupos, com os campos já exibidos marcados. `Enter` adiciona o campo após a entrada selecionada, e o seletor continua aberto para adicionar outros. Um campo já exibido não é adicionado de novo.
- A edição de uma entrada tem quatro linhas: Label, Icon, Show as bar e Reset. `Show as bar` não é aplicável a campos sem gauge.
- Ao mover uma entrada para fora da borda de um painel, ela passa para o início ou o fim do outro painel.
- O limite de largura de valor e o intervalo de atualização têm passos próprios (`step_value_width`, `step_interval`), definidos em `app.rs`.

### Popups

| Popup | Conteúdo |
|---|---|
| Keys (`F1`, `?`) | Ajuda por seção, com rolagem (`↑` `↓` `PgUp` `PgDn` `Home` `End`) |
| Fullscreen (`F2`) | Só a prévia da cena em toda a tela; qualquer tecla volta |
| Confirm (`q`) | Opções Save, Discard e Cancel (rótulos em inglês, como o restante da interface); `←` `→` escolhem, `Enter` confirma, `Esc` cancela |
| Text | Campo de texto com validação em tempo real (`text_error`) |
| Themes | Filtro digitável sobre as paletas embutidas e personalizadas |
| Entry | Editor de uma entrada de campo |
| Add | Seletor de campos |

A área de tela muda conforme o terminal. Abaixo de 60 × 18, a view mostra só a mensagem de tamanho mínimo, mas as teclas continuam sendo tratadas. A partir de 110 colunas, menu, configurações e prévia ficam lado a lado, com a coluna de configurações entre 44 e 60 colunas (34 % da largura do corpo); abaixo disso, a prévia vai para a parte inferior, ao lado do menu e das configurações.

### Prévia

A prévia usa `render::scene::render` com a cena e a configuração em edição, na largura da área de desenho. Ela mostra o que a saída estática mostraria, com as informações coletadas ao abrir o editor.

### Testes do editor

Os testes de `app.rs` criam um `App` com `App::new_for_test(cfg, SysInfo::sample())` e enviam eventos sintéticos com `handle_event(Event::Key(...))` ou `Event::Paste`. Assim, cada regra de teclado é testada sem terminal. Os testes de `view.rs` desenham o `App` em um `TestBackend` do ratatui e procuram o texto esperado no buffer, além de verificar que nenhuma linha ultrapassa a largura da tela. Testes de `input.rs` cobrem cursor, colagem, limites e caracteres largos.

Ao alterar uma tecla, atualize o teste correspondente em `app.rs`, a ajuda (`HELP` em `view.rs`), a legenda do rodapé (`footer_hints`) e a seção "Teclas" deste guia e do README.

## Workspace ao vivo

`atlasfetch monitor` (ou `startup.mode = monitor` em uma execução interativa) abre o workspace de `live.rs`.

Ciclo:

1. `info::collect` roda uma vez e o logo é resolvido. O shell é criado em um PTY com o tamanho da região inferior.
2. A tela alternativa é ativada. A cada intervalo, `refresh_live` atualiza as métricas e a tela é redesenhada. A thread de leitura do PTY alimenta um parser `vt100` (10 000 linhas de histórico) e marca a região como suja, o que também redesenha a tela.
3. Teclas são convertidas em sequências ANSI (`key_bytes`) e enviadas ao shell; colagem é enviada entre marcadores de colagem entre colchetes se o shell os tiver solicitado. Redimensionar a janela também redesenha.
4. `Ctrl+Q` encerra o workspace, e a saída do shell também. O shell é finalizado antes de sair.

Layout: a região superior tem a altura das linhas da cena mais duas bordas, sem passar de 60 % da altura da tela. A região inferior recebe o restante e mostra o título "Shell · Ctrl+Q closes workspace". A região superior tem o título `AtlasFetch · live Nms`.

Shell: `$SHELL -i` (ou `/bin/sh`), com `TERM=xterm-256color` e o diretório atual. Para o fish, o comando recebe `--features=no-query-term` e apaga `fish_greeting` com `-C`, pois a saudação de um fetch atrasaria o prompt e o terminal embutido não responde às consultas.

O workspace precisa de terminal interativo; sem ele, `monitor` falha com uma mensagem.

## Saída JSON

`output::system_info_json` produz um objeto com três chaves:

- `schema_version`: `2`;
- `system`: cada campo presente, como texto de exibição, na ordem de `Field::ALL` (uma implementação manual de `Serialize`; um `BTreeMap` reordenaria as chaves);
- `gauges`: os valores numéricos medidos, com as chaves `memory`, `swap`, `disk` e `vram` (objetos com `used` e `total` em bytes e `percent`), e `cpu_percent`, `gpu_percent`, `cpu_temp_celsius`, `battery_percent` e `brightness_percent`, com duas casas decimais. Chaves sem valor são omitidas.

`cpu_percent` e `gpu_percent` só vêm do `refresh_live`, então a saída estática não as contém. O JSON ignora a configuração de painéis e a cena. Ao mudar o conteúdo do JSON, aumente `SCHEMA_VERSION` e registre a mudança; é o contrato com scripts.

## Benchmark

`benchmark::run` faz um aquecimento (uma coleta e uma renderização completa) e então mede N execuções de:

- `info::collect`;
- a renderização estática completa, em 120 colunas: coleta, logo, cena e conversão para ANSI.

A saída tem o número de execuções, a cena e a largura, e para cada medida o mínimo, a mediana, a média e o máximo em milissegundos.

## Atualizador

`update::run`:

1. localiza o checkout: `ATLASFETCH_SRC` (valida), depois o diretório atual, os ancestrais do executável (até cinco níveis) e, por último, `Projetos/atlasfetch`, `src/atlasfetch`, `atlasfetch`, `code/atlasfetch` e `dev/atlasfetch` dentro da home. Um checkout válido tem `.git`, `Cargo.toml` e `src/main.rs`;
2. recusa o checkout se `git status --porcelain` mostrar alterações;
3. executa `git pull --rebase --autostash`, `cargo build --release --locked` e `install -m 755` para `~/.local/bin/atlasfetch`.

Se qualquer passo falha, o comando para com o motivo. Releases empacotadas não usam esse caminho.

## Qualidade e CI

Antes de enviar uma mudança, rode a mesma sequência do CI:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-targets --all-features --locked
cargo build --release --locked
```

`.github/workflows/ci.yml` executa essas quatro etapas em pushes para `main` e em pull requests, com o toolchain estável, `rustfmt` e `clippy`. `release.yml` roda quando uma tag `v*` é enviada: executa os testes na variante GNU, compila as variantes GNU e musl (esta com `musl-tools`), empacota cada binário em `.tar.gz` e grava um arquivo `.sha256` ao lado.

Os testes cobrem, entre outros: o catálogo de campos (chaves, aliases, grupos e textos); defaults e validação da configuração; normalização; migração de v1 e v2 com backup; quarentena; gravação atômica e backups numerados; cada cena em várias larguras; a navegação, a edição e os atalhos do editor; a saída JSON; a formatação dos valores; a seleção de logos e a limpeza de ASCII; a detecção do checkout do atualizador.

## Como contribuir

Uma mudança está pronta quando:

- o README e este guia descrevem o comportamento novo, e a ajuda da CLI (`--help`) corresponde ao que o programa faz;
- `rustfmt`, Clippy (com `-D warnings`) e os testes passam;
- nenhum erro apaga dados em silêncio: um arquivo que não pode ser usado vai para quarentena, e uma gravação inválida não escreve nada;
- a TUI e a saída estática usam a mesma regra de layout e de texto;
- um nome persistido novo (chave de configuração, chave de campo, nome de cena ou de paleta) tem migração ou aliases de compatibilidade.

Receitas comuns:

- **Novo campo.** Adicione a variante em `src/field.rs` com chave, rótulo, ícone, grupo, descrição e as flags de gauge e de monitor; atualize `Field::ALL` e o tamanho do array; implemente a coleta em `src/info/`; adicione um valor em `SysInfo::sample()`. Se o campo substituir outro nome, mantenha o nome antigo em `from_key`.
- **Nova cena.** Adicione a variante em `Scene`, com `ALL`, nome, rótulo, descrição e aliases em `FromStr`; implemente-a em `src/render/scene.rs` e confirme que `every_scene_fits_every_width` passa com ela.
- **Nova paleta.** Adicione uma entrada `theme!` em `src/theme.rs`. A paleta padrão deve continuar igual à do `singularityos`, como exige um teste.
- **Novo logo.** Coloque o arquivo em `logos/` com o nome da chave; use `_small` para a variante compacta. `build.rs` o incorpora automaticamente.
- **Nova chave de configuração.** Adicione o campo com `#[serde(default)]`, a regra de normalização e a de validação, e um teste. Uma mudança incompatível exige nova versão do esquema, migração a partir da versão anterior e atualização deste guia e do README.
