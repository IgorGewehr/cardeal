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
//! E a **amarração ao motor** ([`ModuloFinanceiro`]): manifesto, migrações
//! (`financeiro_titulo`/`financeiro_parcela`/`financeiro_baixa`) e os primeiros comandos —
//! [`LancarTituloAReceber`] e [`BaixarRecebimento`] — cada um nas cinco etapas de
//! `docs/15-convencoes-codigo.md` §3, com o SQL isolado em [`repositorio`] e os eventos em
//! [`eventos`].
//!
//! ## O que falta (ver `docs/17-roadmap.md`, Fase 1)
//!
//! O espelho **a pagar** (`LancarTituloAPagar`/`BaixarPagamento`), os comandos de **caixa**
//! (`AbrirCaixa`/`FecharCaixa`/sangria/suprimento), `EstornarBaixa`, `RenegociarTitulo`, as
//! **consultas** paginadas, a **conciliação bancária**, a **projeção de fluxo** e o
//! **Pulso**. As assinaturas estão em `docs/modulos/financeiro.md` §5.

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::module_name_repetitions, clippy::must_use_candidate)]
// Ver a mesma justificativa em cardeal-kernel: Erro carrega Detalhes de propósito.
#![allow(clippy::result_large_err)]

mod baixa;
mod caixa;
mod comandos;
mod erros;
pub mod eventos;
mod manifesto;
pub mod migracoes;
mod modulo;
pub mod receituario;
mod recorrencia;
mod repositorio;
mod titulo;

pub use baixa::{PlanoBaixa, SituacaoParcela};
pub use caixa::{
    AberturaCaixa, Caixa, ContasCaixa, EstadoSessao, FechamentoCaixa, MovimentoCaixa, SessaoCaixa,
    TipoMovimento, TOLERANCIA_QUEBRA,
};
pub use comandos::{
    BaixarRecebimento, LancarTituloAReceber, RecebimentoBaixado, TituloAReceberLancado,
};
pub use erros::ErroFinanceiro;
pub use manifesto::{manifesto, MANIFESTO};
pub use modulo::ModuloFinanceiro;
pub use receituario::Autoria;
pub use recorrencia::{Periodicidade, Recorrencia, TipoValor};
pub use repositorio::{BaixaGravada, RepositorioFinanceiro};
pub use titulo::{
    ConstrutorTitulo, EspecieTitulo, EstadoParcela, FormaCobranca, Parcela, PoliticaJuros, Titulo,
    TituloComParcelas,
};
