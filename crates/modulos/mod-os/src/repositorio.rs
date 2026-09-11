//! Todo o SQL do módulo de OS (`docs/15-convencoes-codigo.md` §5): um único lugar com
//! strings SQL, sempre colunas explícitas, sempre parâmetros posicionais.
//!
//! [`RepositorioOs`] grava e lê `os_ordem_servico`/`os_laudo_tecnico`/`os_item_peca`/
//! `os_item_mao_de_obra` sobre a [`UnidadeDeTrabalho`](cardeal_storage::UnidadeDeTrabalho)
//! do escritor único — mesmo padrão do `RepositorioFinanceiro`.

use cardeal_kernel::{
    CodigoErro, Data, Dinheiro, Erro, Id, Instante, Preco, Quantidade, Resultado, Versao,
};
use cardeal_storage::UnidadeDeTrabalho;
use rusqlite::{params, Connection, OptionalExtension};

use crate::apontamento::ApontamentoDeTempo;
use crate::execucao::{ItemMaoDeObra, ItemPeca};
use crate::laudo::LaudoTecnico;
use crate::ordem::{EstadoOs, OrdemServico};

#[allow(clippy::needless_pass_by_value)] // usado como `.map_err(persist)`
pub(crate) fn persist(e: rusqlite::Error) -> Erro {
    Erro::novo(CodigoErro::FALHA_INTERNA, format!("os/SQL: {e}"))
}

pub(crate) fn blob(id: Id) -> Vec<u8> {
    id.em_bytes().to_vec()
}

pub(crate) fn id_de(bytes: Vec<u8>) -> Id {
    <[u8; 16]>::try_from(bytes).map_or(Id::NULO, Id::de_bytes)
}

fn dias(d: Data) -> i64 {
    i64::from(d.em_dias())
}

pub(crate) fn data_de(dias: i64) -> Data {
    Data::de_dias(i32::try_from(dias).unwrap_or(0))
}

fn versao_de(n: i64) -> Versao {
    Versao::nova(u64::try_from(n).unwrap_or(1))
}

fn versao_i64(v: Versao) -> i64 {
    i64::try_from(v.numero()).unwrap_or(i64::MAX)
}

pub(crate) fn estado_txt(e: EstadoOs) -> &'static str {
    e.rotulo()
}

pub(crate) fn estado_de(s: &str) -> EstadoOs {
    match s {
        "EmDiagnostico" => EstadoOs::EmDiagnostico,
        "AguardandoAprovacao" => EstadoOs::AguardandoAprovacao,
        "Aprovada" => EstadoOs::Aprovada,
        "Reprovada" => EstadoOs::Reprovada,
        "EmExecucao" => EstadoOs::EmExecucao,
        "Concluida" => EstadoOs::Concluida,
        "Faturada" => EstadoOs::Faturada,
        "Cancelada" => EstadoOs::Cancelada,
        _ => EstadoOs::Aberta,
    }
}

/// Grava e lê ordens de serviço, laudos e itens de orçamento.
pub struct RepositorioOs<'a, 'b> {
    uow: &'a mut UnidadeDeTrabalho<'b>,
}

impl<'a, 'b> RepositorioOs<'a, 'b> {
    /// Cria o repositório sobre a unidade de trabalho corrente.
    pub fn novo(uow: &'a mut UnidadeDeTrabalho<'b>) -> Self {
        Self { uow }
    }

    fn conn(&self) -> &Connection {
        self.uow.conexao()
    }

    /// O próximo número de OS da empresa.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn proximo_numero(&mut self) -> Resultado<u64> {
        self.uow
            .proximo_numero("os.ordem_servico")
            .map_err(|e| Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string()))
    }

    /// Grava uma ordem de serviço recém-aberta.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn inserir_ordem(&mut self, os: &OrdemServico) -> Resultado<()> {
        self.conn()
            .execute(
                "INSERT INTO os_ordem_servico
                   (id, empresa, numero, cliente, equipamento, data_abertura,
                    tecnico_responsavel, estado, aprovado_por, garantia_dias, valor_total,
                    itens_orcamento, versao)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
                params![
                    blob(os.id),
                    blob(os.empresa),
                    i64::try_from(os.numero).unwrap_or(i64::MAX),
                    blob(os.cliente),
                    os.equipamento,
                    dias(os.data_abertura),
                    blob(os.tecnico_responsavel),
                    estado_txt(os.estado),
                    os.aprovado_por,
                    i64::from(os.garantia_dias),
                    os.valor_total.em_centavos(),
                    i64::from(os.itens_orcamento),
                    versao_i64(os.versao),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Regrava uma ordem de serviço (transição de estado, total atualizado).
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn atualizar_ordem(&mut self, os: &OrdemServico) -> Resultado<()> {
        self.conn()
            .execute(
                "UPDATE os_ordem_servico
                 SET estado = ?2, aprovado_por = ?3, valor_total = ?4, itens_orcamento = ?5,
                     versao = ?6
                 WHERE id = ?1",
                params![
                    blob(os.id),
                    estado_txt(os.estado),
                    os.aprovado_por,
                    os.valor_total.em_centavos(),
                    i64::from(os.itens_orcamento),
                    versao_i64(os.versao),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Busca uma ordem de serviço pelo id. `Ok(None)` = não existe.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn buscar_ordem(&self, id: Id) -> Resultado<Option<OrdemServico>> {
        crate::consultas::buscar_ordem(self.conn(), id)
    }

    /// Grava um laudo técnico.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn inserir_laudo(&mut self, l: &LaudoTecnico) -> Resultado<()> {
        self.conn()
            .execute(
                "INSERT INTO os_laudo_tecnico
                   (id, ordem_servico, descricao_problema, diagnostico, tecnico, criado_em)
                 VALUES (?1,?2,?3,?4,?5,?6)",
                params![
                    blob(l.id),
                    blob(l.ordem_servico),
                    l.descricao_problema,
                    l.diagnostico,
                    blob(l.tecnico),
                    l.criado_em.em_micros(),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Grava um item de peça orçado.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn inserir_item_peca(&mut self, item: &ItemPeca) -> Resultado<()> {
        self.conn()
            .execute(
                "INSERT INTO os_item_peca
                   (id, ordem_servico, produto, quantidade, preco_unitario, custo_unitario,
                    coberto_garantia, aplicada)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                params![
                    blob(item.id),
                    blob(item.ordem_servico),
                    blob(item.produto),
                    item.quantidade.unidades_internas(),
                    item.preco_unitario.unidades_internas(),
                    item.custo_unitario.unidades_internas(),
                    i64::from(item.coberto_garantia),
                    i64::from(item.aplicada),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Regrava um item de peça (depois de `AplicarPeca`).
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn atualizar_item_peca(&mut self, item: &ItemPeca) -> Resultado<()> {
        self.conn()
            .execute(
                "UPDATE os_item_peca SET custo_unitario = ?2, aplicada = ?3 WHERE id = ?1",
                params![
                    blob(item.id),
                    item.custo_unitario.unidades_internas(),
                    i64::from(item.aplicada),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Busca um item de peça pelo id. `Ok(None)` = não existe.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn buscar_item_peca(&self, id: Id) -> Resultado<Option<ItemPeca>> {
        self.conn()
            .query_row(
                "SELECT id, ordem_servico, produto, quantidade, preco_unitario, custo_unitario,
                        coberto_garantia, aplicada
                 FROM os_item_peca WHERE id = ?1",
                [blob(id)],
                item_peca_de_linha,
            )
            .optional()
            .map_err(persist)
    }

    /// Todos os itens de peça de uma ordem de serviço.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn itens_peca_da_ordem(&self, ordem_servico: Id) -> Resultado<Vec<ItemPeca>> {
        crate::consultas::itens_peca_da_ordem(self.conn(), ordem_servico)
    }

    /// Remove um item de peça do orçamento — só chamado enquanto a OS ainda aceita edição
    /// (`OrdemServico::aceita_edicao_de_orcamento`), então nunca uma peça já aplicada.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn remover_item_peca(&mut self, id: Id) -> Resultado<()> {
        self.conn()
            .execute("DELETE FROM os_item_peca WHERE id = ?1", [blob(id)])
            .map_err(persist)?;
        Ok(())
    }

    /// Busca um item de mão de obra pelo id. `Ok(None)` = não existe.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn buscar_item_mao_de_obra(&self, id: Id) -> Resultado<Option<ItemMaoDeObra>> {
        self.conn()
            .query_row(
                "SELECT id, ordem_servico, descricao, valor, tecnico, horas
                 FROM os_item_mao_de_obra WHERE id = ?1",
                [blob(id)],
                item_mao_de_obra_de_linha,
            )
            .optional()
            .map_err(persist)
    }

    /// Remove um item de mão de obra do orçamento.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn remover_item_mao_de_obra(&mut self, id: Id) -> Resultado<()> {
        self.conn()
            .execute("DELETE FROM os_item_mao_de_obra WHERE id = ?1", [blob(id)])
            .map_err(persist)?;
        Ok(())
    }

    /// Grava um item de mão de obra.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn inserir_item_mao_de_obra(&mut self, item: &ItemMaoDeObra) -> Resultado<()> {
        self.conn()
            .execute(
                "INSERT INTO os_item_mao_de_obra
                   (id, ordem_servico, descricao, valor, tecnico, horas)
                 VALUES (?1,?2,?3,?4,?5,?6)",
                params![
                    blob(item.id),
                    blob(item.ordem_servico),
                    item.descricao,
                    item.valor.em_centavos(),
                    blob(item.tecnico),
                    item.horas.map(Quantidade::unidades_internas),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Todos os itens de mão de obra de uma ordem de serviço.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn itens_mao_de_obra_da_ordem(&self, ordem_servico: Id) -> Resultado<Vec<ItemMaoDeObra>> {
        crate::consultas::itens_mao_de_obra_da_ordem(self.conn(), ordem_servico)
    }

    /// Quantos itens de peça do orçamento ainda não foram aplicados.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn contar_pecas_nao_aplicadas(&self, ordem_servico: Id) -> Resultado<usize> {
        let n: i64 = self
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM os_item_peca WHERE ordem_servico = ?1 AND aplicada = 0",
                [blob(ordem_servico)],
                |r| r.get(0),
            )
            .map_err(persist)?;
        Ok(usize::try_from(n).unwrap_or(0))
    }

    /// Grava um apontamento de tempo (aberto ou já com os campos definidos).
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn inserir_apontamento(&mut self, ap: &ApontamentoDeTempo) -> Resultado<()> {
        self.conn()
            .execute(
                "INSERT INTO os_apontamento_tempo
                   (id, ordem_servico, tecnico, inicio, fim, ajustado, motivo_ajuste, versao)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                params![
                    blob(ap.id),
                    blob(ap.ordem_servico),
                    blob(ap.tecnico),
                    ap.inicio.em_micros(),
                    ap.fim.map(Instante::em_micros),
                    i64::from(ap.ajustado),
                    ap.motivo_ajuste,
                    versao_i64(ap.versao),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Regrava um apontamento (encerrado ou ajustado).
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn atualizar_apontamento(&mut self, ap: &ApontamentoDeTempo) -> Resultado<()> {
        self.conn()
            .execute(
                "UPDATE os_apontamento_tempo
                 SET inicio = ?2, fim = ?3, ajustado = ?4, motivo_ajuste = ?5, versao = ?6
                 WHERE id = ?1",
                params![
                    blob(ap.id),
                    ap.inicio.em_micros(),
                    ap.fim.map(Instante::em_micros),
                    i64::from(ap.ajustado),
                    ap.motivo_ajuste,
                    versao_i64(ap.versao),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Busca um apontamento pelo id. `Ok(None)` = não existe.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn buscar_apontamento(&self, id: Id) -> Resultado<Option<ApontamentoDeTempo>> {
        self.conn()
            .query_row(
                "SELECT id, ordem_servico, tecnico, inicio, fim, ajustado, motivo_ajuste, versao
                 FROM os_apontamento_tempo WHERE id = ?1",
                [blob(id)],
                apontamento_de_linha,
            )
            .optional()
            .map_err(persist)
    }

    /// O apontamento aberto do técnico, se houver — em QUALQUER ordem de serviço, é essa
    /// varredura que impede um técnico de "trabalhar" em duas OS ao mesmo tempo.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn apontamento_aberto_do_tecnico(
        &self,
        tecnico: Id,
    ) -> Resultado<Option<ApontamentoDeTempo>> {
        self.conn()
            .query_row(
                "SELECT id, ordem_servico, tecnico, inicio, fim, ajustado, motivo_ajuste, versao
                 FROM os_apontamento_tempo WHERE tecnico = ?1 AND fim IS NULL",
                [blob(tecnico)],
                apontamento_de_linha,
            )
            .optional()
            .map_err(persist)
    }

    /// Todos os apontamentos de uma ordem de serviço, mais antigo primeiro.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn apontamentos_da_ordem(&self, ordem_servico: Id) -> Resultado<Vec<ApontamentoDeTempo>> {
        crate::consultas::apontamentos_da_ordem(self.conn(), ordem_servico)
    }
}

pub(crate) fn ordem_de_linha(r: &rusqlite::Row<'_>) -> rusqlite::Result<OrdemServico> {
    Ok(OrdemServico {
        id: id_de(r.get::<_, Vec<u8>>(0)?),
        empresa: id_de(r.get::<_, Vec<u8>>(1)?),
        numero: u64::try_from(r.get::<_, i64>(2)?).unwrap_or(0),
        cliente: id_de(r.get::<_, Vec<u8>>(3)?),
        equipamento: r.get(4)?,
        data_abertura: data_de(r.get::<_, i64>(5)?),
        tecnico_responsavel: id_de(r.get::<_, Vec<u8>>(6)?),
        estado: estado_de(&r.get::<_, String>(7)?),
        aprovado_por: r.get::<_, Option<String>>(8)?,
        garantia_dias: u16::try_from(r.get::<_, i64>(9)?).unwrap_or(0),
        valor_total: Dinheiro::centavos(r.get::<_, i64>(10)?),
        itens_orcamento: u32::try_from(r.get::<_, i64>(11)?).unwrap_or(0),
        versao: versao_de(r.get::<_, i64>(12)?),
    })
}

pub(crate) fn item_mao_de_obra_de_linha(r: &rusqlite::Row<'_>) -> rusqlite::Result<ItemMaoDeObra> {
    Ok(ItemMaoDeObra {
        id: id_de(r.get::<_, Vec<u8>>(0)?),
        ordem_servico: id_de(r.get::<_, Vec<u8>>(1)?),
        descricao: r.get(2)?,
        valor: Dinheiro::centavos(r.get::<_, i64>(3)?),
        tecnico: id_de(r.get::<_, Vec<u8>>(4)?),
        horas: r.get::<_, Option<i64>>(5)?.map(Quantidade::interna),
    })
}

pub(crate) fn item_peca_de_linha(r: &rusqlite::Row<'_>) -> rusqlite::Result<ItemPeca> {
    Ok(ItemPeca {
        id: id_de(r.get::<_, Vec<u8>>(0)?),
        ordem_servico: id_de(r.get::<_, Vec<u8>>(1)?),
        produto: id_de(r.get::<_, Vec<u8>>(2)?),
        quantidade: Quantidade::interna(r.get::<_, i64>(3)?),
        preco_unitario: Preco::interna(r.get::<_, i64>(4)?),
        custo_unitario: Preco::interna(r.get::<_, i64>(5)?),
        coberto_garantia: r.get::<_, i64>(6)? != 0,
        aplicada: r.get::<_, i64>(7)? != 0,
    })
}

pub(crate) fn apontamento_de_linha(r: &rusqlite::Row<'_>) -> rusqlite::Result<ApontamentoDeTempo> {
    Ok(ApontamentoDeTempo {
        id: id_de(r.get::<_, Vec<u8>>(0)?),
        ordem_servico: id_de(r.get::<_, Vec<u8>>(1)?),
        tecnico: id_de(r.get::<_, Vec<u8>>(2)?),
        inicio: Instante::de_micros(r.get::<_, i64>(3)?),
        fim: r.get::<_, Option<i64>>(4)?.map(Instante::de_micros),
        ajustado: r.get::<_, i64>(5)? != 0,
        motivo_ajuste: r.get::<_, Option<String>>(6)?,
        versao: versao_de(r.get::<_, i64>(7)?),
    })
}
