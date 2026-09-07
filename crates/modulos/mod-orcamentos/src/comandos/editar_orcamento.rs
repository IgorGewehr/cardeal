//! Regrava o cabeçalho de um orçamento — só enquanto `Rascunho`/`Enviado`.

use cardeal_kernel::{Erro, Id, Percentual, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::comandos::carregar_orcamento;
use crate::orcamento::Cabecalho;
use crate::repositorio::RepositorioOrcamentos;

/// Edita o cabeçalho de um orçamento.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditarOrcamento {
    /// O orçamento.
    pub orcamento: Id,
    /// Cliente cadastrado, ou `None` para avulso.
    pub cliente: Option<Id>,
    /// Nome do cliente.
    pub cliente_nome: String,
    /// Documento do cliente.
    pub cliente_documento: Option<String>,
    /// Contato do cliente.
    pub cliente_contato: Option<String>,
    /// Assunto.
    pub assunto: String,
    /// Descrição / escopo.
    pub descricao: Option<String>,
    /// Validade em dias a partir da emissão.
    pub validade_dias: u16,
    /// Condições de pagamento.
    pub condicoes_pagamento: Option<String>,
    /// Prazo de entrega.
    pub prazo_entrega: Option<String>,
    /// Observações.
    pub observacoes: Option<String>,
    /// Desconto de cabeçalho.
    pub desconto_percentual: Percentual,
}

impl Comando for EditarOrcamento {
    type Saida = ();
    const PERMISSAO: &'static str = "orcamentos.orcamento.editar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let mut orcamento = carregar_orcamento(uow, self.orcamento)?;
        let validade = orcamento.data_emissao.mais_dias(i32::from(self.validade_dias));

        orcamento
            .editar_cabecalho(Cabecalho {
                cliente: self.cliente,
                cliente_nome: self.cliente_nome,
                cliente_documento: self.cliente_documento,
                cliente_contato: self.cliente_contato,
                assunto: self.assunto,
                descricao: self.descricao,
                validade,
                condicoes_pagamento: self.condicoes_pagamento,
                prazo_entrega: self.prazo_entrega,
                observacoes: self.observacoes,
                desconto_percentual: self.desconto_percentual,
            })
            .map_err(|e| Erro::de_dominio(&e))?;

        RepositorioOrcamentos::novo(uow).atualizar_orcamento(&orcamento)?;
        Ok(())
    }
}
