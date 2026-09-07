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

use cardeal_kernel::{Dinheiro, Erro, Id, Resultado};
use cardeal_ledger::PapelConta;
use cardeal_ledger::{Contas, Razao, RepositorioRazao};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use mod_estoque::{registrar_saida_comum, DadosSaida};
use serde::{Deserialize, Serialize};

use crate::comandos::{autoria_de, carregar_cupom, papel_da_forma};
use crate::cupom::{validar_pagamentos, FormaPagamentoPdv, PagamentoCupom};
use crate::eventos::VendaFinalizada;
use crate::receituario::{faturar_cupom, ContasFinalizacao};
use crate::repositorio::RepositorioPdv;

/// Uma forma de pagamento informada ao finalizar.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PagamentoInformado {
    /// A forma.
    pub forma: FormaPagamentoPdv,
    /// O valor — o troco em dinheiro já vem descontado (calculado na tela, não aqui).
    pub valor: Dinheiro,
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
}

impl Comando for FinalizarVenda {
    type Saida = VendaFoiFinalizada;
    const PERMISSAO: &'static str = "pdv.venda.finalizar";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let mut cupom = carregar_cupom(uow, self.cupom)?;

        // 2. Validar (domínio puro): soma dos pagamentos e a transição de estado.
        let pagamentos: Vec<PagamentoCupom> = self
            .pagamentos
            .iter()
            .map(|p| PagamentoCupom {
                id: Id::novo(),
                forma: p.forma,
                valor: p.valor,
            })
            .collect();
        validar_pagamentos(&pagamentos, cupom.total).map_err(|e| Erro::de_dominio(&e))?;
        cupom.finalizar().map_err(|e| Erro::de_dominio(&e))?;

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
                    resolvedor
                        .papel(papel_da_forma(p.forma))
                        .map(|conta| (conta, p.valor))
                        .map_err(|e| Erro::de_dominio(&e))
                })
                .collect::<Resultado<Vec<_>>>()?
        };

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
            for pagamento in &pagamentos {
                repo.inserir_pagamento(cupom.id, pagamento)?;
            }
        }

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
        })
    }
}
