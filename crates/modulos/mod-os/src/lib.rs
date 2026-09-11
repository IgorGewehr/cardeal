//! # mod-os
//!
//! O ciclo completo da assistência técnica: abertura, laudo, orçamento aprovado pelo
//! cliente, execução com peças e mão de obra, e faturamento. Ver `docs/modulos/os.md` para
//! a especificação funcional completa.
//!
//! ## O que este crate contém
//!
//! Domínio puro: [`OrdemServico`]/[`EstadoOs`] (a máquina de estados completa,
//! `Aberta → EmDiagnostico → AguardandoAprovacao → Aprovada → EmExecucao → Concluida →
//! Faturada`, mais `Cancelada`/`Reprovada`), [`LaudoTecnico`], [`ItemPeca`]/[`ItemMaoDeObra`]
//! (o orçamento), [`receituario`] (o lançamento combinado de faturamento — receita de
//! serviço junto do custo das peças aplicadas, na mesma partida dobrada) e [`manifesto`].
//!
//! E a amarração ao motor ([`ModuloOs`]): migrações (`os_ordem_servico`/`os_laudo_tecnico`/
//! `os_item_peca`/`os_item_mao_de_obra`) e treze comandos cobrindo o ciclo inteiro, menos
//! `AcionarGarantia`. Dois deles chamam outro módulo **direto, na mesma transação** — não
//! por evento — porque a garantia que importa aqui é atomicidade, não desacoplamento a
//! qualquer custo (`docs/contratos-internos.md` §7 regra 2):
//! - [`AplicarPeca`] chama `mod_estoque::registrar_saida_comum`.
//! - [`FaturarOrdemServico`] usa `mod_financeiro::ConstrutorTitulo` +
//!   `RepositorioFinanceiro::inserir_titulo` para gravar o título vinculado ao lançamento
//!   combinado que a própria OS monta (ver `src/comandos/faturar_ordem_servico.rs` para o
//!   porquê de não usar `mod_financeiro::lancar_titulo_comum` aqui) — **só quando há
//!   cobrança**: uma OS de garantia/cortesia (`valor_total == 0`) não gera título, e se
//!   também não houve custo de peça, não gera lançamento nenhum.
//!
//! **Pedido explícito do usuário (2026-09-11):** na abertura de OS, só o nome do cliente e o
//! `defeito_relatado` (o que o cliente relatou querer resolver, capturado na recepção) são
//! obrigatórios — `equipamento` virou opcional. `LaudoTecnico::descricao_problema` foi
//! deliberadamente **mantido como está** (não removido/renomeado): a parte técnica
//! (`docs/modulos/os.md` §14 e adiante) que consome `RegistrarLaudo`/`LaudoTecnico` está fora
//! do escopo desta sessão, e o campo não causa dano — só deixa de ser a fonte de verdade do
//! relato do cliente, que agora é `OrdemServico::defeito_relatado`. [`EditarDadosDaOrdem`]
//! (novo) completa/corrige `equipamento` e complementa `defeito_relatado` depois da abertura,
//! em qualquer estado não-terminal.
//!
//! Auditoria de produção (2026-09-06) encontrou e corrigiu três lacunas reais: um reparo em
//! garantia (itens a custo zero) não conseguia sair de `AguardandoAprovacao` porque
//! `enviar_para_aprovacao` exigia `valor_total > 0` — agora exige só ter algum item
//! ([`OrdemServico::itens_orcamento`]); não havia como corrigir um item de orçamento digitado
//! errado sem cancelar a OS inteira ([`RemoverItemOrcamento`], novo); e um laudo não podia
//! ser corrigido depois de registrado (`RegistrarLaudo` agora aceita ser chamado de novo
//! enquanto `EmDiagnostico`, sobrescrevendo o texto).
//!
//! ## O que falta (ver `docs/17-roadmap.md`, Fase 1)
//!
//! `AcionarGarantia`/`Reincidencia` (por isso `ItemPeca::coberto_garantia` nunca é ligado
//! nesta versão — sempre `false`), o vínculo com `agenda.CriarCompromisso` (módulo `agenda`
//! ainda não existe), e as **consultas** paginadas (`OrdensAbertas`,
//! `OrdensAguardandoAprovacao`, `HistoricoDoEquipamento`, `TaxaDeReincidencia`).

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::module_name_repetitions, clippy::must_use_candidate)]
// Ver a mesma justificativa em cardeal-kernel: Erro carrega Detalhes de propósito.
#![allow(clippy::result_large_err)]

mod apontamento;
mod comandos;
mod consultas;
mod erros;
pub mod eventos;
mod execucao;
mod laudo;
mod manifesto;
pub mod migracoes;
mod modulo;
mod ordem;
pub mod receituario;
mod repositorio;

pub use apontamento::ApontamentoDeTempo;
pub use comandos::{
    AbrirOrdemServico, AjustarApontamento, AplicarPeca, ApontamentoIniciado, AprovarOrcamentoOs,
    CancelarOrdemServico, ConcluirExecucao, EditarDadosDaOrdem, EncerrarApontamento,
    EnviarParaAprovacao, FaturarOrdemServico, IniciarApontamento, IniciarExecucao,
    ItemOrcamentoNovo, MontarOrcamentoOs, OrdemServicoAberta, OrdemServicoFaturada,
    PecaFoiAplicada, RegistrarLaudo, RegistrarMaoDeObra, RemoverItemOrcamento, ReprovarOrcamentoOs,
    TipoItemOrcamento,
};
pub use consultas::{
    ApontamentosDaOrdem, BuscarDetalheOrdem, DetalheOrdem, HistoricoDoEquipamento,
    ItemAguardandoEstoque, OrdensAguardandoAprovacao, OrdensEmAberto, PecasAguardandoEstoque,
    TempoPorTecnicoNoPeriodo, TempoTotalDaOrdem,
};
pub use erros::ErroOs;
pub use execucao::{ItemMaoDeObra, ItemPeca};
pub use laudo::LaudoTecnico;
pub use manifesto::{manifesto, MANIFESTO};
pub use modulo::ModuloOs;
pub use ordem::{EstadoOs, OrdemServico};
pub use repositorio::RepositorioOs;
