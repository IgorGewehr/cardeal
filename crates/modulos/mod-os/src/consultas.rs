//! As consultas de OS: leitura autorizada sobre `os_ordem_servico`/`os_laudo_tecnico`/
//! `os_item_peca`/`os_item_mao_de_obra` — `docs/modulos/os.md` §10 (a tela de OS precisa da
//! lista de ordens em aberto e do detalhe completo de uma ordem).

use cardeal_kernel::{Id, Quantidade, Resultado};
use cardeal_modkit::{Consulta, Ctx};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::apontamento::ApontamentoDeTempo;
use crate::execucao::{ItemMaoDeObra, ItemPeca};
use crate::laudo::LaudoTecnico;
use crate::ordem::OrdemServico;
use crate::repositorio::{
    apontamento_de_linha, blob, id_de, item_mao_de_obra_de_linha, item_peca_de_linha,
    ordem_de_linha, persist,
};

/// Busca uma ordem de serviço pelo id.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn buscar_ordem(conexao: &Connection, id: Id) -> Resultado<Option<OrdemServico>> {
    conexao
        .query_row(
            "SELECT id, empresa, numero, cliente, equipamento, defeito_relatado, data_abertura,
                    tecnico_responsavel, estado, aprovado_por, garantia_dias, valor_total,
                    itens_orcamento, versao, previsao_entrega, numero_serie, acessorios
             FROM os_ordem_servico WHERE id = ?1",
            [blob(id)],
            ordem_de_linha,
        )
        .optional()
        .map_err(persist)
}

/// Todos os itens de peça de uma ordem.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn itens_peca_da_ordem(conexao: &Connection, ordem_servico: Id) -> Resultado<Vec<ItemPeca>> {
    let mut stmt = conexao
        .prepare(
            "SELECT id, ordem_servico, produto, quantidade, preco_unitario, custo_unitario,
                    coberto_garantia, aplicada, local, lote, estornada, encomenda_fornecedor,
                        encomenda_custo, encomenda_previsao, encomendada_em
             FROM os_item_peca WHERE ordem_servico = ?1 ORDER BY rowid",
        )
        .map_err(persist)?;
    let linhas = stmt
        .query_map([blob(ordem_servico)], item_peca_de_linha)
        .map_err(persist)?;
    linhas
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(persist)
}

/// Todos os itens de mão de obra de uma ordem.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn itens_mao_de_obra_da_ordem(
    conexao: &Connection,
    ordem_servico: Id,
) -> Resultado<Vec<ItemMaoDeObra>> {
    let mut stmt = conexao
        .prepare(
            "SELECT id, ordem_servico, descricao, valor, tecnico, horas
             FROM os_item_mao_de_obra WHERE ordem_servico = ?1 ORDER BY rowid",
        )
        .map_err(persist)?;
    let linhas = stmt
        .query_map([blob(ordem_servico)], item_mao_de_obra_de_linha)
        .map_err(persist)?;
    linhas
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(persist)
}

/// O laudo mais recente de uma ordem, se algum já foi registrado.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn laudo_mais_recente(
    conexao: &Connection,
    ordem_servico: Id,
) -> Resultado<Option<LaudoTecnico>> {
    conexao
        .query_row(
            "SELECT id, ordem_servico, descricao_problema, diagnostico, tecnico, criado_em
             FROM os_laudo_tecnico WHERE ordem_servico = ?1 ORDER BY criado_em DESC LIMIT 1",
            [blob(ordem_servico)],
            |r| {
                Ok(LaudoTecnico {
                    id: id_de(r.get::<_, Vec<u8>>(0)?),
                    ordem_servico: id_de(r.get::<_, Vec<u8>>(1)?),
                    descricao_problema: r.get(2)?,
                    diagnostico: r.get(3)?,
                    tecnico: id_de(r.get::<_, Vec<u8>>(4)?),
                    criado_em: cardeal_kernel::Instante::de_micros(r.get::<_, i64>(5)?),
                })
            },
        )
        .optional()
        .map_err(persist)
}

/// As ordens ainda não finalizadas (nem faturadas, canceladas ou reprovadas) — o que sustenta
/// a lista principal da tela de OS. Sem cursor real ainda, mesma decisão do financeiro
/// (`docs/09-protocolo-api.md` §5): um teto de 500 linhas é suficiente para uma PME.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn ordens_nao_finalizadas(conexao: &Connection, empresa: Id) -> Resultado<Vec<OrdemServico>> {
    let mut stmt = conexao
        .prepare(
            "SELECT id, empresa, numero, cliente, equipamento, defeito_relatado, data_abertura,
                    tecnico_responsavel, estado, aprovado_por, garantia_dias, valor_total,
                    itens_orcamento, versao, previsao_entrega, numero_serie, acessorios
             FROM os_ordem_servico
             WHERE empresa = ?1 AND estado NOT IN ('Faturada','Cancelada','Reprovada')
             ORDER BY numero DESC
             LIMIT 500",
        )
        .map_err(persist)?;
    let linhas = stmt
        .query_map([blob(empresa)], ordem_de_linha)
        .map_err(persist)?;
    linhas
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(persist)
}

/// O detalhe completo de uma ordem de serviço: a própria ordem, o laudo (se houver) e os
/// itens de orçamento — o que a tela de detalhe de OS precisa numa única chamada.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetalheOrdem {
    /// A ordem.
    pub ordem: OrdemServico,
    /// O laudo mais recente, se algum já foi registrado.
    pub laudo: Option<LaudoTecnico>,
    /// Os itens de peça do orçamento.
    pub itens_peca: Vec<ItemPeca>,
    /// Os itens de mão de obra do orçamento.
    pub itens_mao_de_obra: Vec<ItemMaoDeObra>,
    /// A linha do tempo: cada mudança de estado, mais antiga primeiro.
    pub historico: Vec<PassoDaOrdem>,
}

/// Um passo da linha do tempo de uma OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassoDaOrdem {
    /// Quando.
    pub momento: cardeal_kernel::Instante,
    /// Quem.
    pub usuario: Id,
    /// O estado em que a OS entrou.
    pub estado: crate::ordem::EstadoOs,
}

/// A linha do tempo de uma ordem, mais antiga primeiro.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn historico_da_ordem(conexao: &Connection, ordem: Id) -> Resultado<Vec<PassoDaOrdem>> {
    let mut stmt = conexao
        .prepare(
            "SELECT momento, usuario, estado FROM os_historico
             WHERE ordem_servico = ?1 ORDER BY id",
        )
        .map_err(persist)?;
    let linhas = stmt
        .query_map([blob(ordem)], |r| {
            Ok(PassoDaOrdem {
                momento: cardeal_kernel::Instante::de_micros(r.get(0)?),
                usuario: id_de(r.get::<_, Vec<u8>>(1)?),
                estado: crate::repositorio::estado_de(&r.get::<_, String>(2)?),
            })
        })
        .map_err(persist)?;
    linhas
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(persist)
}

/// Lista as ordens não finalizadas da empresa.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrdensEmAberto;

impl Consulta for OrdensEmAberto {
    type Saida = Vec<OrdemServico>;
    const PERMISSAO: &'static str = "os.ordem.ver";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        ordens_nao_finalizadas(conexao, ctx.empresa)
    }
}

/// Todas as ordens de serviço da empresa, em **qualquer** estado — inclusive `Faturada`,
/// `Cancelada` e `Reprovada`, que `OrdensEmAberto` propositalmente esconde (é a fila de
/// trabalho ativa, não o histórico). Pedido explícito do usuário (2026-09-14): depois de
/// faturar, a OS "sumia" da tela porque só existia a visão de fila ativa — o balcão precisa
/// conseguir ver/filtrar/ordenar por qualquer status, inclusive os finalizados.
///
/// Mesmo teto de paginação das demais consultas sem cursor real (`docs/09-protocolo-api.md`
/// §5): 500 linhas, mais que suficiente para o histórico de uma PME combinado com busca.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodasAsOrdens;

impl Consulta for TodasAsOrdens {
    type Saida = Vec<OrdemServico>;
    const PERMISSAO: &'static str = "os.ordem.ver";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let mut stmt = conexao
            .prepare(
                "SELECT id, empresa, numero, cliente, equipamento, defeito_relatado, data_abertura,
                        tecnico_responsavel, estado, aprovado_por, garantia_dias, valor_total,
                        itens_orcamento, versao, previsao_entrega, numero_serie, acessorios
                 FROM os_ordem_servico
                 WHERE empresa = ?1
                 ORDER BY numero DESC
                 LIMIT 500",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map([blob(ctx.empresa)], ordem_de_linha)
            .map_err(persist)?;
        linhas
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)
    }
}

/// Que recorte de estado [`BuscarOrdens`] devolve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FiltroEstadoOs {
    /// A fila de trabalho: tudo que não é `Faturada`/`Cancelada`/`Reprovada`.
    Ativas,
    /// Qualquer estado.
    Todas,
    /// Um estado só.
    Um(crate::ordem::EstadoOs),
}

impl FiltroEstadoOs {
    /// Se uma ordem neste estado entra no recorte.
    #[must_use]
    pub fn combina(self, estado: crate::ordem::EstadoOs) -> bool {
        use crate::ordem::EstadoOs as E;
        match self {
            Self::Ativas => !matches!(estado, E::Faturada | E::Cancelada | E::Reprovada),
            Self::Todas => true,
            Self::Um(e) => e == estado,
        }
    }
}

/// A lista de OS da tela, filtrada **no motor** — substitui carregar `TodasAsOrdens` (teto
/// de 500, então a OS nº 37 sumia da busca depois de alguns meses de uso) e filtrar na tela.
///
/// O termo casa com o número (prefixo; `#12` = só número), o aparelho e o defeito relatado,
/// ignorando acento e caixa e por palavras ("tela samsung" acha "Samsung A52 — tela"). O nome do cliente mora
/// em `mod-clientes`, que esta consulta não lê: quem chama resolve o termo contra o próprio
/// catálogo de clientes e manda os ids em `clientes`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuscarOrdens {
    /// O recorte de estado.
    pub filtro: FiltroEstadoOs,
    /// O texto digitado; vazio = sem filtro de texto.
    pub termo: String,
    /// Clientes cujo nome casou com `termo` (as OS deles entram mesmo que o texto não case).
    pub clientes: Vec<Id>,
    /// Quantas devolver, das mais recentes para as mais antigas.
    pub limite: u32,
}

impl Consulta for BuscarOrdens {
    type Saida = Vec<OrdemServico>;
    const PERMISSAO: &'static str = "os.ordem.ver";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let mut stmt = conexao
            .prepare(
                "SELECT id, empresa, numero, cliente, equipamento, defeito_relatado, data_abertura,
                        tecnico_responsavel, estado, aprovado_por, garantia_dias, valor_total,
                        itens_orcamento, versao, previsao_entrega, numero_serie, acessorios
                 FROM os_ordem_servico
                 WHERE empresa = ?1
                 ORDER BY numero DESC",
            )
            .map_err(persist)?;
        let termo = self.termo.trim();
        // "#12" é inequívoco: só o número. "12" sozinho também pode ser parte do aparelho
        // ("Galaxy A12"), então casa número (prefixo) **ou** texto.
        let so_numero = termo.strip_prefix('#').map(str::trim);
        let numero_digitado = so_numero
            .unwrap_or(termo)
            .chars()
            .all(|c| c.is_ascii_digit())
            .then(|| so_numero.unwrap_or(termo));
        let casa = |os: &OrdemServico| {
            if termo.is_empty() {
                return true;
            }
            let pelo_numero = numero_digitado
                .is_some_and(|n| !n.is_empty() && os.numero.to_string().starts_with(n));
            if so_numero.is_some() {
                return pelo_numero;
            }
            pelo_numero
                || self.clientes.contains(&os.cliente)
                || cardeal_kernel::texto::casa_por_palavras(
                    &format!("{} {}", os.equipamento, os.defeito_relatado),
                    termo,
                )
        };
        let mut saida = Vec::new();
        let linhas = stmt
            .query_map([blob(ctx.empresa)], ordem_de_linha)
            .map_err(persist)?;
        for linha in linhas {
            let os = linha.map_err(persist)?;
            if self.filtro.combina(os.estado) && casa(&os) {
                saida.push(os);
                if saida.len() >= self.limite as usize {
                    break;
                }
            }
        }
        Ok(saida)
    }
}

/// Várias ordens pelo id, numa consulta só — para quem só precisa de número/cliente/estado
/// de muitas OS de uma vez (o extrato do financeiro rotulando "OS #123 — João"), em vez de
/// um [`BuscarDetalheOrdem`] completo por linha. Ids que não existem são ignorados.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrdensPorId {
    /// As ordens desejadas.
    pub ordens: Vec<Id>,
}

impl Consulta for OrdensPorId {
    type Saida = Vec<OrdemServico>;
    const PERMISSAO: &'static str = "os.ordem.ver";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let mut saida = Vec::with_capacity(self.ordens.len());
        // Lotes de 500 ficam bem abaixo do limite de parâmetros do SQLite.
        for lote in self.ordens.chunks(500) {
            let marcadores = vec!["?"; lote.len()].join(",");
            let sql = format!(
                "SELECT id, empresa, numero, cliente, equipamento, defeito_relatado, data_abertura,
                        tecnico_responsavel, estado, aprovado_por, garantia_dias, valor_total,
                        itens_orcamento, versao, previsao_entrega, numero_serie, acessorios
                 FROM os_ordem_servico
                 WHERE empresa = ? AND id IN ({marcadores})"
            );
            let mut stmt = conexao.prepare(&sql).map_err(persist)?;
            let mut parametros = vec![blob(ctx.empresa)];
            parametros.extend(lote.iter().map(|id| blob(*id)));
            let linhas = stmt
                .query_map(rusqlite::params_from_iter(parametros), ordem_de_linha)
                .map_err(persist)?;
            for linha in linhas {
                saida.push(linha.map_err(persist)?);
            }
        }
        Ok(saida)
    }
}

/// O último preço unitário cobrado por um produto numa OS (a mais recente que o orçou) —
/// a sugestão de preço quando a peça entra num orçamento novo e a empresa não mantém tabela
/// de preço. `None` se o produto nunca foi orçado.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct UltimoPrecoDaPeca {
    /// O produto.
    pub produto: Id,
}

impl Consulta for UltimoPrecoDaPeca {
    type Saida = Option<cardeal_kernel::Preco>;
    const PERMISSAO: &'static str = "os.ordem.ver";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        conexao
            .query_row(
                "SELECT i.preco_unitario
                 FROM os_item_peca i
                 JOIN os_ordem_servico o ON o.id = i.ordem_servico
                 WHERE o.empresa = ?1 AND i.produto = ?2
                 ORDER BY o.numero DESC, i.rowid DESC
                 LIMIT 1",
                [blob(ctx.empresa), blob(self.produto)],
                |r| r.get::<_, i64>(0),
            )
            .optional()
            .map(|v| v.map(cardeal_kernel::Preco::interna))
            .map_err(persist)
    }
}

/// Receita, custo das peças e margem das OS faturadas num período — "quanto as OS deram de
/// verdade no mês". A data é a do faturamento (o passo `Faturada` da linha do tempo); OS
/// faturadas antes da linha do tempo existir ficam de fora. O custo é o das peças aplicadas e
/// não estornadas (o mesmo CMV do lançamento de faturamento); mão de obra não tem custo aqui.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MargemDasOrdensNoPeriodo {
    /// O período (datas de faturamento).
    pub periodo: cardeal_kernel::Periodo,
}

/// O que [`MargemDasOrdensNoPeriodo`] devolve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MargemDasOrdens {
    /// Quantas OS faturadas no período.
    pub ordens: u32,
    /// Soma do total faturado.
    pub receita: cardeal_kernel::Dinheiro,
    /// Soma do custo das peças aplicadas.
    pub custo_pecas: cardeal_kernel::Dinheiro,
}

impl MargemDasOrdens {
    /// Receita menos custo das peças.
    #[must_use]
    pub fn margem(&self) -> cardeal_kernel::Dinheiro {
        self.receita - self.custo_pecas
    }
}

impl Consulta for MargemDasOrdensNoPeriodo {
    type Saida = MargemDasOrdens;
    const PERMISSAO: &'static str = "os.ordem.ver";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        // O instante (micros UTC) do começo do primeiro dia e do fim do último, em Brasília.
        let meia_noite = |d: cardeal_kernel::Data| {
            cardeal_kernel::Instante::de_data_hora(
                d,
                cardeal_kernel::Hora::MEIA_NOITE,
                cardeal_kernel::Fuso::BRASILIA,
            )
            .em_micros()
        };
        let de = meia_noite(self.periodo.de);
        let ate = meia_noite(self.periodo.ate.mais_dias(1));
        let mut stmt = conexao
            .prepare(
                "SELECT o.id, o.valor_total
                 FROM os_ordem_servico o
                 WHERE o.empresa = ?1 AND o.estado = 'Faturada'
                   AND (SELECT MAX(h.momento) FROM os_historico h
                        WHERE h.ordem_servico = o.id AND h.estado = 'Faturada')
                       BETWEEN ?2 AND ?3 - 1",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map(rusqlite::params![blob(ctx.empresa), de, ate], |r| {
                Ok((id_de(r.get::<_, Vec<u8>>(0)?), r.get::<_, i64>(1)?))
            })
            .map_err(persist)?;
        let mut m = MargemDasOrdens::default();
        for linha in linhas {
            let (id, total) = linha.map_err(persist)?;
            m.ordens += 1;
            m.receita += cardeal_kernel::Dinheiro::centavos(total);
            for item in itens_peca_da_ordem(conexao, id)? {
                m.custo_pecas += item.total_custo();
            }
        }
        Ok(m)
    }
}

/// Busca o detalhe completo de uma ordem pelo id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuscarDetalheOrdem {
    /// A ordem de serviço.
    pub ordem_servico: Id,
}

impl Consulta for BuscarDetalheOrdem {
    type Saida = Option<DetalheOrdem>;
    const PERMISSAO: &'static str = "os.ordem.ver";

    fn executar(self, _ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let Some(ordem) = buscar_ordem(conexao, self.ordem_servico)? else {
            return Ok(None);
        };
        let laudo = laudo_mais_recente(conexao, self.ordem_servico)?;
        let itens_peca = itens_peca_da_ordem(conexao, self.ordem_servico)?;
        let itens_mao_de_obra = itens_mao_de_obra_da_ordem(conexao, self.ordem_servico)?;
        let historico = historico_da_ordem(conexao, self.ordem_servico)?;
        Ok(Some(DetalheOrdem {
            ordem,
            laudo,
            itens_peca,
            itens_mao_de_obra,
            historico,
        }))
    }
}

/// As ordens paradas em `AguardandoAprovacao` — a fila de aprovação do cliente
/// (`docs/modulos/os.md` §6), sem precisar filtrar `OrdensEmAberto` na mão.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn ordens_aguardando_aprovacao(
    conexao: &Connection,
    empresa: Id,
) -> Resultado<Vec<OrdemServico>> {
    let mut stmt = conexao
        .prepare(
            "SELECT id, empresa, numero, cliente, equipamento, defeito_relatado, data_abertura,
                    tecnico_responsavel, estado, aprovado_por, garantia_dias, valor_total,
                    itens_orcamento, versao, previsao_entrega, numero_serie, acessorios
             FROM os_ordem_servico
             WHERE empresa = ?1 AND estado = 'AguardandoAprovacao'
             ORDER BY numero DESC
             LIMIT 500",
        )
        .map_err(persist)?;
    let linhas = stmt
        .query_map([blob(empresa)], ordem_de_linha)
        .map_err(persist)?;
    linhas
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(persist)
}

/// Lista as ordens paradas aguardando decisão do cliente.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrdensAguardandoAprovacao;

impl Consulta for OrdensAguardandoAprovacao {
    type Saida = Vec<OrdemServico>;
    const PERMISSAO: &'static str = "os.ordem.ver";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        ordens_aguardando_aprovacao(conexao, ctx.empresa)
    }
}

/// Limiar de similaridade (`docs/modulos/os.md` §6) para considerar duas descrições de
/// equipamento "o mesmo aparelho" apesar de erro de digitação ou variação de texto — mais
/// permissivo que o casamento de item de compra (0,82) porque aqui o custo de um falso
/// positivo é só aparecer uma linha a mais no histórico, não um vínculo automático de custo.
const LIMIAR_MESMO_EQUIPAMENTO: f32 = 0.5;

/// Todas as ordens de serviço do mesmo cliente cujo equipamento é "parecido" com o
/// informado — reincidência mesmo sem `AcionarGarantia` formal (`docs/modulos/os.md` §6).
/// Inclui a própria ordem de referência, se `excluir` apontar para uma.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn historico_do_equipamento(
    conexao: &Connection,
    cliente: Id,
    equipamento: &str,
    excluir: Option<Id>,
) -> Resultado<Vec<OrdemServico>> {
    let mut stmt = conexao
        .prepare(
            "SELECT id, empresa, numero, cliente, equipamento, defeito_relatado, data_abertura,
                    tecnico_responsavel, estado, aprovado_por, garantia_dias, valor_total,
                    itens_orcamento, versao, previsao_entrega, numero_serie, acessorios
             FROM os_ordem_servico WHERE cliente = ?1 ORDER BY numero DESC LIMIT 500",
        )
        .map_err(persist)?;
    let linhas = stmt
        .query_map([blob(cliente)], ordem_de_linha)
        .map_err(persist)?;
    let todas = linhas
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(persist)?;
    Ok(todas
        .into_iter()
        .filter(|os| Some(os.id) != excluir)
        .filter(|os| {
            cardeal_kernel::texto::similaridade(&os.equipamento, equipamento)
                >= LIMIAR_MESMO_EQUIPAMENTO
        })
        .collect())
}

/// Busca o histórico de ordens de um cliente para um equipamento parecido.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoricoDoEquipamento {
    /// O cliente.
    pub cliente: Id,
    /// A descrição do equipamento a comparar.
    pub equipamento: String,
    /// A própria ordem de referência, para excluir da lista (opcional).
    pub excluir: Option<Id>,
}

impl Consulta for HistoricoDoEquipamento {
    type Saida = Vec<OrdemServico>;
    const PERMISSAO: &'static str = "os.ordem.ver";

    fn executar(self, _ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        historico_do_equipamento(conexao, self.cliente, &self.equipamento, self.excluir)
    }
}

/// Todos os apontamentos de tempo de uma ordem, mais antigo primeiro.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn apontamentos_da_ordem(
    conexao: &Connection,
    ordem_servico: Id,
) -> Resultado<Vec<ApontamentoDeTempo>> {
    let mut stmt = conexao
        .prepare(
            "SELECT id, ordem_servico, tecnico, inicio, fim, ajustado, motivo_ajuste, versao
             FROM os_apontamento_tempo WHERE ordem_servico = ?1 ORDER BY inicio",
        )
        .map_err(persist)?;
    let linhas = stmt
        .query_map([blob(ordem_servico)], apontamento_de_linha)
        .map_err(persist)?;
    linhas
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(persist)
}

/// Lista os apontamentos de tempo de uma ordem — a UI usa para mostrar o histórico e o
/// cronômetro em andamento.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApontamentosDaOrdem {
    /// A ordem de serviço.
    pub ordem_servico: Id,
}

impl Consulta for ApontamentosDaOrdem {
    type Saida = Vec<ApontamentoDeTempo>;
    const PERMISSAO: &'static str = "os.ordem.ver";

    fn executar(self, _ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        apontamentos_da_ordem(conexao, self.ordem_servico)
    }
}

/// O tempo total apontado numa ordem, em segundos — soma só os apontamentos **encerrados**
/// (um apontamento ainda aberto está em andamento; a UI mostra o parcial dele à parte, com
/// `Instante::agora()`, para não gravar um número que muda a cada consulta).
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn tempo_total_da_ordem(conexao: &Connection, ordem_servico: Id) -> Resultado<i64> {
    let total: i64 = conexao
        .query_row(
            "SELECT COALESCE(SUM(fim - inicio), 0)
             FROM os_apontamento_tempo WHERE ordem_servico = ?1 AND fim IS NOT NULL",
            [blob(ordem_servico)],
            |r| r.get(0),
        )
        .map_err(persist)?;
    Ok(total / 1_000_000)
}

/// O tempo total apontado (encerrado) por uma ordem de serviço, em segundos.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TempoTotalDaOrdem {
    /// A ordem de serviço.
    pub ordem_servico: Id,
}

impl Consulta for TempoTotalDaOrdem {
    type Saida = i64;
    const PERMISSAO: &'static str = "os.ordem.ver";

    fn executar(self, _ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        tempo_total_da_ordem(conexao, self.ordem_servico)
    }
}

/// O tempo total apontado (encerrado) por um técnico num período — a base de produtividade
/// por técnico que alimenta `cardeal-analytics`.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn tempo_por_tecnico_no_periodo(
    conexao: &Connection,
    tecnico: Id,
    de: cardeal_kernel::Instante,
    ate: cardeal_kernel::Instante,
) -> Resultado<i64> {
    let total: i64 = conexao
        .query_row(
            "SELECT COALESCE(SUM(fim - inicio), 0)
             FROM os_apontamento_tempo
             WHERE tecnico = ?1 AND fim IS NOT NULL AND inicio >= ?2 AND inicio < ?3",
            rusqlite::params![blob(tecnico), de.em_micros(), ate.em_micros()],
            |r| r.get(0),
        )
        .map_err(persist)?;
    Ok(total / 1_000_000)
}

/// O tempo total apontado por um técnico num período.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TempoPorTecnicoNoPeriodo {
    /// O técnico.
    pub tecnico: Id,
    /// Início do período (inclusive).
    pub de: cardeal_kernel::Instante,
    /// Fim do período (exclusive).
    pub ate: cardeal_kernel::Instante,
}

impl Consulta for TempoPorTecnicoNoPeriodo {
    type Saida = i64;
    const PERMISSAO: &'static str = "os.ordem.ver";

    fn executar(self, _ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        tempo_por_tecnico_no_periodo(conexao, self.tecnico, self.de, self.ate)
    }
}

/// Uma peça orçada, ainda não aplicada, cujo saldo disponível no estoque **não cobre** a
/// quantidade necessária — auditoria de integração (2026-09-11): hoje, saber que uma OS está
/// parada esperando peça exige abrir a OS e o produto em telas separadas e comparar na
/// cabeça; esta consulta cruza os dois de uma vez, só pela porta pública de `mod_estoque`
/// (`saldo_disponivel_do_produto` — nunca lendo `estoque_saldo_local` direto,
/// `docs/contratos-internos.md` §7 regra 2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemAguardandoEstoque {
    /// A ordem de serviço.
    pub ordem_servico: Id,
    /// O número da OS (para exibir sem outra consulta).
    pub numero: u64,
    /// O item de peça no orçamento.
    pub item_peca: Id,
    /// O produto.
    pub produto: Id,
    /// Quanto o orçamento pede.
    pub quantidade_necessaria: Quantidade,
    /// Quanto há disponível no estoque agora (soma entre locais).
    pub saldo_disponivel: Quantidade,
    /// O aparelho da OS (para a lista de compras dizer para quê é a peça).
    pub equipamento: String,
    /// O cliente da OS.
    pub cliente: Id,
    /// A encomenda, se a peça já foi pedida ao fornecedor.
    pub encomenda: Option<crate::execucao::Encomenda>,
}

/// Todas as peças orçadas (em ordens não finalizadas) ainda não aplicadas cujo saldo
/// disponível no estoque é insuficiente para a quantidade pedida — e as já encomendadas que
/// não chegaram, mesmo que o estoque tenha coberto entretanto (a encomenda continua valendo
/// até alguém registrar a chegada ou desfazê-la).
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn pecas_aguardando_estoque(
    conexao: &Connection,
    empresa: Id,
) -> Resultado<Vec<ItemAguardandoEstoque>> {
    let mut pendentes = Vec::new();
    for os in ordens_nao_finalizadas(conexao, empresa)? {
        for item in itens_peca_da_ordem(conexao, os.id)? {
            if item.aplicada || item.estornada {
                continue;
            }
            let saldo_disponivel = mod_estoque::saldo_disponivel_do_produto(conexao, item.produto)?;
            if saldo_disponivel < item.quantidade || item.encomenda.is_some() {
                pendentes.push(ItemAguardandoEstoque {
                    ordem_servico: os.id,
                    numero: os.numero,
                    item_peca: item.id,
                    produto: item.produto,
                    quantidade_necessaria: item.quantidade,
                    saldo_disponivel,
                    equipamento: os.equipamento.clone(),
                    cliente: os.cliente,
                    encomenda: item.encomenda,
                });
            }
        }
    }
    Ok(pendentes)
}

/// Lista as peças orçadas e ainda não aplicadas cujo estoque disponível é insuficiente — o
/// radar de "OS parada esperando peça".
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PecasAguardandoEstoque;

impl Consulta for PecasAguardandoEstoque {
    type Saida = Vec<ItemAguardandoEstoque>;
    const PERMISSAO: &'static str = "os.ordem.ver";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        pecas_aguardando_estoque(conexao, ctx.empresa)
    }
}
