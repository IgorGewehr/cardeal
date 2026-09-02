// Gera os Cargo.toml e os stubs iniciais de cada crate do workspace.
// Uso: node scripts/scaffold.mjs
import { mkdirSync, writeFileSync, existsSync } from "node:fs";
import { join, dirname } from "node:path";

const raiz = join(import.meta.dirname, "..");

function escrever(rel, conteudo) {
  const caminho = join(raiz, rel);
  mkdirSync(dirname(caminho), { recursive: true });
  writeFileSync(caminho, conteudo, "utf8");
  console.log("  " + rel);
}

const cab = (nome, desc) => `[package]
name         = "${nome}"
description  = "${desc}"
version.workspace      = true
edition.workspace      = true
rust-version.workspace = true
license.workspace      = true
authors.workspace      = true
repository.workspace   = true
`;

// ── crates do núcleo e da borda ───────────────────────────────────────────────
const crates = [
  {
    n: "cardeal-kernel",
    d: "Tipos fundamentais do Cardeal: dinheiro, quantidade, preço, identidade, tempo, documentos.",
    deps: `serde.workspace = true
uuid.workspace = true
thiserror.workspace = true
smallvec.workspace = true
jiff.workspace = true

[dev-dependencies]
proptest.workspace = true
`,
  },
  {
    n: "cardeal-ledger",
    d: "O Razão: plano de contas, lançamentos de partidas dobradas e consultas financeiras.",
    deps: `cardeal-kernel.workspace = true
serde.workspace = true
thiserror.workspace = true
smallvec.workspace = true
tracing.workspace = true

[dev-dependencies]
proptest.workspace = true
`,
  },
  {
    n: "cardeal-storage",
    d: "Persistência SQLite: escritor único com group commit, migrações, outbox e backup.",
    deps: `cardeal-kernel.workspace = true
rusqlite.workspace = true
serde.workspace = true
postcard.workspace = true
thiserror.workspace = true
crossbeam-channel.workspace = true
parking_lot.workspace = true
smallvec.workspace = true
tracing.workspace = true
blake3.workspace = true
zstd.workspace = true

[dev-dependencies]
tempfile.workspace = true
proptest.workspace = true
`,
  },
  {
    n: "cardeal-modkit",
    d: "Sistema de módulos: manifesto, registro, permissões, menu e despacho de comandos.",
    deps: `cardeal-kernel.workspace = true
cardeal-ledger.workspace = true
cardeal-storage.workspace = true
serde.workspace = true
thiserror.workspace = true
indexmap.workspace = true
ahash.workspace = true
tracing.workspace = true
`,
  },
  {
    n: "cardeal-auth",
    d: "Usuários, papéis, permissões efetivas, sessões, dispositivos e CA interna.",
    deps: `cardeal-kernel.workspace = true
cardeal-storage.workspace = true
serde.workspace = true
thiserror.workspace = true
argon2.workspace = true
subtle.workspace = true
zeroize.workspace = true
rand.workspace = true
blake3.workspace = true
rcgen.workspace = true
tracing.workspace = true
`,
  },
  {
    n: "cardeal-protocol",
    d: "Contratos de comando, consulta e evento compartilhados entre servidor e cliente.",
    deps: `cardeal-kernel.workspace = true
serde.workspace = true
postcard.workspace = true
thiserror.workspace = true
smallvec.workspace = true
`,
  },
  {
    n: "cardeal-fiscal",
    d: "Cliente da API fiscal externa: emissão, contingência, DFe e SPED.",
    deps: `cardeal-kernel.workspace = true
cardeal-storage.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
reqwest.workspace = true
tokio.workspace = true
tracing.workspace = true
`,
  },
  {
    n: "cardeal-analytics",
    d: "Projeção colunar do razão para Parquet e consultas analíticas.",
    deps: `cardeal-kernel.workspace = true
cardeal-storage.workspace = true
serde.workspace = true
thiserror.workspace = true
tracing.workspace = true
`,
  },
  {
    n: "cardeal-cliente",
    d: "SDK do cliente: conexão, cache local, fila de saída e modo autônomo.",
    deps: `cardeal-kernel.workspace = true
cardeal-protocol.workspace = true
cardeal-storage.workspace = true
serde.workspace = true
postcard.workspace = true
thiserror.workspace = true
tokio.workspace = true
reqwest.workspace = true
parking_lot.workspace = true
tracing.workspace = true
`,
  },
  {
    n: "cardeal-ui",
    d: "Design system Rubro: tokens, tipografia e componentes sobre egui.",
    deps: `cardeal-kernel.workspace = true
egui.workspace = true
egui_extras.workspace = true
serde.workspace = true
tracing.workspace = true
`,
  },
  {
    n: "cardeal-testkit",
    d: "Ferramentas de teste: motor em memória, relógio controlado e asserções de razão.",
    deps: `cardeal-kernel.workspace = true
cardeal-ledger.workspace = true
cardeal-storage.workspace = true
cardeal-modkit.workspace = true
rusqlite.workspace = true
tempfile.workspace = true
parking_lot.workspace = true
`,
  },
];

console.log("crates de núcleo/borda:");
for (const c of crates) {
  escrever(`crates/${c.n}/Cargo.toml`, `${cab(c.n, c.d)}\n[lints]\nworkspace = false\n\n[dependencies]\n${c.deps}`);
}

// ── binários ─────────────────────────────────────────────────────────────────
escrever(
  "crates/cardeal-server/Cargo.toml",
  `${cab("cardeal-server", "O motor do Cardeal: despacho, persistência, HTTP/WS e agendador.")}
[[bin]]
name = "cardeal-server"
path = "src/main.rs"

[dependencies]
cardeal-kernel.workspace    = true
cardeal-ledger.workspace    = true
cardeal-storage.workspace   = true
cardeal-modkit.workspace    = true
cardeal-auth.workspace      = true
cardeal-protocol.workspace  = true
cardeal-fiscal.workspace    = true
cardeal-analytics.workspace = true

mod-financeiro.workspace  = true
mod-clientes.workspace    = true
mod-estoque.workspace     = true
mod-vendas.workspace      = true
mod-pdv.workspace         = true
mod-compras.workspace     = true
mod-crm.workspace         = true
mod-agenda.workspace      = true
mod-os.workspace          = true
mod-alugueis.workspace    = true
mod-combustivel.workspace = true
mod-hotelaria.workspace   = true
mod-industria.workspace   = true
mod-contabil.workspace    = true

axum.workspace       = true
tower.workspace      = true
tower-http.workspace = true
tokio.workspace      = true
rustls.workspace     = true
mdns-sd.workspace    = true
serde.workspace      = true
serde_json.workspace = true
postcard.workspace   = true
toml.workspace       = true
clap.workspace       = true
thiserror.workspace  = true
anyhow.workspace     = true
tracing.workspace    = true
tracing-subscriber.workspace = true
tracing-appender.workspace   = true
mimalloc.workspace   = true

[dev-dependencies]
cardeal-testkit.workspace = true
`,
);

escrever(
  "crates/cardeal-desktop/Cargo.toml",
  `${cab("cardeal-desktop", "Aplicativo desktop do Cardeal: shell, navegação e telas.")}
[[bin]]
name = "cardeal-desktop"
path = "src/main.rs"

[features]
default = ["motor-embutido"]
# Sobe o motor como thread do próprio processo (modo monoposto).
motor-embutido = ["dep:cardeal-server"]

[dependencies]
cardeal-kernel.workspace   = true
cardeal-ui.workspace       = true
cardeal-cliente.workspace  = true
cardeal-protocol.workspace = true
cardeal-server = { workspace = true, optional = true }

mod-financeiro = { workspace = true, features = ["ui"] }
mod-clientes   = { workspace = true, features = ["ui"] }
mod-estoque    = { workspace = true, features = ["ui"] }
mod-vendas     = { workspace = true, features = ["ui"] }
mod-pdv        = { workspace = true, features = ["ui"] }

eframe.workspace      = true
egui.workspace        = true
egui_extras.workspace = true
tokio.workspace       = true
serde.workspace       = true
clap.workspace        = true
anyhow.workspace      = true
tracing.workspace     = true
tracing-subscriber.workspace = true
`,
);

escrever(
  "xtask/Cargo.toml",
  `${cab("xtask", "Tarefas de build e manutenção do Cardeal.")}
publish = false

[[bin]]
name = "xtask"
path = "src/main.rs"

[dependencies]
clap.workspace     = true
anyhow.workspace   = true
walkdir.workspace  = true
toml.workspace     = true
serde.workspace    = true
serde_json.workspace = true
`,
);

// ── módulos de negócio ───────────────────────────────────────────────────────
const modulos = [
  ["mod-financeiro", "Caixa, contas a pagar e a receber, bancos, conciliação e projeção de fluxo.",
   ["caixa", "receber", "pagar", "bancos", "conciliacao", "projecao", "centro_custo", "cobranca", "cheques", "dre"]],
  ["mod-clientes", "Cadastro único de pessoas: clientes, fornecedores, transportadoras.",
   ["credito", "carteira", "deduplicacao"]],
  ["mod-estoque", "Produtos, saldos, movimentos, custo médio, lotes e inventário.",
   ["saldo", "custo_medio", "lote", "validade", "inventario", "transferencia", "multi_local", "tanques", "grade"]],
  ["mod-vendas", "Orçamento, pedido, tabela de preço, comissão e devolução.",
   ["orcamento", "pedido", "tabela_preco", "comissao", "entrega", "devolucao", "contrato_recorrente"]],
  ["mod-pdv", "Frente de caixa: venda rápida, sessão de caixa, sangria e modo autônomo.",
   ["frente_caixa", "sangria", "turno", "tef", "balanca", "pista", "delivery"]],
  ["mod-compras", "Cotação, pedido de compra, entrada por XML e rateio de despesas.",
   ["cotacao", "pedido", "entrada_xml", "rateio", "devolucao"]],
  ["mod-crm", "Funil, oportunidades, atividades e histórico do cliente.", ["funil", "atividade", "meta"]],
  ["mod-agenda", "Compromissos, recursos e disponibilidade.", ["recurso", "lembrete"]],
  ["mod-os", "Ordem de serviço: laudo, orçamento, execução e garantia.", ["laudo", "garantia", "checklist"]],
  ["mod-alugueis", "Contratos de locação, períodos, devolução e faturamento recorrente.",
   ["contrato", "caucao", "avaria"]],
  ["mod-combustivel", "Bombas, tanques, encerrantes, aferição e LMC.", ["pista", "tanque", "lmc", "aferição"]],
  ["mod-hotelaria", "Mapa de quartos, reservas, folio e diárias.", ["reserva", "folio", "tarifario"]],
  ["mod-industria", "Ficha técnica, ordem de produção e apontamento.", ["ficha_tecnica", "producao", "rastreio"]],
  ["mod-contabil", "Mapeamento contábil e exportação ECD/ECF.", ["mapeamento", "exportacao"]],
];

console.log("módulos:");
for (const [nome, desc, subs] of modulos) {
  const feats = subs.map((s) => `${s} = []`).join("\n");
  escrever(
    `crates/modulos/${nome}/Cargo.toml`,
    `${cab(nome, desc)}
[features]
default = []
# Telas do módulo (compiladas apenas no desktop).
ui = ["dep:cardeal-ui", "dep:egui"]
# Submódulos — ver docs/04-pilar-modularidade.md §2.1
${feats}
completo = [${subs.map((s) => `"${s}"`).join(", ")}]

[dependencies]
cardeal-kernel.workspace  = true
cardeal-ledger.workspace  = true
cardeal-storage.workspace = true
cardeal-modkit.workspace  = true
cardeal-ui = { workspace = true, optional = true }
egui       = { workspace = true, optional = true }
serde.workspace     = true
thiserror.workspace = true
smallvec.workspace  = true
rusqlite.workspace  = true
tracing.workspace   = true

[dev-dependencies]
cardeal-testkit.workspace = true
proptest.workspace        = true
`,
  );
}

console.log("\nPronto.");
