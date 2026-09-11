# 12 — UI/UX: o design system Rubro e a tela O Pulso

## 1. Princípios de interface

1. **O dinheiro é a tela inicial.** Não existe "Dashboard". Existe **O Pulso** — a visão viva do caixa,
   passado e futuro. É a primeira entrada da sidebar e a tela que abre ao entrar.
2. **Densidade honesta.** ERP é ferramenta de trabalho, não landing page. Muita informação por tela,
   organizada por hierarquia visual — não por espaço em branco.
3. **Teclado primeiro.** Toda ação tem atalho. O operador de balcão nunca precisa do mouse.
4. **Vermelho é significado, não decoração.** O vermelho da marca marca **ação primária** e
   **identidade**. Vermelho de erro é um tom distinto e mais escuro. Nunca se confundem.
5. **Nada pisca, nada gira sem motivo.** Animação existe para explicar transição de estado; nunca
   para entreter. E animação custa energia (Pilar I).
6. **Números são tabulares.** Toda coluna monetária alinha as casas decimais. Sempre.
7. **Erro é conversa, não código.** Ver [doc 09 §3](09-protocolo-api.md).

## 2. Paleta

### 2.1 Rubro (primária — referência AnyDesk)

| Token | Hex | Uso |
|---|---|---|
| `rubro-50` | `#FFF2F1` | Fundo de destaque muito sutil, linha selecionada |
| `rubro-100` | `#FFE1DF` | Fundo de badge, hover de item de menu |
| `rubro-200` | `#FFC5C1` | Bordas suaves, divisórias em contexto rubro |
| `rubro-300` | `#FF9C95` | Ilustração, gráfico secundário |
| `rubro-400` | `#F87068` | Hover do botão primário |
| **`rubro-500`** | **`#EF443B`** | **Cor da marca. Botão primário, indicador ativo, logotipo** |
| `rubro-600` | `#D93028` | Pressionado, texto sobre fundo claro (contraste AA) |
| `rubro-700` | `#B3221C` | Erro / destrutivo |
| `rubro-800` | `#8C1A15` | Erro em fundo escuro |
| `rubro-900` | `#6B1411` | Texto de erro em superfície muito clara |

### 2.2 Neutros

| Token | Claro | Escuro | Uso |
|---|---|---|---|
| `fundo` | `#FFFFFF` | `#0F1012` | Superfície da janela |
| `superficie` | `#FAFAFA` | `#17181B` | Cartões, painéis |
| `superficie-2` | `#F4F4F5` | `#1E1F23` | Cabeçalho de tabela, sidebar |
| `borda` | `#E4E4E7` | `#2A2B30` | Divisórias |
| `borda-forte` | `#D4D4D8` | `#3A3B41` | Contorno de campo |
| `texto-fraco` | `#8A8A93` | `#7A7B84` | Legenda, placeholder |
| `texto-medio` | `#52525B` | `#A9AAB2` | Rótulo, texto secundário |
| `texto` | `#27272A` | `#EDEDF0` | Texto principal |
| `texto-forte` | `#111114` | `#FFFFFF` | Títulos, valores em destaque |

### 2.3 Semânticos

| Token | Hex | Significado no sistema |
|---|---|---|
| `positivo` | `#12855A` | Entrada de dinheiro, saldo positivo, autorizado |
| `positivo-suave` | `#E6F5EE` | Fundo |
| `negativo` | `#B3221C` (= `rubro-700`) | Saída de dinheiro, saldo negativo, rejeitado |
| `negativo-suave` | `#FDEBEA` | Fundo |
| `atencao` | `#B45309` | Vence hoje, em contingência, requer conferência |
| `atencao-suave` | `#FEF3E2` | Fundo |
| `info` | `#1D4ED8` | Previsto, informativo, em processamento |
| `info-suave` | `#EAF0FE` | Fundo |
| `neutro-barra` | `#A1A1AA` | Séries neutras em gráfico |

**Regra crítica:** `rubro-500` (marca) e `rubro-700` (erro) nunca aparecem com a mesma função na
mesma tela. Um botão primário é `rubro-500`; um botão destrutivo é `rubro-700` com borda e texto,
nunca preenchido — para que o usuário não confunda "confirmar" com "excluir".

### 2.4 Tema escuro

Mesma estrutura de tokens, valores trocados. `rubro-500` permanece `#EF443B` (a marca não muda);
o que muda é a superfície. Alternância por `Ctrl+Shift+D` ou seguindo o SO.
O tema escuro é o padrão sugerido para terminais de PDV em ambiente de baixa luz e economiza energia
em telas OLED.

## 3. Tipografia

Escala aumentada em 2026-09-11 (revisão de UI/UX pedida pelo usuário a partir de capturas de
tela reais do app: texto pequeno demais para uma janela 1920×1080). Valores antigos entre
parênteses — o token central é [`Papel`](../crates/cardeal-ui/src/tokens/tipografia.rs), nunca
um tamanho solto numa tela.

| Papel | Fonte | Tamanho | Peso |
|---|---|---|---|
| Interface | **Inter** | 14 px (era 13) | 400 / 500 |
| Título de tela | Inter | 23 px (era 20) | 600 |
| Título de seção | Inter | 17 px (era 15) | 600 |
| Rótulo de campo | Inter | 13 px (era 12) | 500, `texto-medio` |
| **Números / dinheiro** | **Inter Tabular** (`font-feature-settings: "tnum"`) | 14 px (era 13) | 500 |
| Valor em destaque (Pulso) | Inter Display | 36 px (era 32) | 600, `tnum` |
| Código, chave de acesso, log | **JetBrains Mono** | 13 px (era 12) | 400 |

Fontes embutidas no binário (~4 MB) para garantir renderização idêntica em qualquer Windows.
Escala global ajustável (90% a 150%) para telas de balcão e para acessibilidade.

## 4. Espaçamento, raio, elevação

- Base **4 px**: `0, 4, 8, 12, 16, 24, 32, 48, 64`.
- Raio: `4` (campo, botão), `8` (cartão), `14` (modal), `999` (pílula/badge).
- Elevação por borda + sombra mínima; nada de sombras dramáticas.
  `nivel-0` sem sombra; `nivel-1` = `0 1px 2px rgba(0,0,0,.06)`; `nivel-2` = `0 4px 12px rgba(0,0,0,.10)`.
- Altura de linha de grade: **32 px** (compacta, era 28), 38 px (confortável, era 32), 44 px
  (toque, era 36). Configurável (aumentada junto da tipografia em 2026-09-11).

## 5. Sidebar retrátil

```
EXPANDIDA (264 px)                    RECOLHIDA (64 px)
┌──────────────────────────┐          ┌──────┐
│ ▮ CARDEAL          « ⌘B  │          │  ▮   │
├──────────────────────────┤          ├──────┤
│ ▸ ❤  Pulso            ●  │  ← ativo │  ❤ ● │
│   💰 Financeiro       ▾  │          │  💰  │
│      Contas a Receber    │          │  🛒  │
│      Contas a Pagar      │          │  📦  │
│      Fluxo de Caixa      │          │  👥  │
│      Contas Bancárias    │          │  📄  │
│      Conciliação         │          │  ⚙  │
│   🛒 Vendas              │          └──────┘
│   🧾 PDV                 │
│   📦 Estoque             │
│   👥 Clientes            │
│   🛍  Compras            │
│   📄 Fiscal          ⚠3  │  ← badge
│   📊 Análises            │
├──────────────────────────┤
│   ⚙  Configurações       │
│   👤 Ana Souza · Matriz  │
└──────────────────────────┘
```

Comportamento:

- Alterna com `Ctrl+B`. A animação dura **160 ms** com `ease-out`; durante ela a janela repinta,
  depois volta a dormir (Pilar I).
- Recolhida mostra só ícones; tooltip aparece após 400 ms de hover.
- **Financeiro é sempre o primeiro grupo** e o Pulso é sua primeira entrada — peso 0, fixo.
- Entradas vêm do registro de módulos ([doc 04](04-pilar-modularidade.md)). Módulo desligado não
  aparece; submódulo desligado não aparece.
- Indicador ativo: barra `rubro-500` de 3 px à esquerda + fundo `rubro-50` + ícone em `rubro-600`.
- Badges numéricos (`⚠3`) para itens que exigem ação; nunca badge decorativo.
- Estado (recolhida/expandida, grupos abertos) é persistido por usuário **e por dispositivo** — o
  operador do PDV quer recolhida, o financeiro quer expandida.

## 6. O Pulso — o financeiro como tela inicial

> O conceito: em vez de cartões desconexos de KPI, **uma linha do tempo contínua do dinheiro**,
> onde passado e futuro estão no mesmo eixo e "hoje" é apenas uma linha vertical.
> O usuário lê o caixa da empresa da mesma forma que lê um eletrocardiograma.

```
┌───────────────────────────────────────────────────────────────────────────────────────┐
│  Pulso · Matriz                          setembro 2026    [◂ mês]  [hoje]  [mês ▸]    │
├───────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                        │
│   SALDO HOJE                 EM 30 DIAS              FÔLEGO           A RECEBER HOJE   │
│   R$ 48.230,15               R$ 71.905,40            34 dias          R$ 12.480,00     │
│   ▲ 4,2% vs ontem            ▲ projetado             ▓▓▓▓▓▓▓░░        7 títulos        │
│                                                                                        │
├───────────────────────────────────────────────────────────────────────────────────────┤
│  RIO DO CAIXA                                            ● realizado ▓ comprometido    │
│                                                          ░ previsto                    │
│   90k ┤                                              ░░░░░░░░░░                        │
│       │                                         ▓▓▓▓░░░░░░░░░░░░░░░                    │
│   60k ┤                    ●●●●●●●●●●      ▓▓▓▓▓▓▓▓▓▓▓░░░░░░░░░░░░░░░░                 │
│       │        ●●●●●●●●●●●●          ●●●●▓▓                          ░░░               │
│   30k ┤●●●●●●●●                          ┊                                             │
│       │                                  ┊ HOJE                                        │
│     0 ┼──────────────────────────────────┊──────────────────────────────────────────   │
│       │                                  ┊         ⚠ 14/out                            │
│  -30k ┤                                  ┊         projeção cruza zero                 │
│       └──┬────────┬────────┬────────┬────┊───┬────────┬────────┬────────┬──────────    │
│         ago      15/ago    set     15/set  1/out    15/out    nov     15/nov           │
│                                                                                        │
│   ⓘ Clique em qualquer ponto para ver os lançamentos daquele dia.                      │
├───────────────────────────────────────────────────────────────────────────────────────┤
│  EXIGE AÇÃO HOJE (4)                          │  ONDE O DINHEIRO ESTÁ                  │
│  ─────────────────────────────────────────    │  ────────────────────────              │
│  🔴 3 títulos vencem hoje      R$ 8.420,00    │  Caixa 1              R$    840,00     │
│     Fornecedor Alfa, Beta, Gama        [Ver]  │  Cofre                R$  3.200,00     │
│  🟠 Projeção cruza zero em 14/out             │  Banco do Brasil      R$ 21.400,15     │
│     Faltam R$ 6.120 no dia         [Simular]  │  Nubank PJ            R$ 14.790,00     │
│  🟠 Caixa 2 aberto há 14 h                    │  Cartões a compensar  R$  8.000,00     │
│     Operador: Carlos              [Fechar]    │  ─────────────────────────────────     │
│  🔵 4 notas de compra a conferir              │  Total                R$ 48.230,15     │
│     Chegaram por DFe               [Abrir]    │                                        │
├───────────────────────────────────────────────┴────────────────────────────────────────┤
│  RADAR                                                                                 │
│  ▸ Margem da categoria Bebidas caiu 3,1 p.p. em 14 dias        [investigar]            │
│  ▸ Cliente Supermercado Sul: 2 títulos em atraso, R$ 4.100     [cobrar]                │
│  ▸ Taxa média de cartão subiu para 2,84% (era 2,41%)           [ver contratos]         │
│  ▸ 12 produtos com giro alto e saldo abaixo do ponto de pedido [gerar pedido]          │
└───────────────────────────────────────────────────────────────────────────────────────┘
```

### 6.1 O Rio do Caixa

O elemento assinatura do produto. Um gráfico de área empilhada em **um único eixo temporal contínuo**:

| Faixa | Preenchimento | O que é |
|---|---|---|
| Passado, sólido `positivo`/`negativo` | ●●● | **Realizado** — dinheiro que efetivamente andou |
| Futuro, hachurado `info` | ▓▓▓ | **Comprometido** — títulos já lançados, com vencimento |
| Futuro, pontilhado `texto-fraco` | ░░░ | **Previsto** — recorrências e estimativa estatística |

Detalhes de interação:

- A linha "HOJE" é fixa e sempre visível; o gráfico rola sob ela.
- **Cruzamento de zero** no futuro é marcado com pino `atencao` e o texto do dia. É a informação mais
  valiosa de um ERP de PME e quase nenhum mostra.
- Clique em um dia abre uma gaveta lateral com os lançamentos daquele dia, agrupados por conta.
- Arrastar sobre um intervalo mostra o total do período na parte superior.
- `Shift+arrastar` compara com o mesmo período do ano anterior (linha fantasma).
- Zoom por scroll: dia → semana → mês → trimestre. O nível de agregação muda a consulta, não só o
  desenho — sempre lendo do razão.

### 6.2 Fôlego (dias de caixa)

`fôlego = saldo atual ÷ média de saída líquida diária dos últimos 30 dias`.

Barra de 0 a 90 dias, com faixas: `< 15` vermelho, `15–45` âmbar, `> 45` verde.
Ao lado, a frase em português: *"Com o ritmo atual, o caixa dura até 5 de outubro."*
Um número que o dono entende sem treinamento — e que muda o comportamento dele.

### 6.3 Exige ação hoje

Fila priorizada, gerada pelos módulos ativos. Cada item declara: severidade, título, valor,
subtítulo e **uma ação primária que resolve** — não "ver relatório", mas "Fechar caixa", "Cobrar",
"Gerar pedido". A meta é que o dono abra o sistema, resolva os 4 itens e feche. Se a lista está
vazia, a área mostra um estado positivo: *"Nada exige sua atenção agora."*

### 6.4 Radar

Anomalias detectadas por comparação estatística ([doc 11](11-analytics-bi.md)), sempre com:
o número, a comparação, o período e um botão que leva ao detalhe. **Nunca** um alerta sem explicação.

### 6.5 Cartões fixados

Qualquer resposta da tela de Perguntas pode ser fixada no Pulso, virando um cartão personalizado.
É assim que cada empresa constrói o seu painel — sem nunca ter existido um "editor de dashboard".

## 7. Componentes do design system

Todos em `cardeal-ui`, com galeria viva em `cargo run -p cardeal-ui --example galeria`.

| Componente | Notas de comportamento |
|---|---|
| `Botao` | Variantes: primário (`rubro-500`), secundário (contorno), fantasma, destrutivo (`rubro-700` contorno) |
| `CampoMoeda` | Digitação da direita para a esquerda (`1` `2` `3` `4` → `R$ 12,34`), como toda calculadora e todo ERP brasileiro |
| `CampoDocumento` | Máscara automática CPF/CNPJ pelo tamanho, com validação de dígito |
| `CampoData` | Aceita `10/03`, `1003`, `hoje`, `ontem`, `+7`, `fim do mês` |
| `Busca` | Debounce de 180 ms, resultado em até 60 ms, navegação com setas, `Enter` seleciona |
| `Grade` | Virtualizada, colunas redimensionáveis e reordenáveis, agrupamento, totalizador no rodapé, exportação, largura persistida por usuário |
| `SeletorConta` | Busca por código ou nome, mostra a hierarquia, favoritos recentes primeiro |
| `LinhaDoTempo` | O Rio do Caixa e derivados |
| `Badge` | Estado de documento, contador de pendência |
| `Gaveta` | Painel lateral para detalhe sem perder o contexto da lista |
| `PaletaComandos` | `Ctrl+K` — ir para tela, executar ação, buscar cliente/produto/título |
| `Assistente` | Fluxos de várias etapas (abertura de caixa, fechamento, inventário) |
| `DialogoConflito` | Resolução de edição concorrente ([doc 03 §4.1](03-pilar-resiliencia.md)) |
| `FaixaEstado` | Faixa fina no topo: fiscal, energia, conexão, modo autônomo |

## 8. Atalhos globais

| Tecla | Ação |
|---|---|
| `Ctrl+K` | Paleta de comandos |
| `Ctrl+B` | Recolher/expandir sidebar |
| `Ctrl+1..9` | Ir para a n-ésima entrada da sidebar (`Ctrl+1` = Pulso) |
| `F1` | Ajuda contextual da tela atual |
| `F2` | Confirmar / finalizar (contexto) |
| `F3` | Buscar na tela |
| `F5` | Recarregar dados da tela |
| `Esc` | Fechar gaveta/modal, ou limpar busca |
| `Ctrl+N` | Novo registro no contexto atual |
| `Ctrl+S` | Salvar |
| `Alt+←/→` | Navegar histórico de telas |
| `Ctrl+Shift+D` | Alternar tema |

### PDV (camada própria, ver [modulos/pdv.md](modulos/pdv.md))

| Tecla | Ação |
|---|---|
| `F2` | Finalizar venda |
| `F3` | Buscar produto |
| `F4` | Quantidade |
| `F5` | Desconto (sujeito a limite) |
| `F6` | Identificar cliente |
| `F7` | Cancelar item |
| `F8` | Cancelar cupom (exige supervisor) |
| `F9` | Sangria / suprimento |
| `F10` | Consulta de preço (não abre venda) |
| `F12` | Abrir/fechar caixa |

## 9. Acessibilidade

- Contraste mínimo **AA (4.5:1)** para texto; testado em CI com `xtask contraste`.
- Foco sempre visível: anel de 2 px `rubro-500` com deslocamento de 2 px.
- Nenhuma informação transmitida só por cor: entrada/saída têm sinal e ícone além da cor
  (importante — daltonismo vermelho/verde afeta ~8% dos homens, e nossa paleta é vermelha).
- Escala de fonte de 90% a 150% sem quebrar layout.
- Navegação completa por teclado, com ordem de tabulação explícita.
- Alvos de toque de no mínimo 36 px em telas sensíveis.

## 10. Tecnologia

**egui / eframe** com backend `wgpu`. Ver [ADR-0004](adr/0004-egui-como-camada-de-interface.md).

Razões: RAM de 40–90 MB (contra 300+ MB de webview), 0% de CPU ocioso, modo imediato que torna grades
densas e dinâmicas triviais, Rust puro (um só idioma na equipe, um só sistema de build), inicialização
em ~200 ms, e binário único sem runtime a instalar.

O que custa: o design system precisa ser construído por nós — o que este documento resolve —
e não há CSS. Em troca, os tokens ficam tipados e verificados pelo compilador.

```rust
// Tokens são tipos, não strings. Errar uma cor é erro de compilação.
ui.add(Botao::primario("Finalizar venda").atalho(Tecla::F2));
ui.add(Valor::dinheiro(total).tamanho(Escala::Destaque).sinal(Sinal::Automatico));
```

Um console web (Leptos, compartilhando os mesmos tokens exportados como CSS custom properties) está
previsto para acesso remoto — não para a v1.
