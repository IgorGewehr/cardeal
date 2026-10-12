//! Importa a estrutura mercadológica de uma planilha CSV (`Grupo;Subgrupo;Categoria;
//! Subcategoria`) — ver [`crate::importacao_estrutura`]. Tudo ou nada: qualquer erro de linha
//! recusa o arquivo inteiro.

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
use crate::importacao_estrutura::{planejar, PaiPlanejado};
#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioEstoque;

/// Cria os grupos que a planilha traz e a árvore ainda não tem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportarEstrutura {
    /// O conteúdo do CSV.
    pub texto: String,
}

/// O que a importação fez.
#[derive(Debug, Serialize, Deserialize)]
pub struct EstruturaImportada {
    /// Grupos criados.
    pub criados: u32,
    /// Grupos da planilha que já existiam.
    pub ja_existiam: u32,
}

impl Comando for ImportarEstrutura {
    type Saida = EstruturaImportada;
    const PERMISSAO: &'static str = "estoque.produto.criar";
    const RISCO: Risco = Risco::Medio;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let existentes =
            crate::consultas_importacao::nos_existentes(uow.conexao(), ctx.organizacao)?;
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
        let arvore = crate::hierarquia::Arvore::de_nos(
            &existentes
                .iter()
                .map(|e| crate::hierarquia::NoGrupo {
                    id: e.id,
                    pai: e.pai,
                })
                .collect::<Vec<_>>(),
        );
        let mut repo = RepositorioEstoque::novo(uow);
        let mut ids: Vec<Id> = Vec::with_capacity(plano.nos.len());
        for no in &plano.nos {
            let pai = match no.pai {
                PaiPlanejado::Raiz => None,
                PaiPlanejado::Existente(p) => Some(p),
                PaiPlanejado::Novo(i) => Some(ids[i]),
            };
            // As mesmas regras de `CriarGrupoProduto`: pai existente e profundidade. Pai novo
            // já foi medido pelo plano (o caminho tem no máximo `NIVEL_MAXIMO` níveis).
            if let PaiPlanejado::Existente(_) = no.pai {
                arvore
                    .validar_novo_filho(pai)
                    .map_err(crate::comandos::erro_de_hierarquia)?;
            }
            let id = Id::novo();
            repo.inserir_grupo_produto(ctx.organizacao, id, &no.codigo, &no.nome, pai)?;
            ids.push(id);
        }
        Ok(EstruturaImportada {
            criados: u32::try_from(ids.len()).unwrap_or(u32::MAX),
            ja_existiam: plano.ja_existiam,
        })
    }
}
