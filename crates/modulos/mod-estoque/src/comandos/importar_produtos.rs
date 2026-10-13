//! Importa produtos de uma planilha CSV — ver [`crate::importacao_produtos`]. Tudo ou nada:
//! qualquer erro de linha recusa o arquivo inteiro, e grupos, unidades, produtos e estoque
//! inicial entram na mesma transação.

#[cfg(feature = "sqlite")]
use cardeal_kernel::{CodigoErro, Erro, Id, Resultado};
use cardeal_modkit::Comando;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::Risco;
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use crate::comandos::importar_estrutura::{criar_grupos_planejados, id_planejado};
#[cfg(feature = "sqlite")]
use crate::comandos::{registrar_entrada_comum, DadosEntrada};
#[cfg(feature = "sqlite")]
use crate::importacao_produtos::{planejar, UnidadePlanejada};
#[cfg(feature = "sqlite")]
use crate::produto::{Produto, Unidade};
#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioEstoque;

/// Cria os produtos que a planilha traz e o catálogo ainda não tem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportarProdutos {
    /// O conteúdo do CSV.
    pub texto: String,
}

/// O que a importação fez.
#[derive(Debug, Serialize, Deserialize)]
pub struct ProdutosImportados {
    /// Produtos criados.
    pub criados: u32,
    /// Produtos da planilha que já existiam (ficaram como estavam).
    pub ja_existiam: u32,
    /// Grupos da estrutura criados para acomodá-los.
    pub grupos_criados: u32,
    /// Unidades criadas.
    pub unidades_criadas: u32,
    /// Produtos que entraram com estoque inicial.
    pub com_estoque: u32,
}

impl Comando for ImportarProdutos {
    type Saida = ProdutosImportados;
    const PERMISSAO: &'static str = "estoque.produto.criar";
    const RISCO: Risco = Risco::Medio;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let existentes = crate::consultas_importacao::existentes_para_produtos(
            uow.conexao(),
            ctx.organizacao,
            ctx.empresa,
        )?;
        let plano = planejar(&self.texto, &existentes);
        if let Some(e) = plano.erros.first() {
            return Err(Erro::novo(
                CodigoErro::REGRA_VIOLADA,
                format!(
                    "a planilha tem {} linha(s) com erro (a primeira: linha {}: {}); nada foi importado",
                    plano.erros.len(),
                    e.linha,
                    e.mensagem
                ),
            ));
        }
        // A importação não é atalho de permissão: o que fora dela exige outro poder, aqui
        // também exige.
        if plano.define_parametros() && !ctx.concede("estoque.produto.editar") {
            return Err(Erro::novo(
                CodigoErro::SEM_PERMISSAO,
                "margem, ponto de pedido e estoque mínimo/máximo exigem a permissão de editar produto",
            ));
        }
        if plano.tem_estoque() && !ctx.concede("estoque.movimento.entrada") {
            return Err(Erro::novo(
                CodigoErro::SEM_PERMISSAO,
                "o estoque inicial exige a permissão de registrar entrada de estoque",
            ));
        }

        let grupos =
            criar_grupos_planejados(&plano.grupos, &existentes.grupos, ctx.organizacao, uow)?;

        let mut unidades: Vec<Id> = Vec::with_capacity(plano.unidades.len());
        for u in &plano.unidades {
            let unidade = Unidade {
                id: Id::novo(),
                empresa: ctx.organizacao,
                sigla: u.sigla.clone(),
                nome: u.nome.clone(),
                fracionavel: u.fracionavel,
            };
            RepositorioEstoque::novo(uow).inserir_unidade(&unidade)?;
            unidades.push(unidade.id);
        }

        let mut com_estoque = 0u32;
        for novo in &plano.produtos {
            // O plano só põe aqui grupo que existe ou que acabou de ser criado.
            let grupo = id_planejado(novo.grupo, &grupos).ok_or_else(|| {
                Erro::novo(CodigoErro::FALHA_INTERNA, "produto planejado sem grupo")
            })?;
            let unidade = match novo.unidade {
                UnidadePlanejada::Existente(id) => id,
                UnidadePlanejada::Nova(i) => unidades[i],
            };
            // As mesmas regras de `CriarProduto` (o plano já as conferiu; o domínio confere de
            // novo, que é barato e mantém uma porta só de entrada).
            let mut produto = Produto::novo(ctx.organizacao, grupo, &novo.nome, &novo.ncm, unidade)
                .map_err(|e| Erro::de_dominio(&e))?;
            if let Some(gtin) = novo.codigo_barras.as_deref() {
                produto = produto
                    .com_codigo_barras(gtin)
                    .map_err(|e| Erro::de_dominio(&e))?;
            }
            produto = produto.com_detalhes_tecnicos(novo.detalhes.clone());
            produto.ponto_pedido = novo.ponto_pedido;
            produto.estoque_minimo = novo.estoque_minimo;
            produto.estoque_maximo = novo.estoque_maximo;
            RepositorioEstoque::novo(uow).inserir_produto(&produto)?;

            if novo.margem.is_some() {
                crate::repositorio_preco::definir_margem_do_produto(
                    uow.conexao(),
                    ctx.organizacao,
                    produto.id,
                    novo.margem,
                )?;
            }
            if let Some(e) = novo.estoque {
                registrar_entrada_comum(
                    DadosEntrada {
                        produto: produto.id,
                        local: e.local,
                        quantidade: e.quantidade,
                        custo_unitario: e.custo_unitario,
                        origem_modulo: "estoque",
                        origem_id: None,
                    },
                    ctx,
                    uow,
                )?;
                com_estoque += 1;
            }
        }

        Ok(ProdutosImportados {
            criados: u32::try_from(plano.produtos.len()).unwrap_or(u32::MAX),
            ja_existiam: plano.ja_existiam,
            grupos_criados: u32::try_from(grupos.len()).unwrap_or(u32::MAX),
            unidades_criadas: u32::try_from(unidades.len()).unwrap_or(u32::MAX),
            com_estoque,
        })
    }
}
