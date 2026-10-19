//! As consultas da transferência: o histórico de transferências e o saldo de um produto em
//! cada local (o "disponível na origem" da tela).

use cardeal_kernel::{Dinheiro, Id, Instante, Quantidade};
use cardeal_modkit::Consulta;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;

/// Um produto de uma transferência registrada.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemTransferido {
    /// O produto.
    pub produto: Id,
    /// O nome.
    pub nome: String,
    /// A sigla da unidade.
    pub unidade: String,
    /// A quantidade.
    pub quantidade: Quantidade,
    /// O valor, ao custo médio da origem na hora.
    pub valor: Dinheiro,
}

/// Uma transferência registrada.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumoTransferencia {
    /// A transferência.
    pub id: Id,
    /// O local de origem (nome).
    pub origem: String,
    /// O local de destino (nome).
    pub destino: String,
    /// A observação.
    pub observacao: Option<String>,
    /// Quando.
    pub criado_em: Instante,
    /// Os itens.
    pub itens: Vec<ItemTransferido>,
}

impl ResumoTransferencia {
    /// O valor total, ao custo da origem.
    #[must_use]
    pub fn valor(&self) -> Dinheiro {
        self.itens
            .iter()
            .fold(Dinheiro::ZERO, |total, i| total + i.valor)
    }
}

/// As transferências do CNPJ, da mais recente para a mais antiga (até 100).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Transferencias;

impl Consulta for Transferencias {
    type Saida = Vec<ResumoTransferencia>;
    const PERMISSAO: &'static str = "estoque.transferencia.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        Ok(
            crate::repositorio_transferencia::listar(conexao, ctx.empresa, 100)?
                .into_iter()
                .map(|t| ResumoTransferencia {
                    id: t.id,
                    origem: t.origem,
                    destino: t.destino,
                    observacao: t.observacao,
                    criado_em: t.criado_em,
                    itens: t
                        .itens
                        .into_iter()
                        .map(|i| ItemTransferido {
                            produto: i.produto,
                            nome: i.nome,
                            unidade: i.unidade,
                            quantidade: i.quantidade,
                            valor: i.valor,
                        })
                        .collect(),
                })
                .collect(),
        )
    }
}

/// O saldo de um produto num local.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaldoNoLocal {
    /// O local.
    pub local: Id,
    /// O nome do local.
    pub nome: String,
    /// O disponível ali (zero sem saldo registrado).
    pub disponivel: Quantidade,
}

/// O disponível de um produto em cada local ativo do CNPJ.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SaldosDoProdutoPorLocal {
    /// O produto.
    pub produto: Id,
}

impl Consulta for SaldosDoProdutoPorLocal {
    type Saida = Vec<SaldoNoLocal>;
    const PERMISSAO: &'static str = "estoque.saldo.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        Ok(
            crate::repositorio_transferencia::saldos_por_local(conexao, ctx.empresa, self.produto)?
                .into_iter()
                .map(|(local, nome, disponivel)| SaldoNoLocal {
                    local,
                    nome,
                    disponivel,
                })
                .collect(),
        )
    }
}
