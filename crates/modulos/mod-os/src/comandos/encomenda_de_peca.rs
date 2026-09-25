//! Peça sob encomenda: pedir ao fornecedor ([`EncomendarPeca`]) e, quando ela chega, dar
//! entrada, pagar e aplicar na OS num clique só ([`RegistrarChegadaDaPeca`]) — sem nota de
//! compra, sem cadastrar fornecedor, sem passar pela tela de Estoque.
//!
//! Contabilização da chegada: igual a uma compra (`mod-compras`): a entrada no estoque não
//! lança nada sozinha; o título a pagar faz D Estoque / C Fornecedores (e a baixa, quando
//! pago na hora, C Caixa/Banco). A aplicação na OS registra o custo real na peça, que vira o
//! CMV do faturamento. O título nasce com `origem_modulo = "os_peca"` e o item como origem —
//! nunca `"os"`, que é a origem do título a **receber** da OS (desfaturar o procura por ela).

use cardeal_kernel::{Arredondamento, CodigoErro, Data, Dinheiro, Erro, Id, Preco, Resultado};
use cardeal_ledger::PapelConta;
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use mod_estoque::{registrar_entrada_comum, DadosEntrada};
use mod_financeiro::{
    baixar_pagamento_comum, lancar_titulo_comum, DadosLancamentoTitulo, EspecieTitulo,
    MeioPagamento,
};
use serde::{Deserialize, Serialize};

use crate::comandos::carregar_ordem;
use crate::erros::ErroOs;
use crate::execucao::{Encomenda, ItemPeca};
use crate::repositorio::RepositorioOs;

/// Marca uma peça do orçamento como pedida ao fornecedor (ou corrige o pedido).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncomendarPeca {
    /// A ordem de serviço.
    pub ordem_servico: Id,
    /// O item de peça.
    pub item_peca: Id,
    /// De quem foi pedida (texto livre).
    pub fornecedor: String,
    /// Custo combinado por unidade, se já souber.
    pub custo_previsto: Option<Preco>,
    /// Quando deve chegar.
    pub previsao_chegada: Option<Data>,
}

impl Comando for EncomendarPeca {
    type Saida = ();
    const PERMISSAO: &'static str = "os.orcamento.montar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let (_, mut item) = carregar_item_pendente(uow, self.ordem_servico, self.item_peca)?;
        let fornecedor = self.fornecedor.trim();
        if fornecedor.is_empty() {
            return Err(Erro::novo(
                CodigoErro::CAMPO_OBRIGATORIO,
                "informe de quem a peça foi pedida",
            ));
        }
        item.encomenda = Some(Encomenda {
            fornecedor: fornecedor.to_owned(),
            custo_previsto: self.custo_previsto,
            previsao_chegada: self.previsao_chegada,
            encomendada_em: item
                .encomenda
                .as_ref()
                .map_or_else(|| ctx.hoje(), |e| e.encomendada_em),
        });
        RepositorioOs::novo(uow).gravar_encomenda(&item)
    }
}

/// Como a peça que chegou foi paga.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum PagamentoDaPeca {
    /// Paga agora (Dinheiro sai do Caixa; Pix/cartão da conta escolhida).
    PagoAgora {
        /// O meio.
        meio_pagamento: MeioPagamento,
        /// A conta bancária, quando não é a padrão do meio.
        conta_origem: Option<Id>,
    },
    /// Fica em Contas a Pagar, vencendo na data dada.
    APrazo {
        /// Vencimento.
        vencimento: Data,
    },
}

/// A peça encomendada chegou: entrada no estoque com o custo real, conta a pagar (ou já
/// paga) e aplicação na OS — tudo numa transação.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrarChegadaDaPeca {
    /// A ordem de serviço.
    pub ordem_servico: Id,
    /// O item de peça.
    pub item_peca: Id,
    /// Quanto custou, por unidade.
    pub custo_unitario: Preco,
    /// De quem (vazio = o da encomenda, se houver).
    pub fornecedor: String,
    /// Como foi paga.
    pub pagamento: PagamentoDaPeca,
}

/// O que [`RegistrarChegadaDaPeca`] devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct ChegadaRegistrada {
    /// O custo total (quantidade × custo unitário).
    pub custo_total: Dinheiro,
    /// O título a pagar gerado.
    pub titulo: Id,
    /// Se já nasceu pago.
    pub pago: bool,
}

impl Comando for RegistrarChegadaDaPeca {
    type Saida = ChegadaRegistrada;
    const PERMISSAO: &'static str = "os.peca.aplicar";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // Mexe no financeiro também: as permissões de lá são conferidas aqui.
        let pago_agora = matches!(self.pagamento, PagamentoDaPeca::PagoAgora { .. });
        if !ctx.concede("financeiro.pagar.criar")
            || (pago_agora && !ctx.concede("financeiro.pagar.baixar"))
        {
            return Err(Erro::novo(
                CodigoErro::SEM_PERMISSAO,
                "sem permissão para lançar a conta a pagar da peça",
            ));
        }
        if self.custo_unitario.unidades_internas() <= 0 {
            return Err(Erro::novo(
                CodigoErro::CAMPO_OBRIGATORIO,
                "informe quanto a peça custou",
            ));
        }
        let (os, mut item) = carregar_item_pendente(uow, self.ordem_servico, self.item_peca)?;
        let fornecedor = match self.fornecedor.trim() {
            "" => item
                .encomenda
                .as_ref()
                .map(|e| e.fornecedor.clone())
                .unwrap_or_default(),
            f => f.to_owned(),
        };

        // 1. Entrada no estoque, no mesmo local de onde a peça sai logo em seguida.
        let local = mod_estoque::melhor_local_de_saida(uow.conexao(), ctx.empresa, item.produto)?
            .ok_or_else(|| Erro::de_dominio(&ErroOs::SemLocalDeEstoque))?;
        registrar_entrada_comum(
            DadosEntrada {
                produto: item.produto,
                local,
                quantidade: item.quantidade,
                custo_unitario: self.custo_unitario,
                origem_modulo: "os",
                origem_id: Some(os.id),
            },
            ctx,
            uow,
        )?;

        // 2. Aplicar na OS (custo real na peça).
        super::aplicar_peca::consumir_e_aplicar(&mut item, os.id, local, None, ctx, uow)?;

        // 3. Conta a pagar (D Estoque / C Fornecedores) e, se pago agora, a baixa.
        let custo_total = Dinheiro::de_total(
            item.quantidade,
            self.custo_unitario,
            Arredondamento::MeioAcima,
        );
        let observacao = if fornecedor.is_empty() {
            format!("Peça da OS #{}", os.numero)
        } else {
            format!("Peça da OS #{} — {fornecedor}", os.numero)
        };
        let titulo =
            lancar_conta_da_peca(item.id, custo_total, observacao, self.pagamento, ctx, uow)?;

        // 4. Guarda de quem veio (para o histórico da peça), mesmo sem encomenda prévia.
        if item.encomenda.is_none() && !fornecedor.is_empty() {
            item.encomenda = Some(Encomenda {
                fornecedor,
                custo_previsto: Some(self.custo_unitario),
                previsao_chegada: None,
                encomendada_em: ctx.hoje(),
            });
            RepositorioOs::novo(uow).gravar_encomenda(&item)?;
        }

        Ok(ChegadaRegistrada {
            custo_total,
            titulo,
            pago: pago_agora,
        })
    }
}

/// Lança a conta a pagar da peça (origem `"os_peca"`, o item) e, se foi paga na hora, a baixa.
/// Devolve o título.
fn lancar_conta_da_peca(
    item: Id,
    custo_total: Dinheiro,
    observacao: String,
    pagamento: PagamentoDaPeca,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<Id> {
    let vencimento = match pagamento {
        PagamentoDaPeca::APrazo { vencimento } => vencimento,
        PagamentoDaPeca::PagoAgora { .. } => ctx.hoje(),
    };
    let titulo = lancar_titulo_comum(
        DadosLancamentoTitulo {
            especie: EspecieTitulo::Pagar,
            contraparte: None,
            valor_total: custo_total,
            emissao: ctx.hoje(),
            parcelas: 1,
            primeiro_vencimento: vencimento,
            intervalo_dias: 0,
            observacao: Some(observacao),
            categoria: None,
            origem_modulo: "os_peca",
            origem_id: Some(item),
            papel_contraparte: PapelConta::Fornecedores,
            papel_resultado: PapelConta::EstoqueMercadorias,
        },
        ctx,
        uow,
    )?;
    if let PagamentoDaPeca::PagoAgora {
        meio_pagamento,
        conta_origem,
    } = pagamento
    {
        let parcela = titulo
            .parcelas
            .first()
            .copied()
            .ok_or_else(|| Erro::nao_encontrado("parcela do título da peça"))?;
        baixar_pagamento_comum(
            parcela,
            custo_total,
            ctx.hoje(),
            meio_pagamento,
            conta_origem,
            ctx,
            uow,
        )?;
    }
    Ok(titulo.titulo)
}

/// A OS (não finalizada) e o item de peça dela, ainda não aplicado.
fn carregar_item_pendente(
    uow: &mut UnidadeDeTrabalho,
    ordem: Id,
    item_peca: Id,
) -> Resultado<(crate::ordem::OrdemServico, ItemPeca)> {
    let os = carregar_ordem(uow, ordem)?;
    os.exigir_nao_finalizada()
        .map_err(|e| Erro::de_dominio(&e))?;
    let item = RepositorioOs::novo(uow)
        .buscar_item_peca(item_peca)?
        .ok_or_else(|| Erro::nao_encontrado("item de peça"))?;
    if item.ordem_servico != os.id {
        return Err(Erro::de_dominio(&ErroOs::ItemNaoPertenceAOrdem));
    }
    if item.aplicada {
        return Err(Erro::de_dominio(&ErroOs::PecaJaAplicada));
    }
    Ok((os, item))
}
