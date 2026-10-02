//! A listagem paginada de OS (`os.ordens.v2`) — o caminho da rede: das mais recentes para as
//! mais antigas, página por cursor (`numero`), a busca e os nomes dos clientes resolvidos no
//! servidor (o navegador não precisa carregar o catálogo de clientes). A `v1`
//! ([`super::BuscarOrdens`]) continua para o desktop até a migração das telas.

#[cfg(feature = "sqlite")]
use cardeal_kernel::Id;
use cardeal_modkit::{Consulta, Pagina, PedidoPagina};
use serde::{Deserialize, Serialize};

use super::FiltroEstadoOs;
use crate::ordem::OrdemServico;

#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;

/// Uma OS na lista, já com o nome do cliente.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemListaOrdem {
    /// A ordem.
    pub ordem: OrdemServico,
    /// O nome do cliente (`None` se o cadastro sumiu).
    pub cliente_nome: Option<String>,
}

/// As OS da empresa, uma página por vez.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListarOrdens {
    /// O recorte de estado.
    pub filtro: FiltroEstadoOs,
    /// Número ("#12"), aparelho, defeito ou nome do cliente; vazio = sem filtro.
    pub busca: String,
    /// A página.
    pub pagina: PedidoPagina,
}

impl Consulta for ListarOrdens {
    type Saida = Pagina<ItemListaOrdem>;
    const PERMISSAO: &'static str = "os.ordem.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        use super::CriterioBusca;
        use crate::repositorio::{blob, ordem_de_linha, persist};

        let termo = self.busca.trim();
        let clientes = if termo.is_empty() || termo.starts_with('#') {
            Vec::new()
        } else {
            mod_clientes::pessoas_cujo_nome_casa(conexao, ctx.organizacao, termo)?
        };
        let criterio = CriterioBusca::novo(termo, &clientes);
        let antes_de: Option<u64> = self.pagina.chave()?;
        let limite = self.pagina.limite_efetivo();

        // A busca é por palavras sem acento (como nas telas), então o filtro de texto roda
        // aqui; o cursor e o índice (empresa, numero) limitam a varredura ao necessário.
        let mut stmt = conexao
            .prepare_cached(
                "SELECT id, empresa, numero, cliente, equipamento, defeito_relatado, data_abertura,
                        tecnico_responsavel, estado, aprovado_por, garantia_dias, valor_total,
                        itens_orcamento, versao, previsao_entrega, numero_serie, acessorios
                 FROM os_ordem_servico
                 WHERE empresa = ?1 AND (?2 IS NULL OR numero < ?2)
                 ORDER BY numero DESC",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map(
                rusqlite::params![
                    blob(ctx.empresa),
                    antes_de.map(|n| i64::try_from(n).unwrap_or(i64::MAX))
                ],
                ordem_de_linha,
            )
            .map_err(persist)?;
        let primeira = self.pagina.apos.is_none();
        let mut achadas = Vec::with_capacity(limite + 1);
        let mut total = 0u64;
        for linha in linhas {
            let os = linha.map_err(persist)?;
            if !(self.filtro.combina(os.estado) && criterio.casa(&os)) {
                continue;
            }
            total += 1;
            if achadas.len() <= limite {
                achadas.push(os);
            } else if !primeira {
                // Sem total para contar, a página está completa: para de varrer.
                break;
            }
        }

        let ids: Vec<Id> = achadas.iter().map(|o| o.cliente).collect();
        let nomes = mod_clientes::nomes_das_pessoas(conexao, &ids)?;
        let itens = achadas
            .into_iter()
            .map(|ordem| ItemListaOrdem {
                cliente_nome: nomes.get(&ordem.cliente).cloned(),
                ordem,
            })
            .collect();
        Ok(Pagina::montar(
            itens,
            &self.pagina,
            |i: &ItemListaOrdem| i.ordem.numero,
            primeira.then_some(total),
        ))
    }
}
