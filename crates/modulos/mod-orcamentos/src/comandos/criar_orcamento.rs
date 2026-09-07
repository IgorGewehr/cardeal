//! Cria um orçamento comercial (cabeçalho + itens iniciais opcionais).

use cardeal_kernel::{Erro, Id, Percentual, Preco, Quantidade, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::comandos::montar_itens;
use crate::orcamento::{Cabecalho, Orcamento};
use crate::repositorio::RepositorioOrcamentos;

/// Um item na carga de criação/edição de itens.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NovoItemOrcamento {
    /// Descrição livre.
    pub descricao: String,
    /// Quantidade.
    pub quantidade: Quantidade,
    /// Unidade (`un`, `h`…), pode ser vazia.
    pub unidade: String,
    /// Preço unitário.
    pub preco_unitario: Preco,
    /// Desconto da linha.
    pub desconto_percentual: Percentual,
}

/// Cria um orçamento em `Rascunho`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriarOrcamento {
    /// Cliente cadastrado, ou `None` para avulso.
    pub cliente: Option<Id>,
    /// Nome do cliente (obrigatório).
    pub cliente_nome: String,
    /// Documento do cliente.
    pub cliente_documento: Option<String>,
    /// Contato do cliente.
    pub cliente_contato: Option<String>,
    /// Assunto.
    pub assunto: String,
    /// Descrição / escopo.
    pub descricao: Option<String>,
    /// Validade em dias a partir de hoje.
    pub validade_dias: u16,
    /// Condições de pagamento.
    pub condicoes_pagamento: Option<String>,
    /// Prazo de entrega.
    pub prazo_entrega: Option<String>,
    /// Observações.
    pub observacoes: Option<String>,
    /// Desconto de cabeçalho.
    pub desconto_percentual: Percentual,
    /// Itens iniciais (pode ser vazio).
    pub itens: Vec<NovoItemOrcamento>,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct OrcamentoCriado {
    /// O orçamento criado.
    pub orcamento: Id,
    /// O número atribuído.
    pub numero: u64,
}

impl Comando for CriarOrcamento {
    type Saida = OrcamentoCriado;
    const PERMISSAO: &'static str = "orcamentos.orcamento.criar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let numero = RepositorioOrcamentos::novo(uow).proximo_numero()?;
        let hoje = ctx.hoje();
        let validade = hoje.mais_dias(i32::from(self.validade_dias));

        let orcamento = Orcamento::novo(
            ctx.empresa,
            numero,
            Cabecalho {
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
            },
            hoje,
            ctx.usuario,
            ctx.agora,
        )
        .map_err(|e| Erro::de_dominio(&e))?;

        let itens = montar_itens(orcamento.id, self.itens).map_err(|e| Erro::de_dominio(&e))?;

        let mut repo = RepositorioOrcamentos::novo(uow);
        repo.inserir_orcamento(&orcamento)?;
        repo.redefinir_itens(orcamento.id, &itens)?;

        Ok(OrcamentoCriado {
            orcamento: orcamento.id,
            numero,
        })
    }
}
