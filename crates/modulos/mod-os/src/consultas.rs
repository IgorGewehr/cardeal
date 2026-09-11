//! As consultas de OS: leitura autorizada sobre `os_ordem_servico`/`os_laudo_tecnico`/
//! `os_item_peca`/`os_item_mao_de_obra` — `docs/modulos/os.md` §10 (a tela de OS precisa da
//! lista de ordens em aberto e do detalhe completo de uma ordem).

use cardeal_kernel::{Id, Resultado};
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
            "SELECT id, empresa, numero, cliente, equipamento, data_abertura,
                    tecnico_responsavel, estado, aprovado_por, garantia_dias, valor_total,
                    itens_orcamento, versao
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
                    coberto_garantia, aplicada
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
            "SELECT id, empresa, numero, cliente, equipamento, data_abertura,
                    tecnico_responsavel, estado, aprovado_por, garantia_dias, valor_total,
                    itens_orcamento, versao
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
        Ok(Some(DetalheOrdem {
            ordem,
            laudo,
            itens_peca,
            itens_mao_de_obra,
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
            "SELECT id, empresa, numero, cliente, equipamento, data_abertura,
                    tecnico_responsavel, estado, aprovado_por, garantia_dias, valor_total,
                    itens_orcamento, versao
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
            "SELECT id, empresa, numero, cliente, equipamento, data_abertura,
                    tecnico_responsavel, estado, aprovado_por, garantia_dias, valor_total,
                    itens_orcamento, versao
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
