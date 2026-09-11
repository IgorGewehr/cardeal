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
//! - [`CategoriaFinanceira`] — rótulo livre de custo/receita, fora do plano de contas
//!   (§11.9): "Aluguel", "Assinatura `SaaS` — Cliente X". Nunca contabiliza nada sozinho — só
//!   alimenta agregações por categoria/mês (`TotalPorCategoriaNoPeriodo`), a base para
//!   gráficos de custo recorrente, MRR por projeto/cliente e churn.
//! - [`receituario`] — funções que transformam esses eventos em
//!   [`LancamentoBalanceado`](cardeal_ledger::LancamentoBalanceado).
//!
//! E a **amarração ao motor** ([`ModuloFinanceiro`]): manifesto, migrações
//! (`financeiro_titulo`/`financeiro_parcela`/`financeiro_baixa`, desde a v2
//! `financeiro_caixa`/`financeiro_sessao_caixa`/`financeiro_movimento_caixa`, e desde a v3
//! `financeiro_categoria`/`financeiro_recorrencia` + a coluna `financeiro_titulo.categoria`)
//! e os comandos — título/baixa nas duas espécies (receber e pagar, ambos com `categoria`
//! opcional), o ciclo inteiro de caixa (**[`CadastrarCaixa`]**, **[`AbrirCaixa`]**,
//! **[`RegistrarSuprimento`]**, **[`RegistrarSangria`]**, **[`FecharCaixa`]**), os dois de
//! risco Alto que faltavam: **[`EstornarBaixa`]** (reverte o `Realizado` no Razão via
//! `Razao::estornar`, reabre a parcela) e **[`RenegociarTitulo`]** (fecha o saldo em aberto
//! como `Renegociada`, abre um `Titulo` novo com as condições novas — sem lançamento novo,
//! porque a receita/despesa já foi reconhecida na emissão original, ver o doc do próprio
//! comando) — e agora **[`CriarCategoria`]**/**[`CriarRecorrencia`]** (grava a regra; nenhum
//! `Titulo` nasce na hora) com **[`materializar_recorrencias_pendentes`]** pronta para um
//! agendador chamar (não é `Comando`: `docs/modulos/financeiro.md` §5 já descrevia
//! `MaterializarRecorrencia` como "tarefa agendada, sem permissão de usuário", e nenhum
//! crate deste workspace tem agendador ainda — mesmo padrão de
//! `mod_compras::importar_nota_da_sefaz`). Cada comando nas cinco etapas de
//! `docs/15-convencoes-codigo.md` §3, com o SQL isolado em [`repositorio`] e os eventos em
//! [`eventos`]. O saldo que a sangria e o fechamento cego usam não é um contador à parte: é
//! `RepositorioRazao::saldo_realizado` (nova em `cardeal-ledger`) sobre a própria conta do
//! caixa — a mesma fonte de verdade que qualquer relatório vai ler depois.
//!
//! ## O que falta (ver `docs/17-roadmap.md`, Fase 1)
//!
//! A **conciliação bancária**, a **projeção de fluxo** e o **Pulso** (ambos podem reaproveitar
//! `TotalPorCategoriaNoPeriodo` para a faixa "onde o dinheiro está"). As assinaturas estão em
//! `docs/modulos/financeiro.md` §5. `CadastrarCaixa` não estava no §5 original — a decisão
//! tomada e o motivo estão no doc do próprio comando (`src/comandos/cadastrar_caixa.rs`) e em
//! `docs/modulos/financeiro.md` §5/§9.

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::module_name_repetitions, clippy::must_use_candidate)]
// Ver a mesma justificativa em cardeal-kernel: Erro carrega Detalhes de propósito.
#![allow(clippy::result_large_err)]

mod baixa;
mod caixa;
mod categoria;
mod comandos;
mod consultas;
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
pub use categoria::CategoriaFinanceira;
pub use comandos::{
    lancar_titulo_comum, materializar_recorrencias_pendentes, AbrirCaixa, BaixaFoiEstornada,
    BaixarPagamento, BaixarRecebimento, CadastrarCaixa, CaixaCadastrado, CaixaFoiAberto,
    CaixaFoiFechado, CategoriaCriada, ContaBancariaCriada, CriarCategoria, CriarContaBancaria,
    CriarRecorrencia, DadosLancamentoTitulo, EstornarBaixa, FecharCaixa, LancarTituloAPagar,
    LancarTituloAReceber, PagamentoBaixado, RecebimentoBaixado, RecorrenciaCriada,
    RegistrarSangria, RegistrarSuprimento, RenegociarTitulo, SangriaFoiRegistrada,
    SuprimentoFoiRegistrado, TituloAPagarLancado, TituloAReceberLancado, TituloFoiRenegociado,
    TituloGravado,
};
pub use consultas::{
    Caixas, Categorias, ContasDeCaixa, ContasDeResultado, ContasDisponiveis, ExtratoDisponivel,
    ItemCaixa, ItemContaDisponivel, ItemContaResultado, ItemMovimentoDisponivel,
    ItemTituloEmAberto, ItemTotalPorCategoria, Recorrencias, TituloDaOrigem, TitulosAPagarEmAberto,
    TitulosAReceberEmAberto, TotalPorCategoriaNoPeriodo,
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
