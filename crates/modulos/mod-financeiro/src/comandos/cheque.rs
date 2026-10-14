//! Os comandos da carteira de cheques e a consulta dela (`crate::cheque`).
//!
//! - `RegistrarCheque`: dá a baixa na parcela **em Cheques a receber** (D 1.2.03 / C Clientes)
//!   e guarda o cheque `EmCarteira`.
//! - `CompensarCheque`: D Banco / C Cheques a receber, na data do depósito — um movimento da
//!   conta que a conciliação casa com a linha do extrato.
//! - `DevolverCheque`: estorna a compensação (se houve) e a baixa; a parcela volta a ficar em
//!   aberto e o evento `financeiro.cheque_devolvido.v1` sai.

#[cfg(feature = "sqlite")]
use cardeal_kernel::{CodigoErro, Erro, Resultado};
use cardeal_kernel::{Data, Dinheiro, Id};
#[cfg(feature = "sqlite")]
use cardeal_ledger::{ConstrutorLancamento, Contas, Origem, PapelConta, Razao, RepositorioRazao};
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::{Comando, Consulta, Risco};
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
#[cfg(feature = "sqlite")]
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::cheque::{Cheque, DadosDoCheque, EstadoCheque};
#[cfg(feature = "sqlite")]
use crate::repositorio::{blob, data_de, id_de, persist};

#[cfg(feature = "sqlite")]
const COLUNAS: &str = "c.id, c.empresa, c.parcela, c.baixa, c.banco, c.agencia, c.conta, c.numero,
     c.emitente, c.documento_emitente, c.valor, c.recebido_em, c.bom_para, c.estado,
     c.conta_deposito, c.compensado_em, c.lancamento_compensacao, c.devolvido_em, c.alinea,
     c.motivo";

#[cfg(feature = "sqlite")]
fn cheque_de(r: &rusqlite::Row<'_>) -> rusqlite::Result<Cheque> {
    let id_opt = |i: usize| -> rusqlite::Result<Option<Id>> {
        Ok(r.get::<_, Option<Vec<u8>>>(i)?.map(id_de))
    };
    Ok(Cheque {
        id: id_de(r.get(0)?),
        empresa: id_de(r.get(1)?),
        parcela: id_de(r.get(2)?),
        baixa: id_de(r.get(3)?),
        dados: DadosDoCheque {
            banco: r.get(4)?,
            agencia: r.get(5)?,
            conta: r.get(6)?,
            numero: r.get(7)?,
            emitente: r.get(8)?,
            documento_emitente: r.get(9)?,
        },
        valor: Dinheiro::centavos(r.get(10)?),
        recebido_em: data_de(r.get(11)?),
        bom_para: data_de(r.get(12)?),
        estado: EstadoCheque::de(&r.get::<_, String>(13)?),
        conta_deposito: id_opt(14)?,
        compensado_em: r.get::<_, Option<i64>>(15)?.map(data_de),
        lancamento_compensacao: id_opt(16)?,
        devolvido_em: r.get::<_, Option<i64>>(17)?.map(data_de),
        alinea: r
            .get::<_, Option<i64>>(18)?
            .and_then(|a| u8::try_from(a).ok()),
        motivo: r.get(19)?,
    })
}

#[cfg(feature = "sqlite")]
fn carregar(conn: &Connection, empresa: Id, id: Id) -> Resultado<Cheque> {
    conn.query_row(
        &format!("SELECT {COLUNAS} FROM financeiro_cheque c WHERE c.empresa = ?1 AND c.id = ?2"),
        params![blob(empresa), blob(id)],
        cheque_de,
    )
    .optional()
    .map_err(persist)?
    .ok_or_else(|| Erro::nao_encontrado("cheque"))
}

#[cfg(feature = "sqlite")]
fn gravar(conn: &Connection, c: &Cheque, agora: cardeal_kernel::Instante) -> Resultado<()> {
    let dia = |d: Data| i64::from(d.em_dias());
    conn.execute(
        "INSERT INTO financeiro_cheque
            (id, empresa, parcela, baixa, banco, agencia, conta, numero, emitente,
             documento_emitente, valor, recebido_em, bom_para, estado, conta_deposito,
             compensado_em, lancamento_compensacao, devolvido_em, alinea, motivo, criado_em)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21)
         ON CONFLICT (id) DO UPDATE SET
            estado = excluded.estado, conta_deposito = excluded.conta_deposito,
            compensado_em = excluded.compensado_em,
            lancamento_compensacao = excluded.lancamento_compensacao,
            devolvido_em = excluded.devolvido_em, alinea = excluded.alinea,
            motivo = excluded.motivo",
        params![
            blob(c.id),
            blob(c.empresa),
            blob(c.parcela),
            blob(c.baixa),
            c.dados.banco,
            c.dados.agencia,
            c.dados.conta,
            c.dados.numero,
            c.dados.emitente,
            c.dados.documento_emitente,
            c.valor.em_centavos(),
            dia(c.recebido_em),
            dia(c.bom_para),
            c.estado.txt(),
            c.conta_deposito.map(blob),
            c.compensado_em.map(dia),
            c.lancamento_compensacao.map(blob),
            c.devolvido_em.map(dia),
            c.alinea.map(i64::from),
            c.motivo,
            agora.em_micros(),
        ],
    )
    .map_err(persist)?;
    Ok(())
}

#[cfg(feature = "sqlite")]
fn conta_cheques_a_receber(ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Id> {
    let repo = RepositorioRazao::novo(uow);
    Contas::nova(&repo, ctx.empresa)
        .papel(PapelConta::ChequesAReceber)
        .map_err(|e| Erro::de_dominio(&e))
}

/// Registra um cheque recebido como pagamento de uma parcela a receber.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrarCheque {
    /// A parcela que ele paga.
    pub parcela: Id,
    /// O valor do cheque.
    pub valor: Dinheiro,
    /// Quando foi recebido (a data da baixa); não pode ser futura.
    pub recebido_em: Data,
    /// A partir de quando pode ser depositado; o próprio recebimento, se à vista.
    pub bom_para: Data,
    /// O que está escrito no cheque.
    pub dados: DadosDoCheque,
}

/// O cheque registrado.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ChequeRegistrado {
    /// O cheque.
    pub cheque: Id,
    /// A baixa (em Cheques a receber).
    pub baixa: Id,
    /// Se a parcela ficou quitada.
    pub parcela_quitada: bool,
}

impl Comando for RegistrarCheque {
    type Saida = ChequeRegistrado;
    const PERMISSAO: &'static str = "financeiro.cheque.registrar";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        use crate::erros::ErroFinanceiro;
        let dados = self.dados.normalizado().map_err(|e| Erro::de_dominio(&e))?;
        if self.recebido_em > ctx.hoje() {
            return Err(Erro::de_dominio(&ErroFinanceiro::DataDoChequeInvalida(
                "o recebimento não pode ser futuro",
            )));
        }
        let repetido: Option<i64> = uow
            .conexao()
            .query_row(
                "SELECT 1 FROM financeiro_cheque
                 WHERE empresa = ?1 AND banco = ?2 AND agencia = ?3 AND conta = ?4 AND numero = ?5
                   AND estado <> 'Devolvido'",
                params![
                    blob(ctx.empresa),
                    dados.banco,
                    dados.agencia,
                    dados.conta,
                    dados.numero
                ],
                |r| r.get(0),
            )
            .optional()
            .map_err(persist)?;
        if repetido.is_some() {
            return Err(Erro::de_dominio(&ErroFinanceiro::ChequeDuplicado));
        }
        let destino = conta_cheques_a_receber(ctx, uow)?;
        let baixa = crate::baixar_recebimento_comum(
            self.parcela,
            self.valor,
            self.recebido_em,
            crate::MeioPagamento::Dinheiro,
            Some(destino),
            ctx,
            uow,
        )?;
        let cheque = Cheque {
            id: Id::novo(),
            empresa: ctx.empresa,
            parcela: self.parcela,
            baixa: baixa.baixa,
            dados,
            valor: self.valor,
            recebido_em: self.recebido_em,
            bom_para: self.bom_para.max(self.recebido_em),
            estado: EstadoCheque::EmCarteira,
            conta_deposito: None,
            compensado_em: None,
            lancamento_compensacao: None,
            devolvido_em: None,
            alinea: None,
            motivo: None,
        };
        gravar(uow.conexao(), &cheque, ctx.agora)?;
        Ok(ChequeRegistrado {
            cheque: cheque.id,
            baixa: baixa.baixa,
            parcela_quitada: baixa.parcela_quitada,
        })
    }
}

/// O banco compensou o cheque: o dinheiro entrou na conta.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CompensarCheque {
    /// O cheque.
    pub cheque: Id,
    /// A conta bancária (grupo 1.1 Disponível) onde foi depositado.
    pub conta: Id,
    /// A data da compensação; entre o recebimento e hoje.
    pub data: Data,
}

/// A compensação feita.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ChequeCompensado {
    /// O lançamento D Banco / C Cheques a receber.
    pub lancamento: Id,
}

impl Comando for CompensarCheque {
    type Saida = ChequeCompensado;
    const PERMISSAO: &'static str = "financeiro.cheque.compensar";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        use crate::erros::ErroFinanceiro;
        let mut cheque = carregar(uow.conexao(), ctx.empresa, self.cheque)?;
        if self.data > ctx.hoje() {
            return Err(Erro::de_dominio(&ErroFinanceiro::DataDoChequeInvalida(
                "a compensação não pode ser futura",
            )));
        }
        let disponivel: Option<(String, Option<String>)> = uow
            .conexao()
            .query_row(
                "SELECT codigo, papel_padrao FROM razao_conta
                 WHERE empresa = ?1 AND id = ?2 AND tipo = 'Analitica' AND ativa = 1",
                params![blob(ctx.empresa), blob(self.conta)],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(persist)?;
        if !disponivel.is_some_and(|(codigo, papel)| {
            codigo.starts_with("1.1.") && papel.as_deref() != Some("Aplicacoes")
        }) {
            return Err(Erro::novo(
                CodigoErro::ENTRADA_INVALIDA,
                "o cheque é depositado numa conta de banco ou no caixa (grupo 1.1 Disponível)",
            ));
        }
        let cheques_a_receber = conta_cheques_a_receber(ctx, uow)?;
        let lancamento = ConstrutorLancamento::novo(
            ctx.empresa,
            self.data,
            format!(
                "Compensação do cheque nº {} — {}",
                cheque.dados.numero, cheque.dados.emitente
            ),
        )
        .origem(
            Origem::modulo("financeiro")
                .tipo("cheque")
                .agregado(cheque.id),
        )
        .liquidacao(self.data)
        .criado_por(ctx.usuario, ctx.dispositivo)
        .agora(ctx.agora)
        .debitar(self.conta, cheque.valor)
        .creditar(cheques_a_receber, cheque.valor)
        .construir()
        .map_err(|e| Erro::de_dominio(&e))?;
        let lancamento = Razao::registrar(&mut RepositorioRazao::novo(uow), lancamento)
            .map_err(|e| Erro::de_dominio(&e))?;
        cheque
            .compensar(self.conta, self.data, lancamento)
            .map_err(|e| Erro::de_dominio(&e))?;
        gravar(uow.conexao(), &cheque, ctx.agora)?;
        Ok(ChequeCompensado { lancamento })
    }
}

/// O cheque voltou (sem fundos, sustado…): estorna a compensação, se houve, e a baixa — a
/// parcela volta a ficar em aberto.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevolverCheque {
    /// O cheque.
    pub cheque: Id,
    /// A alínea que o banco informou (11, 12, 21…), se houver.
    pub alinea: Option<u8>,
    /// Observação livre.
    pub observacao: String,
}

/// A devolução feita.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ChequeFoiDevolvido {
    /// A parcela reaberta.
    pub parcela: Id,
    /// O estorno da compensação, se o cheque já tinha compensado.
    pub estorno_compensacao: Option<Id>,
    /// O estorno da baixa.
    pub estorno_baixa: Id,
}

impl Comando for DevolverCheque {
    type Saida = ChequeFoiDevolvido;
    const PERMISSAO: &'static str = "financeiro.cheque.devolver";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let mut cheque = carregar(uow.conexao(), ctx.empresa, self.cheque)?;
        let compensacao = cheque.lancamento_compensacao;
        let alinea = self.alinea.map_or_else(String::new, |a| {
            crate::cheque::descricao_alinea(a).map_or_else(
                || format!(" — alínea {a}"),
                |d| format!(" — alínea {a} ({d})"),
            )
        });
        let observacao = match self.observacao.trim() {
            "" => String::new(),
            o => format!(": {o}"),
        };
        let motivo = format!(
            "Cheque nº {} devolvido{alinea}{observacao}",
            cheque.dados.numero
        );
        cheque
            .devolver(ctx.hoje(), self.alinea, motivo.clone())
            .map_err(|e| Erro::de_dominio(&e))?;
        let estorno_compensacao = match compensacao {
            Some(l) => Some(
                Razao::estornar(&mut RepositorioRazao::novo(uow), l, &motivo, ctx.hoje())
                    .map_err(|e| Erro::de_dominio(&e))?,
            ),
            None => None,
        };
        let estorno = crate::estornar_baixa_comum(cheque.baixa, &motivo, ctx, uow)?;
        gravar(uow.conexao(), &cheque, ctx.agora)?;
        let titulo = crate::repositorio::parcela_por_id(uow.conexao(), cheque.parcela)?
            .map_or(Id::NULO, |p| p.titulo);
        uow.publicar(crate::eventos::ChequeDevolvido {
            titulo,
            cheque: cheque.id,
            motivo,
        })
        .map_err(|e| Erro::de_dominio(&e))?;
        Ok(ChequeFoiDevolvido {
            parcela: estorno.parcela,
            estorno_compensacao,
            estorno_baixa: estorno.lancamento_estorno,
        })
    }
}

/// Um cheque na carteira, com quem deve (o cliente do título) e o que ele pagou.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemCheque {
    /// O cheque.
    pub cheque: Cheque,
    /// O cliente do título, se houver.
    pub cliente: Option<Id>,
    /// A descrição do título.
    pub descricao: Option<String>,
}

/// A carteira de cheques, os mais urgentes (bom para mais antigo) primeiro.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CarteiraDeCheques {
    /// Só este estado; `None` = todos.
    pub estado: Option<EstadoCheque>,
}

impl Consulta for CarteiraDeCheques {
    type Saida = Vec<ItemCheque>;
    const PERMISSAO: &'static str = "financeiro.cheque.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let mut stmt = conexao
            .prepare(&format!(
                "SELECT {COLUNAS}, t.contraparte_id, t.observacao
                 FROM financeiro_cheque c
                 JOIN financeiro_parcela p ON p.id = c.parcela
                 JOIN financeiro_titulo t ON t.id = p.titulo
                 WHERE c.empresa = ?1 AND (?2 IS NULL OR c.estado = ?2)
                 ORDER BY c.bom_para, c.recebido_em LIMIT 2000"
            ))
            .map_err(persist)?;
        let linhas = stmt
            .query_map(
                params![blob(ctx.empresa), self.estado.map(EstadoCheque::txt)],
                |r| {
                    Ok(ItemCheque {
                        cheque: cheque_de(r)?,
                        cliente: r.get::<_, Option<Vec<u8>>>(20)?.map(id_de),
                        descricao: r.get(21)?,
                    })
                },
            )
            .map_err(persist)?;
        linhas
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)
    }
}
