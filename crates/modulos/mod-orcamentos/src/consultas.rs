//! As consultas do módulo de orçamentos — leitura autorizada, sem SQL cru na tela.
//! `docs/09-protocolo-api.md` §5: sem cursor real ainda, teto de 500 linhas (suficiente para
//! o volume de uma PME).

use cardeal_kernel::{Data, Dinheiro, Id, Percentual, Resultado};
use cardeal_modkit::{Consulta, Ctx};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::item::ItemOrcamento;
use crate::orcamento::{EstadoOrcamento, Orcamento};
use crate::repositorio::{blob, buscar_orcamento, estado_de, itens_do_orcamento, persist};

/// Uma linha da lista de orçamentos — cabeçalho para a grade, sem carregar itens.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemOrcamentoLista {
    /// O orçamento.
    pub orcamento: Id,
    /// Número sequencial.
    pub numero: u64,
    /// Nome do cliente.
    pub cliente_nome: String,
    /// Assunto.
    pub assunto: String,
    /// Data de emissão.
    pub data_emissao: Data,
    /// Validade.
    pub validade: Data,
    /// Total líquido.
    pub total: Dinheiro,
    /// Estado.
    pub estado: EstadoOrcamento,
    /// Quantos itens.
    pub itens: u32,
    /// Se a validade já passou (e o estado ainda é Rascunho/Enviado).
    pub vencido: bool,
}

/// Lista os orçamentos da empresa, mais recentes primeiro, com filtros opcionais.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OrcamentosRecentes {
    /// Filtra por estado.
    pub estado: Option<EstadoOrcamento>,
    /// Filtra por cliente cadastrado.
    pub cliente: Option<Id>,
    /// Busca textual em assunto e nome do cliente (case-insensitive, sem acento não).
    pub texto: Option<String>,
    /// Emissão a partir desta data.
    pub desde: Option<Data>,
    /// Emissão até esta data.
    pub ate: Option<Data>,
}

impl Consulta for OrcamentosRecentes {
    type Saida = Vec<ItemOrcamentoLista>;
    const PERMISSAO: &'static str = "orcamentos.orcamento.ver";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let hoje = ctx.hoje();
        let (sql, filtra_estado) = if let Some(estado) = self.estado {
            (
                format!(
                    "SELECT o.id, o.numero, o.cliente, o.cliente_nome, o.assunto, o.data_emissao,
                            o.validade, o.desconto_percentual, o.estado,
                            COALESCE((SELECT SUM(i.total) FROM orcamentos_item i WHERE i.orcamento = o.id), 0),
                            (SELECT COUNT(*) FROM orcamentos_item i WHERE i.orcamento = o.id)
                     FROM orcamentos_orcamento o
                     WHERE o.empresa = ?1 AND o.estado = '{}'
                     ORDER BY o.numero DESC LIMIT 500",
                    estado.rotulo()
                ),
                true,
            )
        } else {
            (
                "SELECT o.id, o.numero, o.cliente, o.cliente_nome, o.assunto, o.data_emissao,
                        o.validade, o.desconto_percentual, o.estado,
                        COALESCE((SELECT SUM(i.total) FROM orcamentos_item i WHERE i.orcamento = o.id), 0),
                        (SELECT COUNT(*) FROM orcamentos_item i WHERE i.orcamento = o.id)
                 FROM orcamentos_orcamento o
                 WHERE o.empresa = ?1
                 ORDER BY o.numero DESC LIMIT 500"
                    .to_owned(),
                false,
            )
        };
        let _ = filtra_estado;

        let mut stmt = conexao.prepare(&sql).map_err(persist)?;
        let texto = self
            .texto
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_lowercase);

        let linhas = stmt
            .query_map([blob(ctx.empresa)], |r| {
                let liquido_itens = Dinheiro::centavos(r.get::<_, i64>(9)?);
                let desc = Percentual::unidades(r.get::<_, i64>(7)?);
                let total = liquido_itens
                    - liquido_itens.aplicar(desc, cardeal_kernel::Arredondamento::MeioAcima);
                let validade = crate::repositorio::data_de(r.get::<_, i64>(6)?);
                let estado = estado_de(&r.get::<_, String>(8)?);
                Ok(ItemOrcamentoLista {
                    orcamento: crate::repositorio::id_de(r.get::<_, Vec<u8>>(0)?),
                    numero: u64::try_from(r.get::<_, i64>(1)?).unwrap_or(0),
                    cliente_nome: r.get::<_, String>(3)?,
                    assunto: r.get::<_, String>(4)?,
                    data_emissao: crate::repositorio::data_de(r.get::<_, i64>(5)?),
                    validade,
                    total,
                    estado,
                    itens: u32::try_from(r.get::<_, i64>(10)?).unwrap_or(0),
                    vencido: validade < hoje
                        && matches!(
                            estado,
                            EstadoOrcamento::Rascunho | EstadoOrcamento::Enviado
                        ),
                })
            })
            .map_err(persist)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)?;

        // Cliente id não está na linha da grade; filtro por cliente e por texto/período no Rust.
        let cliente_alvo = self.cliente;
        let itens_cliente: Vec<Id> = if let Some(c) = cliente_alvo {
            clientes_com_orcamento(conexao, ctx.empresa, c)?
        } else {
            Vec::new()
        };

        Ok(linhas
            .into_iter()
            .filter(|l| {
                cliente_alvo.is_none() || itens_cliente.contains(&l.orcamento)
            })
            .filter(|l| {
                texto.as_ref().is_none_or(|t| {
                    l.assunto.to_lowercase().contains(t)
                        || l.cliente_nome.to_lowercase().contains(t)
                })
            })
            .filter(|l| self.desde.is_none_or(|d| l.data_emissao >= d))
            .filter(|l| self.ate.is_none_or(|d| l.data_emissao <= d))
            .collect())
    }
}

fn clientes_com_orcamento(conexao: &Connection, empresa: Id, cliente: Id) -> Resultado<Vec<Id>> {
    let mut stmt = conexao
        .prepare(
            "SELECT id FROM orcamentos_orcamento WHERE empresa = ?1 AND cliente = ?2",
        )
        .map_err(persist)?;
    let linhas = stmt
        .query_map(rusqlite::params![blob(empresa), blob(cliente)], |r| {
            Ok(crate::repositorio::id_de(r.get::<_, Vec<u8>>(0)?))
        })
        .map_err(persist)?;
    linhas.collect::<rusqlite::Result<Vec<_>>>().map_err(persist)
}

/// O detalhe completo de um orçamento — cabeçalho + itens.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetalheOrcamento {
    /// O orçamento.
    pub orcamento: Orcamento,
    /// Os itens, na ordem digitada.
    pub itens: Vec<ItemOrcamento>,
    /// Soma bruta dos itens.
    pub subtotal: Dinheiro,
    /// Desconto total (itens + cabeçalho).
    pub desconto: Dinheiro,
    /// Total líquido.
    pub total: Dinheiro,
}

/// Busca o detalhe de um orçamento pelo id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuscarOrcamento {
    /// O orçamento.
    pub orcamento: Id,
}

impl Consulta for BuscarOrcamento {
    type Saida = Option<DetalheOrcamento>;
    const PERMISSAO: &'static str = "orcamentos.orcamento.ver";

    fn executar(self, _ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let Some(orcamento) = buscar_orcamento(conexao, self.orcamento)? else {
            return Ok(None);
        };
        let itens = itens_do_orcamento(conexao, self.orcamento)?;
        Ok(Some(DetalheOrcamento {
            subtotal: Orcamento::subtotal(&itens),
            desconto: orcamento.desconto_total(&itens),
            total: orcamento.total(&itens),
            orcamento,
            itens,
        }))
    }
}

/// Indicadores para os cartões da aba de orçamentos.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumoOrcamentosSaida {
    /// Quantos orçamentos em aberto (Rascunho + Enviado).
    pub abertos: u32,
    /// Valor somado dos orçamentos em aberto.
    pub valor_aberto: Dinheiro,
    /// Quantos aprovados nos últimos 30 dias.
    pub aprovados_periodo: u32,
    /// Valor aprovado nos últimos 30 dias.
    pub valor_aprovado_periodo: Dinheiro,
    /// Taxa de conversão (aprovados / decididos) em pontos percentuais, 0..=100.
    pub taxa_conversao: Percentual,
}

/// Calcula o [`ResumoOrcamentosSaida`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumoOrcamentos;

impl Consulta for ResumoOrcamentos {
    type Saida = ResumoOrcamentosSaida;
    const PERMISSAO: &'static str = "orcamentos.orcamento.ver";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let corte = ctx.hoje().mais_dias(-30);
        let mut stmt = conexao
            .prepare(
                "SELECT o.estado, o.data_emissao, o.desconto_percentual,
                        COALESCE((SELECT SUM(i.total) FROM orcamentos_item i WHERE i.orcamento = o.id), 0)
                 FROM orcamentos_orcamento o WHERE o.empresa = ?1",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map([blob(ctx.empresa)], |r| {
                let liquido = Dinheiro::centavos(r.get::<_, i64>(3)?);
                let desc = Percentual::unidades(r.get::<_, i64>(2)?);
                let total =
                    liquido - liquido.aplicar(desc, cardeal_kernel::Arredondamento::MeioAcima);
                Ok((
                    estado_de(&r.get::<_, String>(0)?),
                    crate::repositorio::data_de(r.get::<_, i64>(1)?),
                    total,
                ))
            })
            .map_err(persist)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)?;

        let mut r = ResumoOrcamentosSaida {
            abertos: 0,
            valor_aberto: Dinheiro::ZERO,
            aprovados_periodo: 0,
            valor_aprovado_periodo: Dinheiro::ZERO,
            taxa_conversao: Percentual::ZERO,
        };
        let (mut decididos, mut aprovados) = (0_i64, 0_i64);
        for (estado, emissao, total) in linhas {
            match estado {
                EstadoOrcamento::Rascunho | EstadoOrcamento::Enviado => {
                    r.abertos += 1;
                    r.valor_aberto += total;
                }
                EstadoOrcamento::Aprovado | EstadoOrcamento::Convertido => {
                    decididos += 1;
                    aprovados += 1;
                    if emissao >= corte {
                        r.aprovados_periodo += 1;
                        r.valor_aprovado_periodo += total;
                    }
                }
                EstadoOrcamento::Recusado => decididos += 1,
                EstadoOrcamento::Expirado | EstadoOrcamento::Cancelado => {}
            }
        }
        r.taxa_conversao = Percentual::da_razao(aprovados, decididos.max(1));
        Ok(r)
    }
}
