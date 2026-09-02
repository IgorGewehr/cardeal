# 16 — Onboarding: o primeiro dia

> Meta: em **4 horas** você entende a arquitetura, roda o sistema e faz sua primeira alteração.
> Em **1 semana**, você entrega um módulo simples sozinho.

## Hora 1 — Rodando

```bash
# 1. Toolchain
winget install Rustlang.Rustup           # Windows
rustup toolchain install stable
rustup component add clippy rustfmt

# 2. Clone e build
git clone <repo> cardeal && cd cardeal
cargo build --workspace                  # primeira vez: ~4 min

# 3. Base de demonstração (mercado de bairro, 90 dias de histórico)
cargo xtask semear --cenario mercado

# 4. Suba
cargo run -p cardeal-server              # terminal 1
cargo run -p cardeal-desktop             # terminal 2
```

Entre com `admin` / `admin`. Você cai no **Pulso**.

**Faça agora, nesta ordem** — não pule, é o que constrói o modelo mental:

1. No Pulso, clique num ponto do Rio do Caixa e veja os lançamentos do dia.
2. Abra **PDV**, aperte `F12` para abrir o caixa, venda alguma coisa (`F3` busca, `F2` finaliza).
3. Volte ao **Pulso** e veja o saldo mudar.
4. Vá em **Financeiro → Fluxo de Caixa** e ache a sua venda.
5. Abra o lançamento e veja as partidas: débito no caixa, crédito na receita, débito no CMV,
   crédito no estoque.
6. Vá em **Estoque** e confirme que o saldo do produto caiu.

Você acabou de ver a tese do produto funcionando: **uma ação, um lançamento, três telas coerentes,
zero sincronização**.

## Hora 2 — Lendo

Nesta ordem, sem pular:

1. [doc 00 — Visão de produto](00-visao-produto.md) — 10 min
2. [doc 01 — Arquitetura geral](01-arquitetura-geral.md) — 20 min, com atenção à §3
3. [doc 05 — Núcleo financeiro](05-nucleo-financeiro.md) — 30 min, **o mais importante**
4. [doc 04 — Modularidade](04-pilar-modularidade.md) — 20 min
5. [doc 15 — Convenções](15-convencoes-codigo.md) — 15 min

Depois, abra o código e siga o caminho de uma venda com os olhos:

```
crates/modulos/mod-pdv/src/comandos/finalizar_venda.rs   ← comece aqui
  → crates/modulos/mod-estoque/src/dominio/baixa.rs
  → crates/cardeal-ledger/src/construtor.rs
  → crates/cardeal-storage/src/escritor.rs
```

## Hora 3 — Sua primeira alteração

**Exercício:** adicione o campo "apelido" ao cadastro de cliente e mostre-o na busca do PDV.

```bash
# 1. Migração
cat > crates/modulos/mod-clientes/migracoes/0004_apelido.sql <<'SQL'
ALTER TABLE cliente_pessoa ADD COLUMN apelido TEXT;
CREATE INDEX cliente_pessoa_apelido ON cliente_pessoa(empresa, apelido) WHERE apelido IS NOT NULL;
SQL
```

Depois:
2. Adicione `pub apelido: Option<String>` em `dominio/pessoa.rs`.
3. Inclua no `SELECT` e no `INSERT` de `repositorio.rs`.
4. Adicione o campo no formulário em `telas/cadastro.rs`.
5. Inclua na busca do PDV (`mod-pdv/src/consultas/buscar_cliente.rs`).
6. Escreva o teste em `testes/integracao.rs`.

```bash
cargo test -p mod-clientes
cargo run -p cardeal-server            # a migração aplica sozinha no boot
```

Se levou mais de 40 minutos, algo na arquitetura está mais difícil do que deveria —
**abra uma issue**. Facilidade de alteração é requisito, não sorte.

## Hora 4 — Seu primeiro módulo

```bash
cargo xtask novo-modulo --id fidelidade --nome "Fidelidade"
```

O gerador cria o esqueleto completo. Sua tarefa:

1. **Escreva o `README.md` primeiro.** O que o módulo faz, quais entidades, quais comandos, e —
   obrigatoriamente — **quais lançamentos ele posta no razão**.
2. Preencha `manifesto.rs`: permissões, menu, submódulos, contas requeridas.
3. Modele o domínio em `dominio/` — sem I/O, testável sozinho.
4. Escreva os comandos seguindo as cinco etapas do [doc 15 §3](15-convencoes-codigo.md).
5. Escreva o teste de receituário provando os lançamentos.
6. Registre o módulo em `cardeal-server/src/modulos.rs`.

Exemplo de lançamento para o módulo Fidelidade:

| Evento | Débito | Crédito | Estado |
|---|---|---|---|
| Pontos concedidos na venda | Despesa com fidelidade (5.5) | Provisão de pontos (2.1.x) | Confirmado |
| Pontos resgatados | Provisão de pontos | Receita de vendas | Realizado |
| Pontos expirados | Provisão de pontos | Outras receitas | Realizado |

Note que **até um programa de fidelidade tem contrapartida financeira**. Se você não conseguiu
escrever as linhas dessa tabela, ainda não entendeu o módulo que vai construir.

## Mapa mental do repositório

```
Preciso mexer em...                     Vá para...
─────────────────────────────────────────────────────────────────────
Aritmética de dinheiro                  crates/cardeal-kernel/src/dinheiro.rs
Como um lançamento é montado            crates/cardeal-ledger/src/construtor.rs
Como algo é gravado                     crates/cardeal-storage/src/escritor.rs
Como um módulo se registra              crates/cardeal-modkit/src/registro.rs
Quem pode fazer o quê                   crates/cardeal-auth/src/autorizacao.rs
Cores, botões, grade                    crates/cardeal-ui/src/
A tela inicial (Pulso)                  crates/modulos/mod-financeiro/src/telas/pulso.rs
A tela de venda                         crates/modulos/mod-pdv/src/telas/frente_caixa.rs
Emissão de nota                         crates/cardeal-fiscal/src/
Um relatório novo                       crates/cardeal-analytics/src/
Tarefas de build e manutenção           xtask/src/
```

## Perguntas que todo mundo faz

**"Por que não usar um ORM?"**
Ver [doc 02 §2.2](02-pilar-eficiencia.md). SQL explícito é mais rápido, mais previsível e mais fácil
de otimizar. E o SQL fica todo em um arquivo por módulo — não espalhado em anotações.

**"Por que partidas dobradas? Não dá para ter uma tabela de movimento?"**
Ver [doc 05 §1](05-nucleo-financeiro.md). Resumo: porque é o único jeito de garantir que os números
nunca divergem entre telas, e porque todo relatório vira uma consulta sobre a mesma tabela.

**"Por que SQLite e não Postgres?"**
Ver [ADR-0003](adr/0003-sqlite-como-motor-de-armazenamento.md). Resumo: instalação zero, RAM 60×
menor, backup é copiar um arquivo, e um escritor único elimina uma classe inteira de bugs.

**"Por que um escritor só? Não vira gargalo?"**
78 vendas/s em HDD antigo, 2.900/s em SSD. Uma PME gera 2 por minuto no pico.
Ver [doc 07 §4](07-persistencia-sqlite.md).

**"Por que egui e não uma UI web?"**
Ver [ADR-0004](adr/0004-egui-como-camada-de-interface.md). Resumo: 90 MB contra 400 MB, 0% de CPU
ocioso, um só idioma, um só build.

**"Onde eu coloco a regra X?"**
Se é regra pura do negócio → `dominio/`. Se orquestra várias coisas → `comandos/`.
Se é leitura → `consultas/`. Se é SQL → `repositorio.rs`. Se é de mais de um módulo → o razão ou um
evento. Se não coube em nenhum → provavelmente o desenho está errado; pergunte na revisão.

## Regras culturais

1. **Documentação é parte da entrega.** PR que muda comportamento e não toca em `docs/` é rejeitado.
2. **Especificação antes de código.** O `README.md` do módulo vem primeiro.
3. **Escreva o teste que prova o dinheiro.** Todo efeito financeiro tem teste de receituário.
4. **Se ficou difícil, o desenho está errado.** Dificuldade é sinal, não desafio pessoal.
5. **Pergunte cedo.** Uma pergunta de 5 minutos vale mais que 3 horas de suposição.
