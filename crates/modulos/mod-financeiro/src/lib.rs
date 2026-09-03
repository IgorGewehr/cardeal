//! # mod-financeiro
//!
//! O substrato de dinheiro do Cardeal: caixa, contas a receber e a pagar, bancos,
//! conciliação, projeção de fluxo e o Pulso. Ver `docs/modulos/financeiro.md` para a
//! especificação funcional completa.
//!
//! > O financeiro **não implementa o Razão** (isso é `cardeal-ledger`). Ele é o principal
//! > operador dele: cada título lançado, cada baixa, cada sessão de caixa vira partida
//! > dobrada no mesmo instante, pelo receituário de `docs/modulos/financeiro.md` §7.
//!
//! ## O que este crate contém agora
//!
//! O **domínio puro**, sem banco nem `async` (`docs/19-estado-e-processo.md` §3.2):
//!
//! - [`manifesto`] — a identidade declarativa do módulo (submódulos, permissões, menu,
//!   contas requeridas, eventos), validada por [`Manifesto::validar`](cardeal_modkit::Manifesto::validar).
//! - [`ConstrutorTitulo`] → [`TituloComParcelas`] — monta título e parcelas com rateio que
//!   **conserva o total ao centavo** (`docs/modulos/financeiro.md` §11.2).
//! - [`Parcela::situacao_em`] / [`Parcela::planejar_baixa`] — juros, multa e desconto
//!   **calculados na leitura**, nunca materializados fora da baixa (§11.1).
//! - [`SessaoCaixa`] — abertura com suprimento, suprimento/sangria e **fechamento cego**
//!   com apuração de quebra (§11.3), cada transição produzindo seu lançamento.
//! - [`Recorrencia`] — a regra que **projeta ocorrências sem gravar linha** (§11.7):
//!   enumera vencimentos numa janela e materializa um só quando entra na antecedência.
//! - [`receituario`] — funções que transformam esses eventos em
//!   [`LancamentoBalanceado`](cardeal_ledger::LancamentoBalanceado).
//!
//! ## O que falta (ver `docs/17-roadmap.md`, Fase 1)
//!
//! Os **comandos** (`AbrirCaixa`, `LancarTitulo`, `BaixarParcela`…) e **consultas** paginadas,
//! que dependem de `cardeal-storage` (a `UnidadeDeTrabalho` e a trait `Comando` de
//! `cardeal-modkit`, ver `docs/contratos-internos.md` §4); a **conciliação bancária**, a
//! **projeção de fluxo** (agrega recorrência + parcelas + Razão) e o **Pulso**. As
//! assinaturas dos comandos estão em `docs/modulos/financeiro.md` §5.

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::module_name_repetitions, clippy::must_use_candidate)]
// Ver a mesma justificativa em cardeal-kernel: Erro carrega Detalhes de propósito.
#![allow(clippy::result_large_err)]

mod baixa;
mod caixa;
mod erros;
mod manifesto;
pub mod receituario;
mod recorrencia;
mod titulo;

pub use baixa::{PlanoBaixa, SituacaoParcela};
pub use caixa::{
    AberturaCaixa, Caixa, ContasCaixa, EstadoSessao, FechamentoCaixa, MovimentoCaixa, SessaoCaixa,
    TipoMovimento, TOLERANCIA_QUEBRA,
};
pub use erros::ErroFinanceiro;
pub use manifesto::{manifesto, MANIFESTO};
pub use receituario::Autoria;
pub use recorrencia::{Periodicidade, Recorrencia, TipoValor};
pub use titulo::{
    ConstrutorTitulo, EspecieTitulo, EstadoParcela, FormaCobranca, Parcela, PoliticaJuros, Titulo,
    TituloComParcelas,
};
