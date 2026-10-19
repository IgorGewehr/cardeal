//! Finaliza a venda (`F2`): valida a soma dos pagamentos contra o total, consome estoque item
//! a item, monta o lançamento combinado (um débito por forma de pagamento + desconto + CMV) e
//! publica o evento. `docs/modulos/pdv.md` §5, §7 e §11 regra 5.
//!
//! **Não emite documento fiscal** — isso é responsabilidade assíncrona de `mod-fiscal`
//! (`docs/10-modulo-fiscal.md` §3), que ainda não existe implementado; `Cupom` não tem sequer
//! o campo `documento_fiscal` nesta fatia (ver nota em `src/migracoes.rs`). Só suporta as
//! formas de pagamento com liquidação imediata (`FormaPagamentoPdv`) — venda "na carteira" (a
//! prazo) fica para quando existir um jeito de gerar o `Titulo` correspondente sem duplicar
//! `mod-vendas::FaturarPedido`.

use cardeal_kernel::{Dinheiro, Id};
#[cfg(feature = "sqlite")]
use cardeal_kernel::{Erro, Resultado};
#[cfg(feature = "sqlite")]
use cardeal_ledger::PapelConta;
#[cfg(feature = "sqlite")]
use cardeal_ledger::RepositorioRazao;
#[cfg(feature = "sqlite")]
use cardeal_ledger::{Contas, Razao};
use cardeal_modkit::Comando;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::Risco;
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
#[cfg(feature = "sqlite")]
use mod_estoque::{registrar_saida_comum, DadosSaida};
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use crate::comandos::{autoria_de, carregar_cupom, limite_desconto, papel_da_forma};
use crate::cupom::FormaPagamentoPdv;
#[cfg(feature = "sqlite")]
use crate::cupom::{validar_pagamentos, PagamentoCupom};
#[cfg(feature = "sqlite")]
use crate::eventos::VendaFinalizada;
#[cfg(feature = "sqlite")]
use crate::receituario::{faturar_cupom, ContasFinalizacao};
#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioPdv;

/// Uma forma de pagamento informada ao finalizar.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PagamentoInformado {
    /// A forma.
    pub forma: FormaPagamentoPdv,
    /// O valor — o troco em dinheiro já vem descontado (calculado na tela, não aqui).
    pub valor: Dinheiro,
    /// A condição, quando a forma é a prazo.
    pub prazo: Option<crate::cupom::CondicaoPrazo>,
}

/// Finaliza a venda.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinalizarVenda {
    /// O cupom.
    pub cupom: Id,
    /// Os pagamentos — a soma precisa fechar exatamente com o total do cupom.
    pub pagamentos: Vec<PagamentoInformado>,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct VendaFoiFinalizada {
    /// O lançamento gerado no razão.
    pub lancamento: Id,
    /// O total pago.
    pub total: Dinheiro,
    /// A conta a receber da parte a prazo, se houve.
    pub titulo_a_receber: Option<Id>,
}

/// A conta de um pagamento, o valor e o cliente (só na parte a prazo).
#[cfg(feature = "sqlite")]
type PagamentoResolvido = (Id, Dinheiro, Option<cardeal_ledger::Contraparte>);

/// As contas do lançamento da venda: receita, desconto, CMV e estoque, e a conta de cada
/// pagamento (a parte a prazo leva o cliente na partida).
#[cfg(feature = "sqlite")]
fn resolver_contas(
    cupom: &crate::cupom::Cupom,
    pagamentos: &[crate::cupom::PagamentoCupom],
    cmv_total: Dinheiro,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<(ContasFinalizacao, Vec<PagamentoResolvido>)> {
    let contas = {
        let repo = RepositorioRazao::novo(uow);
        let resolvedor = Contas::nova(&repo, ctx.empresa);
        ContasFinalizacao {
            receita: resolvedor
                .papel(PapelConta::ReceitaVendas)
                .map_err(|e| Erro::de_dominio(&e))?,
            descontos: if cupom.desconto.e_positivo() {
                resolvedor
                    .papel(PapelConta::DescontosConcedidos)
                    .map_err(|e| Erro::de_dominio(&e))?
            } else {
                Id::NULO
            },
            cmv: if cmv_total.e_positivo() {
                resolvedor
                    .papel(PapelConta::Cmv)
                    .map_err(|e| Erro::de_dominio(&e))?
            } else {
                Id::NULO
            },
            estoque: if cmv_total.e_positivo() {
                resolvedor
                    .papel(PapelConta::EstoqueMercadorias)
                    .map_err(|e| Erro::de_dominio(&e))?
            } else {
                Id::NULO
            },
        }
    };
    let pagamentos_resolvidos = {
        let repo = RepositorioRazao::novo(uow);
        let resolvedor = Contas::nova(&repo, ctx.empresa);
        pagamentos
            .iter()
            .map(|p| {
                let contraparte = (p.forma == crate::cupom::FormaPagamentoPdv::Prazo)
                    .then_some(cupom.cliente)
                    .flatten()
                    .map(cardeal_ledger::Contraparte::Cliente);
                resolvedor
                    .papel(papel_da_forma(p.forma))
                    .map(|conta| (conta, p.valor, contraparte))
                    .map_err(|e| Erro::de_dominio(&e))
            })
            .collect::<Resultado<Vec<_>>>()?
    };
    Ok((contas, pagamentos_resolvidos))
}

/// A parte a prazo vira conta a receber do cliente — parcelas apontando para o lançamento da
/// venda (que já debitou Clientes a receber), sem lançar receita de novo.
#[cfg(feature = "sqlite")]
fn abrir_conta_a_prazo(
    cupom: &crate::cupom::Cupom,
    a_prazo: Option<crate::cupom::PagamentoCupom>,
    lancamento: Id,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<Option<Id>> {
    Ok(match (a_prazo, cupom.cliente) {
        (Some(p), Some(cliente)) => {
            let c = p.prazo.expect("validar_pagamentos exige a condição");
            let mut tcp = mod_financeiro::ConstrutorTitulo::novo(
                ctx.empresa,
                mod_financeiro::EspecieTitulo::Receber,
                Some(cardeal_ledger::Contraparte::Cliente(cliente)),
                p.valor,
                ctx.hoje(),
            )
            .origem("pdv", Some(cupom.id))
            .parcelas(c.parcelas, c.primeiro_vencimento, c.intervalo_dias)
            .observacao(format!("Venda nº {} (PDV)", cupom.numero_terminal))
            .construir()
            .map_err(|e| Erro::de_dominio(&e))?;
            for parcela in &mut tcp.parcelas {
                parcela.lancamento = Some(lancamento);
            }
            mod_financeiro::RepositorioFinanceiro::novo(uow).inserir_titulo(&tcp)?;
            Some(tcp.titulo.id)
        }
        _ => None,
    })
}

/// A venda a prazo precisa de um cliente, com crédito liberado e limite que comporte o valor
/// (o "em aberto" vem do financeiro). Sem limite cadastrado, não há teto.
#[cfg(feature = "sqlite")]
fn exigir_credito(
    cupom: &crate::cupom::Cupom,
    valor: Dinheiro,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<()> {
    let Some(cliente) = cupom.cliente else {
        return Err(Erro::de_dominio(&crate::erros::ErroPdv::PrazoSemCliente));
    };
    let Some(limite) = mod_clientes::limite_credito_da_pessoa(uow.conexao(), cliente)? else {
        return Ok(());
    };
    if limite.situacao == mod_clientes::SituacaoCredito::Bloqueado {
        return Err(Erro::de_dominio(&crate::erros::ErroPdv::CreditoBloqueado(
            limite.motivo_bloqueio.clone().unwrap_or_default(),
        )));
    }
    let em_aberto =
        mod_financeiro::em_aberto_a_receber_do_cliente(uow.conexao(), ctx.empresa, cliente)?;
    let disponivel = limite.disponivel(em_aberto).disponivel;
    if valor > disponivel {
        return Err(Erro::de_dominio(
            &crate::erros::ErroPdv::LimiteDeCreditoExcedido {
                disponivel: disponivel.nao_negativo(),
            },
        ));
    }
    Ok(())
}

impl Comando for FinalizarVenda {
    type Saida = VendaFoiFinalizada;
    const PERMISSAO: &'static str = "pdv.venda.finalizar";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let mut cupom = carregar_cupom(uow, self.cupom)?;
        let rateado = ratear_desconto_no_total(&mut cupom, ctx)?;

        // 2. Validar (domínio puro): soma dos pagamentos e a transição de estado.
        let pagamentos: Vec<PagamentoCupom> = self
            .pagamentos
            .iter()
            .map(|p| PagamentoCupom {
                id: Id::novo(),
                forma: p.forma,
                valor: p.valor,
                prazo: p.prazo,
            })
            .collect();
        validar_pagamentos(&pagamentos, cupom.total).map_err(|e| Erro::de_dominio(&e))?;
        cupom.finalizar().map_err(|e| Erro::de_dominio(&e))?;
        let a_prazo = pagamentos
            .iter()
            .find(|p| p.forma == crate::cupom::FormaPagamentoPdv::Prazo)
            .copied();
        if let Some(p) = a_prazo {
            exigir_credito(&cupom, p.valor, ctx, uow)?;
        }

        // 3. Consumir o estoque item a item, somando o CMV apurado (quem consome é quem
        //    lança — `docs/modulos/estoque.md` §7).
        let mut cmv_total = Dinheiro::ZERO;
        for item in cupom.itens.iter().filter(|i| !i.cancelado) {
            let saida = registrar_saida_comum(
                DadosSaida {
                    produto: item.produto,
                    local: cupom.local_expedicao,
                    quantidade: item.quantidade,
                    origem_modulo: "pdv",
                    origem_id: Some(cupom.id),
                },
                ctx,
                uow,
            )?;
            cmv_total += saida.aplicada.custo_total;
        }

        // Resolver contas e montar o lançamento combinado (pagamentos + desconto + CMV).
        let (contas, pagamentos_resolvidos) =
            resolver_contas(&cupom, &pagamentos, cmv_total, ctx, uow)?;

        let lancamento_balanceado = faturar_cupom(
            &cupom,
            ctx.hoje(),
            &pagamentos_resolvidos,
            cmv_total,
            contas,
            autoria_de(ctx),
        )
        .map_err(|e| Erro::de_dominio(&e))?;

        let lancamento = {
            let mut repo = RepositorioRazao::novo(uow);
            Razao::registrar(&mut repo, lancamento_balanceado).map_err(|e| Erro::de_dominio(&e))?
        };

        // 4. Persistir.
        {
            let mut repo = RepositorioPdv::novo(uow);
            repo.atualizar_cupom(&cupom)?;
            if rateado {
                for item in cupom.itens.iter().filter(|i| !i.cancelado) {
                    repo.atualizar_item(item)?;
                }
            }
            for pagamento in &pagamentos {
                repo.inserir_pagamento(cupom.id, pagamento)?;
            }
        }
        // A parte a prazo vira conta a receber do cliente — parcelas apontando para o
        // lançamento da venda (que já debitou Clientes a receber), sem lançar receita de novo.
        let titulo_a_receber = abrir_conta_a_prazo(&cupom, a_prazo, lancamento, ctx, uow)?;

        // 5. Publicar.
        uow.publicar(VendaFinalizada {
            cupom: cupom.id,
            terminal: cupom.terminal,
            total: cupom.total,
        })
        .map_err(|e| Erro::de_dominio(&e))?;

        Ok(VendaFoiFinalizada {
            lancamento,
            total: cupom.total,
            titulo_a_receber,
        })
    }
}

/// O desconto no total (`Shift+F5`) passa a morar nos itens antes de tudo — a NFC-e, o
/// comprovante e o estorno leem o desconto de cada item. Confere de novo o teto de quem
/// finaliza: um item cancelado depois de um desconto em valor sobe o percentual da venda.
/// Devolve se havia o que ratear (os itens precisam ser regravados).
#[cfg(feature = "sqlite")]
fn ratear_desconto_no_total(cupom: &mut crate::cupom::Cupom, ctx: &Ctx) -> Resultado<bool> {
    if cupom.desconto_no_total.is_none() {
        return Ok(false);
    }
    cupom.recalcular();
    cupom
        .conferir_teto(limite_desconto(ctx))
        .map_err(|e| Erro::de_dominio(&e))?;
    cupom.ratear_desconto_no_total();
    Ok(true)
}
