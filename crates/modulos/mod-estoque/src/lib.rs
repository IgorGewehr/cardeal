//! # mod-estoque
//!
//! A verdade física do que existe, onde existe e quanto custou. Ver
//! `docs/modulos/estoque.md` para a especificação funcional completa.
//!
//! ## O que este crate contém agora
//!
//! O **domínio puro** (`docs/19-estado-e-processo.md` §3.2):
//!
//! - [`Produto`] / [`Variacao`] / [`validar_gtin`] / [`Unidade`] / [`Conversao`] / [`Lote`]
//!   — cadastro, validação de NCM e de GTIN (dígito verificador mod-10) sem I/O, e a regra
//!   "validade exige lote" (§11.7).
//! - [`SaldoLocal`] / [`custo_medio_movel`] — o **custo médio ponderado móvel** recalculado
//!   só na entrada (§11.1), o saldo que **pode ficar negativo** e é sinalizado, não
//!   bloqueado (§11.2), e a reserva que **não é saída** (§11.3).
//! - [`Inventario`] — máquina de estado com **contagem cega** (§11.5) e geração de ajustes.
//! - [`receituario`] — os lançamentos que o próprio estoque posta (ajuste, perda,
//!   transferência, entrada avulsa — §7); o CMV da venda é lançado pelo módulo que consome.
//! - [`manifesto`] — a identidade declarativa do módulo, validada.
//!
//! E a **amarração ao motor** ([`ModuloEstoque`]): manifesto, migrações
//! (`estoque_grupo_produto`/`estoque_unidade`/`estoque_produto`/`estoque_local`/
//! `estoque_saldo_local`/`estoque_movimento`, e agora v2 adiciona
//! `estoque_produto.codigo_barras`) e os comandos de cadastro (**[`CriarGrupoProduto`]**,
//! **[`CriarUnidade`]**, **[`CriarProduto`]** — aceita `codigo_barras` opcional, validado
//! por [`validar_gtin`] — **[`CriarLocal`]**) e de movimento (**[`RegistrarEntrada`]**,
//! **[`RegistrarSaida`]**, e agora **[`AjustarSaldo`]** — correção manual de saldo fora de
//! inventário, `docs/modulos/estoque.md` §5/§11.8, a única das quatro que **posta no
//! razão** via `receituario::ajuste_manual`, D Estoque/C Outras receitas na sobra ou
//! D Perdas/C Estoque na falta) — o suficiente para o módulo `os` aplicar peça e consumir
//! estoque de verdade. O corpo de `RegistrarEntrada`/`RegistrarSaida` é exposto também como
//! função `pub` (`registrar_entrada_comum`/`registrar_saida_comum`) para outro módulo já
//! dependente chamar direto, na própria transação — mesmo padrão de
//! `mod_financeiro::lancar_titulo_comum`.
//!
//! ## O que falta (ver `docs/17-roadmap.md`, Fase 2)
//!
//! Grade/variação, lote/validade, o fluxo formal de inventário (contagem cega — o domínio
//! [`Inventario`] já existe e é testado, falta o comando), transferência entre locais,
//! curva ABC, perfil tributário, a `PortaCatalogo` que `vendas`/`pdv` vão consumir, e as
//! **consultas** paginadas. Nenhum bloqueia o módulo `os`. `RegistrarEntrada`/
//! `RegistrarSaida` desta fatia **não lançam no razão** — a entrada/saída física em si não
//! tem contrapartida de estoque própria (quem compra lança via `mod_financeiro`, quem
//! consome via o módulo que consome); só `AjustarSaldo` posta, porque não há nenhum outro
//! módulo gerando a contrapartida de uma correção manual.

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::module_name_repetitions, clippy::must_use_candidate)]
#![allow(clippy::result_large_err)]

mod comandos;
mod consultas;
mod erros;
mod inventario;
mod manifesto;
pub mod migracoes;
mod modulo;
mod produto;
pub mod receituario;
mod repositorio;
mod saldo;

pub use comandos::{
    registrar_entrada_comum, registrar_saida_comum, AjustarSaldo, CriarGrupoProduto, CriarLocal,
    CriarProduto, CriarUnidade, DadosEntrada, DadosSaida, EntradaGravada, EntradaRegistrada,
    GrupoProdutoCriado, LocalCriado, ProdutoCriado, RegistrarEntrada, RegistrarSaida, SaidaGravada,
    SaidaRegistrada, SaldoAjustado, TipoLocal, UnidadeCriada,
};
pub use consultas::{
    GruposProduto, ItemGrupoProduto, ItemLocal, ItemProdutoComSaldo, ItemUnidade, Locais,
    ProdutoPorCodigoBarras, ProdutosComSaldo, Unidades,
};
pub use erros::ErroEstoque;
pub use inventario::{AjusteInventario, ContagemItem, EstadoInventario, Inventario};
pub use manifesto::{manifesto, MANIFESTO};
pub use modulo::ModuloEstoque;
pub use produto::{validar_gtin, Conversao, EstadoLote, Lote, Produto, Unidade, Variacao};
pub use repositorio::RepositorioEstoque;
pub use saldo::{custo_medio_movel, Movimento, SaidaAplicada, SaldoLocal, TipoMovimento};
