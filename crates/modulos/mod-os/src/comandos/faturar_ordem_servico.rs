//! Fatura a ordem de serviço concluída: monta o lançamento combinado (receita de serviço +
//! custo das peças aplicadas) e cria o título a receber no financeiro.
//!
//! `docs/modulos/os.md` §5 e §7. Não usa `mod_financeiro::lancar_titulo_comum` — aquele
//! helper monta um lançamento simples (só D contraparte/C resultado); a OS precisa de um
//! lançamento **combinado** (receita + CMV juntos, como o próprio doc exige — "mesmo
//! lançamento", não dois). Por isso a OS monta o lançamento com o próprio
//! [`crate::receituario::faturar_ordem_servico`] e reaproveita só as peças de mais baixo
//! nível do financeiro (`ConstrutorTitulo`, `RepositorioFinanceiro::inserir_titulo`) para
//! gravar o título vinculado a esse mesmo lançamento — nunca cria um segundo.

use cardeal_kernel::{Data, Dinheiro, Erro, Id, Resultado};
use cardeal_ledger::{
    Contas, Contraparte as ContraparteRazao, PapelConta, Razao, RepositorioRazao,
};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use mod_financeiro::{
    baixar_recebimento_comum, ConstrutorTitulo, EspecieTitulo, MeioPagamento, RepositorioFinanceiro,
};
use serde::{Deserialize, Serialize};

use crate::comandos::{autoria_de, carregar_ordem};
use crate::eventos::OrdemFaturada;
use crate::execucao::ItemPeca;
use crate::receituario::{faturar_ordem_servico, ContasFaturamento};
use crate::repositorio::RepositorioOs;

/// Se/como a OS já foi paga no ato de faturar — o caso comum no balcão: cliente paga na
/// retirada, fatura e recebe são o mesmo clique. `None` em `FaturarOrdemServico::pago_no_ato`
/// = só gera o título a receber, sem baixar (paga depois, pela tela de Financeiro).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PagamentoNoAto {
    /// Dinheiro, Pix ou cartão — decide Caixa vs. Bancos (`MeioPagamento::papel`).
    pub meio_pagamento: MeioPagamento,
    /// Uma conta bancária específica, por fora do padrão de `meio_pagamento` (quando a
    /// empresa tem mais de uma). `None` = usa o padrão do meio de pagamento.
    pub conta_destino: Option<Id>,
}

/// Fatura a ordem de serviço.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct FaturarOrdemServico {
    /// A ordem de serviço.
    pub ordem_servico: Id,
    /// Em quantas parcelas dividir o título a receber gerado (1 = à vista). Ignorado quando
    /// `valor_total` é zero (cortesia/garantia) — nenhum título nasce nesse caso.
    pub parcelas: u16,
    /// Vencimento da primeira parcela.
    pub primeiro_vencimento: Data,
    /// Dias entre parcelas consecutivas — ignorado quando `parcelas == 1`.
    pub intervalo_dias: i32,
    /// Quando `Some`, a baixa é dada na mesma transação, pelo valor cheio da OS — pedido
    /// explícito do usuário (2026-09-15): "ao faturar ele pede o método de pagamento". Força
    /// 1 parcela à vista — `parcelas`/`primeiro_vencimento`/`intervalo_dias` são ignorados
    /// nesse caso (pago agora e parcelado são conceitos incompatíveis). Ignorado (sem baixa)
    /// quando `valor_total` é zero — nada para baixar.
    pub pago_no_ato: Option<PagamentoNoAto>,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct OrdemServicoFaturada {
    /// O lançamento gerado no razão, se houve algo a lançar (nenhum, se a OS foi cortesia:
    /// sem cobrança e sem peça aplicada).
    pub lancamento: Option<Id>,
    /// O título a receber gerado no financeiro, se houve cobrança (`valor_total > 0`).
    pub titulo: Option<Id>,
    /// O total faturado.
    pub valor_total: Dinheiro,
    /// Verdadeiro se `titulo` já nasceu com baixa completa (`pago_no_ato`). Irrelevante
    /// (`false`) quando `titulo` é `None`.
    pub titulo_pago: bool,
}

impl Comando for FaturarOrdemServico {
    type Saida = OrdemServicoFaturada;
    const PERMISSAO: &'static str = "os.faturar";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let mut os = carregar_ordem(uow, self.ordem_servico)?;
        let itens_peca = RepositorioOs::novo(uow).itens_peca_da_ordem(os.id)?;

        // 2. Validar (domínio puro): transiciona Concluida -> Faturada.
        os.faturar().map_err(|e| Erro::de_dominio(&e))?;
        let total_custo_pecas: Dinheiro = itens_peca.iter().map(ItemPeca::total_custo).sum();

        // 3. Resolver contas e montar o lançamento combinado (receita + CMV).
        let contas = {
            let repo = RepositorioRazao::novo(uow);
            let resolvedor = Contas::nova(&repo, ctx.empresa);
            ContasFaturamento {
                clientes_a_receber: resolvedor
                    .papel(PapelConta::ClientesAReceber)
                    .map_err(|e| Erro::de_dominio(&e))?,
                receita_servicos: resolvedor
                    .papel(PapelConta::ReceitaServicos)
                    .map_err(|e| Erro::de_dominio(&e))?,
                custo_servico: if total_custo_pecas.e_positivo() {
                    resolvedor
                        .papel(PapelConta::CustoServico)
                        .map_err(|e| Erro::de_dominio(&e))?
                } else {
                    Id::NULO
                },
                estoque: if total_custo_pecas.e_positivo() {
                    resolvedor
                        .papel(PapelConta::EstoqueMercadorias)
                        .map_err(|e| Erro::de_dominio(&e))?
                } else {
                    Id::NULO
                },
            }
        };
        let lancamento_balanceado =
            faturar_ordem_servico(&os, total_custo_pecas, contas, autoria_de(ctx));

        let lancamento = match lancamento_balanceado {
            Some(lb) => {
                let mut repo = RepositorioRazao::novo(uow);
                Some(Razao::registrar(&mut repo, lb).map_err(|e| Erro::de_dominio(&e))?)
            }
            // Nada a lançar: sem cobrança e sem custo de peça — um serviço cortesia.
            None => None,
        };

        // 4. Persistir: título a receber vinculado a ESTE lançamento, só se houve cobrança
        // (uma OS de garantia/cortesia sem cobrança não gera título nenhum).
        let (titulo, titulo_pago) = if os.valor_total.e_positivo() {
            let lancamento_id = lancamento.expect("valor_total positivo sempre gera um lançamento");
            // Pago no ato e parcelado são conceitos incompatíveis (recebido tudo agora não
            // parcela) — quando `pago_no_ato` está presente, força 1 parcela à vista, o
            // parcelamento pedido (se algum) é ignorado.
            let (parcelas, primeiro_vencimento, intervalo_dias) = if self.pago_no_ato.is_some() {
                (1, ctx.hoje(), 0)
            } else {
                (self.parcelas, self.primeiro_vencimento, self.intervalo_dias)
            };
            let mut tcp = ConstrutorTitulo::novo(
                ctx.empresa,
                EspecieTitulo::Receber,
                Some(ContraparteRazao::Cliente(os.cliente)),
                os.valor_total,
                ctx.hoje(),
            )
            .origem("os", Some(os.id))
            .parcelas(parcelas, primeiro_vencimento, intervalo_dias)
            .observacao(format!("Ordem de serviço #{}", os.numero))
            .construir()
            .map_err(|e| Erro::de_dominio(&e))?;
            // Um único lançamento combinado (receita + CMV) cobre o valor_total inteiro —
            // toda parcela derivada dele referencia o mesmo lançamento de origem, não um por
            // parcela (diferente do caminho genérico de `mod_financeiro::lancar_titulo_comum`).
            for parcela in &mut tcp.parcelas {
                parcela.lancamento = Some(lancamento_id);
            }
            RepositorioFinanceiro::novo(uow).inserir_titulo(&tcp)?;

            let pago = if let Some(pagamento) = self.pago_no_ato {
                let primeira_parcela = tcp
                    .parcelas
                    .first()
                    .expect("ConstrutorTitulo sempre grava ao menos uma parcela")
                    .id;
                baixar_recebimento_comum(
                    primeira_parcela,
                    os.valor_total,
                    ctx.hoje(),
                    pagamento.meio_pagamento,
                    pagamento.conta_destino,
                    ctx,
                    uow,
                )?;
                true
            } else {
                false
            };
            (Some(tcp.titulo.id), pago)
        } else {
            (None, false)
        };
        RepositorioOs::novo(uow).atualizar_ordem(&os)?;

        // 5. Publicar.
        uow.publicar(OrdemFaturada {
            ordem_servico: os.id,
            cliente: os.cliente,
            valor_total: os.valor_total,
            lancamento,
            titulo,
        })
        .map_err(|e| Erro::de_dominio(&e))?;

        Ok(OrdemServicoFaturada {
            lancamento,
            titulo,
            valor_total: os.valor_total,
            titulo_pago,
        })
    }
}
