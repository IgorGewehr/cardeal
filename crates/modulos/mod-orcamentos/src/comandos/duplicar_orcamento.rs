//! Clona um orçamento como um novo `Rascunho` — atalho para "faça outro parecido".

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::comandos::{carregar_orcamento, OrcamentoCriado};
use crate::item::ItemOrcamento;
use crate::orcamento::{Cabecalho, Orcamento};
use crate::repositorio::RepositorioOrcamentos;

/// Duplica um orçamento existente.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicarOrcamento {
    /// O orçamento de origem.
    pub orcamento: Id,
}

impl Comando for DuplicarOrcamento {
    type Saida = OrcamentoCriado;
    const PERMISSAO: &'static str = "orcamentos.orcamento.criar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let origem = carregar_orcamento(uow, self.orcamento)?;
        let itens_origem = RepositorioOrcamentos::novo(uow).itens_do_orcamento(origem.id)?;

        let numero = RepositorioOrcamentos::novo(uow).proximo_numero()?;
        let hoje = ctx.hoje();
        let janela = origem.data_emissao.dias_ate(origem.validade).max(1);

        let novo = Orcamento::novo(
            ctx.empresa,
            numero,
            Cabecalho {
                cliente: origem.cliente,
                cliente_nome: origem.cliente_nome.clone(),
                cliente_documento: origem.cliente_documento.clone(),
                cliente_contato: origem.cliente_contato.clone(),
                assunto: origem.assunto.clone(),
                descricao: origem.descricao.clone(),
                validade: hoje.mais_dias(janela),
                condicoes_pagamento: origem.condicoes_pagamento.clone(),
                prazo_entrega: origem.prazo_entrega.clone(),
                observacoes: origem.observacoes.clone(),
                desconto_percentual: origem.desconto_percentual,
            },
            hoje,
            ctx.usuario,
            ctx.agora,
        )
        .map_err(|e| Erro::de_dominio(&e))?;

        let itens: Vec<ItemOrcamento> = itens_origem
            .into_iter()
            .map(|i| ItemOrcamento {
                id: Id::novo(),
                orcamento: novo.id,
                ..i
            })
            .collect();

        let mut repo = RepositorioOrcamentos::novo(uow);
        repo.inserir_orcamento(&novo)?;
        repo.redefinir_itens(novo.id, &itens)?;

        Ok(OrcamentoCriado {
            orcamento: novo.id,
            numero,
        })
    }
}
