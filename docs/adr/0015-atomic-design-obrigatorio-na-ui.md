# ADR-0015 — Atomic Design é obrigatório: telas só compõem `cardeal-ui`, nunca `egui` cru

- **Status:** Aceita
- **Data:** 2026-09-19
- **Decisores:** Equipe de arquitetura
- **Relacionadas:** ADR-0004, ADR-0010

## Contexto

O ADR-0004 escolheu `egui` e decidiu que o design system Rubro seria "uma camada de componentes
própria" sobre ele. O crate `cardeal-ui` foi organizado em camadas de Atomic Design (tokens → atoms →
molecules → organisms), mas **a regra de que as telas devem usar só essa camada nunca foi escrita
como regra** — existia apenas como comentário de módulo no `lib.rs`, e nenhum documento em `docs/`
sequer citava "Atomic Design". Nada a verificava.

Auditoria de 2026-09-19 em `crates/cardeal-desktop/src` (13 mil linhas de telas):

- Os átomos **são** amplamente usados: dezenas de `Rotulo`, `Botao` e `Campo` por tela, `Grade` e
  `Dialogo` nas telas de listagem, zero `TextEdit`, `RichText` ou `ui.label` crus.
- O vazamento está nas **lacunas do design system**, onde a tela improvisa em vez de pedir o
  componente: **60 ocorrências** de `egui` cru em 11 arquivos — 18 × `egui::Frame` (painéis e cartões
  montados à mão; `tela_pdv.rs` repete o mesmo `Frame` 4 vezes), 13 × `egui::Stroke`, 12 ×
  `ui.separator`, 8 × `ui.painter`, 7 × `Color32` literal, 1 × `ui.checkbox`, 1 × `egui::ComboBox`.
- Componentes que existem ficam sem uso: `dialogo_confirmacao` não é chamado por nenhuma tela.

**Atualização (mesma data):** a tela de PDV foi reescrita sobre o design system e saiu da lista
(4 `Frame`, 2 `Stroke` e 1 `separator` a menos → 53 ocorrências restantes). Nasceram `Painel`,
`Divisor`, `Tecla` e `CampoBusca`; `Etiqueta`, `CartaoKpi`, `ItemDeLista` e `Dialogo` ganharam o que
faltava. A catraca registrou a queda sozinha — é exatamente o uso previsto.

O padrão é o esperado quando não há regra: a peça que falta no design system não é criada, é
contornada, e o contorno vira precedente. Cada `Frame` à mão é uma decisão de aparência (cor, raio,
margem, sombra) tomada fora dos tokens — exatamente o que o doc 12 quis impedir.

## Alternativas consideradas

| Opção | Por que não |
|---|---|
| **Só documentar e confiar na revisão** | É o estado atual. A regra nem estava documentada e as violações passaram. Sessões de trabalho assistidas por IA não "lembram" convenções que não estão no repositório nem quebram o build. |
| **Esconder `egui` do `cardeal-desktop` atrás de re-exports de `cardeal-ui`** | Não impede nada: `egui::Frame` continua acessível pelo re-export. As telas precisam de `egui::Ui` e `Layout` de qualquer forma. |
| **`clippy::disallowed_types` / `disallowed_methods`** | Melhor que texto (entende tipos, sem falso positivo), mas não tem catraca: os 60 usos atuais exigiriam 60 `#[allow]` ou um único `allow` que apaga a regra. Boa evolução quando a dívida chegar a zero (ver "Quando revisitar"). |
| **Verificador textual com catraca no `xtask`** *(escolhida)* | Zero dependência, funciona hoje com a dívida existente, e impede que ela cresça. |

## Decisão

**Toda tela do Cardeal é composta exclusivamente por componentes de `cardeal-ui`.** Em
`crates/cardeal-desktop/src` (`tela_*.rs` e `main.rs`):

1. **Composição, não construção.** Uma tela combina atoms, molecules e organisms e faz layout
   (`ui.horizontal`, `ui.columns`, `ui.vertical`, `ui.add_space(Espaco::…)`). Ela **não** desenha:
   sem `egui::Frame`, `Stroke`, `Color32`, `ui.painter`, `ui.separator`, `ui.checkbox`,
   `egui::ComboBox`, `TextEdit`, `RichText`, `Window`, `Grid`, `TableBuilder`, `ui.button`,
   `ui.label`, `ui.heading`, `ui.add_sized`. A lista viva e o componente substituto de cada item
   estão em `xtask/src/verificar_ui.rs` (`PROIBIDOS`).
2. **Faltou o componente? Crie-o em `cardeal-ui`, antes.** Escolha a camada certa
   (atom = uma peça com aparência própria; molecule = composição de atoms; organism = peça de tela),
   dê-lhe uma entrada em `examples/galeria.rs` e só então use-o na tela. Nunca "só dessa vez" — o
   primeiro `Frame` à mão é o que autoriza o segundo.
3. **Só tokens.** Cor vem de `ui.cores()`, espaço de `Espaco`, raio de `Raio`, tipografia de
   `Papel`/`Rotulo`. Nenhum literal de cor, tamanho de fonte ou raio numa tela.
4. **Sentido das dependências:** `tokens ← atoms ← molecules ← organisms ← telas`. Um átomo nunca
   conhece uma molécula; uma molécula nunca conhece um organismo.
5. **Componente existente ganha preferência.** Antes de criar um, procure: `dialogo_confirmacao`,
   `LinhaDeAcao`, `SecaoExpansivel`, `EstadoVazio` já existem. Reinventar um deles é violação.
6. **Exceção justificada:** `// ui-livre: <motivo>` na linha (ou na linha acima). O motivo é
   obrigatório; exceção sem motivo é contada como violação. Espera-se que sejam raras e caibam em
   uma frase (ex.: desenho de um gráfico que ainda não é organismo).

**Imposição — `cargo xtask verificar-ui`**, obrigatório no checklist de PR
([doc 15 §10](../15-convencoes-codigo.md)). É uma **catraca**: `xtask/ui-baseline.toml` registra a
dívida atual por arquivo e por padrão; violação nova falha, dívida paga também falha até ser
registrada (o baseline só desce), e arquivo novo começa com zero permitido.

## Consequências

### Positivas

- Mudar a aparência de "cartão", "painel" ou "divisor" é editar **um** lugar.
- A dívida atual é finita e visível (53 ocorrências hoje, eram 60); cada refatoração de tela a reduz de forma
  mensurável.
- A regra vale igual para pessoas e para IA: falha o comando, não a memória de alguém.
- Força o design system a cobrir o que as telas precisam (painel, divisor, caixa de seleção).

### Negativas

- Uma tela nova que precise de algo inédito passa a exigir primeiro um componente em `cardeal-ui` —
  mais um passo (e uma entrada na galeria) antes da tela.
- O verificador é textual: não vê `use egui::Frame;` seguido de `Frame::none()`, nem constrói o
  padrão por macro. Cobre o caminho comum (`egui::Frame`) e o custo de burlá-lo é deliberado, não
  acidental.

### Mitigações

- Os componentes que faltavam são o **primeiro** trabalho a fazer. `Painel` e `Divisor` já existem e
  cobrem `Frame` + `Stroke` + `separator` (36 das 53 ocorrências restantes); faltam caixa de seleção e
  tile de lista. Migrar uma tela é trocar o `egui` cru por eles e registrar a queda no baseline.
- `--gerar-baseline` recusa qualquer contagem que suba, então o arquivo não vira álibi.

## Quando revisitar

- **Dívida zerada:** trocar o verificador textual por `clippy::disallowed_types` /
  `disallowed_methods` (entende tipos), permitindo o `egui` cru apenas dentro de `cardeal-ui`.
- Se `PROIBIDOS` gerar falsos positivos recorrentes, refinar o padrão em vez de abrir exceções.
