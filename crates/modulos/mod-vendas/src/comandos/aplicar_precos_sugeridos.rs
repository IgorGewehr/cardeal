//! Leva o preço sugerido pelo markup divisor (`mod-estoque`) para uma tabela de preço: cria a
//! regra permanente do produto ou atualiza a que já existe. Produto que o estoque não
//! consegue precificar (sem custo, sem margem) fica de fora e volta com o motivo — nunca
//! recebe um preço inventado.

use cardeal_kernel::Id;
use cardeal_modkit::Comando;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::Risco;
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
use mod_estoque::precificacao::ArredondamentoDePreco;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use cardeal_kernel::{CodigoErro, Erro, Preco, Resultado};

#[cfg(feature = "sqlite")]
use crate::preco::{AlvoRegra, RegraPreco, TipoTabela};
#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioVendas;

/// Aplica o preço sugerido dos produtos à tabela de preço.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AplicarPrecosSugeridos {
    /// A tabela de destino (de venda ou atacado; promocional exige vigência e não serve).
    pub tabela_preco: Id,
    /// Os produtos; vazio = nenhum (a tela escolhe um grupo e lista os produtos dele).
    pub produtos: Vec<Id>,
    /// Como arredondar para o preço de prateleira.
    pub arredondamento: ArredondamentoDePreco,
}

/// O que foi feito.
#[derive(Debug, Serialize, Deserialize)]
pub struct PrecosAplicados {
    /// Regras novas criadas.
    pub criadas: u32,
    /// Regras existentes atualizadas.
    pub atualizadas: u32,
    /// Produtos que ficaram de fora e o porquê.
    pub ignorados: Vec<(Id, String)>,
}

impl Comando for AplicarPrecosSugeridos {
    type Saida = PrecosAplicados;
    const PERMISSAO: &'static str = "vendas.tabela_preco.editar";
    const RISCO: Risco = Risco::Medio;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let tabela = RepositorioVendas::novo(uow)
            .buscar_tabela_preco(self.tabela_preco)?
            .ok_or_else(|| Erro::nao_encontrado("tabela de preço"))?;
        if !tabela.ativa || tabela.tipo == TipoTabela::Promocional {
            return Err(Erro::novo(
                CodigoErro::REGRA_VIOLADA,
                "escolha uma tabela de venda ou de atacado ativa (a promocional exige período)",
            ));
        }

        let sugestoes =
            mod_estoque::sugerir_precos(uow.conexao(), ctx, None, Some(&self.produtos))?;
        let mut saida = PrecosAplicados {
            criadas: 0,
            atualizadas: 0,
            ignorados: Vec::new(),
        };
        for id in &self.produtos {
            let Some(s) = sugestoes.iter().find(|s| s.produto == *id) else {
                saida
                    .ignorados
                    .push((*id, "produto inexistente ou inativo".to_owned()));
                continue;
            };
            let Some(calculo) = s.sugerido else {
                saida.ignorados.push((
                    *id,
                    s.problema
                        .clone()
                        .unwrap_or_else(|| "sem preço sugerido".to_owned()),
                ));
                continue;
            };
            let preco = Preco::centavos(
                mod_estoque::precificacao::arredondar_preco(calculo.preco, self.arredondamento)
                    .em_centavos(),
            );
            let mut repo = RepositorioVendas::novo(uow);
            if let Some(regra) = repo.regra_permanente_do_produto(self.tabela_preco, *id)? {
                repo.atualizar_preco_da_regra(regra, preco)?;
                saida.atualizadas += 1;
            } else {
                repo.inserir_regra_preco(&RegraPreco {
                    id: Id::novo(),
                    empresa: ctx.empresa,
                    tabela_preco: self.tabela_preco,
                    alvo: AlvoRegra::Produto(*id),
                    quantidade_minima: None,
                    preco,
                    periodo_de: None,
                    periodo_ate: None,
                })?;
                saida.criadas += 1;
            }
        }
        Ok(saida)
    }
}
