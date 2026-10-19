//! A conferência do saldo bancário e o alerta da conciliação parada.
//!
//! **Saldo do banco × sistema:** o saldo que o banco informa (lido do OFX — `LEDGERBAL` — ou
//! digitado, para quem importa CSV) contra o saldo do Razão na mesma data. A diferença se
//! explica pelo que está só de um lado: linhas do extrato ainda pendentes ou ignoradas (estão no
//! banco, não no sistema) e baixas nesta conta, dentro do período que os extratos cobrem, que o
//! banco ainda não mostrou (estão no sistema, não no banco). O que sobra é a diferença **sem
//! explicação** — saldo inicial não lançado, lançamento na conta errada, linha que falta no
//! extrato. Sem diferença e sem nada pendente, a conta está fechada.
//!
//! **Alerta:** linhas pendentes há mais de N dias, extrato sem importar há mais de N dias e
//! diferença sem explicação.

use cardeal_kernel::{Data, Dinheiro, Id};
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::{Comando, Consulta, Risco};
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use cardeal_kernel::{CodigoErro, Erro, Resultado};
#[cfg(feature = "sqlite")]
use rusqlite::{params, Connection, OptionalExtension};

#[cfg(feature = "sqlite")]
use crate::repositorio::{blob, data_de, id_de, persist};

/// De onde veio o saldo do banco.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrigemDoSaldo {
    /// Lido do OFX importado.
    Extrato,
    /// Digitado pelo usuário.
    Digitado,
}

/// Um saldo informado pelo banco, ao fim do dia `data`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaldoDoBanco {
    /// A data do saldo.
    pub data: Data,
    /// O saldo.
    pub saldo: Dinheiro,
    /// De onde veio.
    pub origem: OrigemDoSaldo,
}

/// Uma soma de linhas: quantas e quanto, com sinal (entrada positiva, saída negativa).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Soma {
    /// Quantas.
    pub quantidade: u32,
    /// Quanto, com sinal.
    pub valor: Dinheiro,
}

/// O saldo do banco conferido com o do sistema, na data do saldo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FechamentoDoSaldo {
    /// O saldo que o banco informou.
    pub banco: SaldoDoBanco,
    /// O saldo do Razão na mesma data (lançamentos realizados até o fim do dia).
    pub sistema: Dinheiro,
    /// Linhas do extrato até a data ainda sem destino: estão no banco, não no sistema.
    pub pendentes: Soma,
    /// Linhas do extrato até a data ignoradas de propósito: no banco, fora do sistema.
    pub ignoradas: Soma,
    /// Baixas e outros movimentos nesta conta (depósito do caixa, transferência, compensação de
    /// cheque), no período coberto pelos extratos, que nenhuma linha conciliou: estão no
    /// sistema, o banco ainda não mostrou.
    pub so_no_sistema: Soma,
}

impl FechamentoDoSaldo {
    /// Banco − sistema.
    #[must_use]
    pub fn diferenca(&self) -> Dinheiro {
        self.banco.saldo - self.sistema
    }

    /// A parte da diferença que nada do que está só de um lado explica.
    #[must_use]
    pub fn sem_explicacao(&self) -> Dinheiro {
        self.diferenca() - self.pendentes.valor - self.ignoradas.valor + self.so_no_sistema.valor
    }

    /// Sem diferença sem explicação e sem nada esperando (pendente ou só no sistema).
    #[must_use]
    pub fn fechado(&self) -> bool {
        self.sem_explicacao().e_zero()
            && self.pendentes.quantidade == 0
            && self.so_no_sistema.quantidade == 0
    }
}

/// Uma conta bancária na conferência.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConferenciaDaConta {
    /// A conta do plano.
    pub conta: Id,
    /// O nome.
    pub nome: String,
    /// O código.
    pub codigo: String,
    /// Linhas do extrato pendentes, de qualquer data.
    pub pendentes: u32,
    /// A data da linha pendente mais antiga.
    pub pendente_mais_antiga: Option<Data>,
    /// A data da última linha importada; `None` = nenhum extrato importado.
    pub ultima_linha: Option<Data>,
    /// A conferência com o último saldo informado; `None` = o banco nunca informou saldo.
    pub fechamento: Option<FechamentoDoSaldo>,
}

impl ConferenciaDaConta {
    /// O que precisa de atenção, em frases curtas; vazio = em dia. `dias` é a tolerância para
    /// linha pendente e para extrato sem importar.
    #[must_use]
    pub fn alertas(&self, hoje: Data, dias: i32) -> Vec<String> {
        let idade = |d: Data| hoje.em_dias() - d.em_dias();
        let mut alertas = Vec::new();
        if let Some(d) = self.pendente_mais_antiga.filter(|d| idade(*d) > dias) {
            alertas.push(format!(
                "{} linha(s) do extrato sem conciliar; a mais antiga é de {} ({} dias)",
                self.pendentes,
                d.formatar(),
                idade(d)
            ));
        }
        if let Some(d) = self.ultima_linha.filter(|d| idade(*d) > dias) {
            alertas.push(format!(
                "Extrato sem importar desde {} ({} dias)",
                d.formatar(),
                idade(d)
            ));
        }
        if let Some(f) = self.fechamento.filter(|f| !f.sem_explicacao().e_zero()) {
            alertas.push(format!(
                "Diferença sem explicação de {} entre o banco e o sistema em {}",
                f.sem_explicacao().formatar_com_simbolo(),
                f.banco.data.formatar()
            ));
        }
        alertas
    }
}

/// As contas bancárias (o Disponível, sem o caixa físico) com a conferência do saldo e as
/// pendências da conciliação.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ConferenciaDasContas;

impl Consulta for ConferenciaDasContas {
    type Saida = Vec<ConferenciaDaConta>;
    const PERMISSAO: &'static str = "financeiro.conciliacao.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let mut stmt = conexao
            .prepare(
                "SELECT id, codigo, nome FROM razao_conta
                 WHERE empresa = ?1 AND tipo = 'Analitica' AND ativa = 1 AND codigo LIKE '1.1.%'
                   AND COALESCE(papel_padrao, '') <> 'Caixa'
                 ORDER BY codigo",
            )
            .map_err(persist)?;
        let contas: Vec<(Id, String, String)> = stmt
            .query_map([blob(ctx.empresa)], |r| {
                Ok((id_de(r.get::<_, Vec<u8>>(0)?), r.get(1)?, r.get(2)?))
            })
            .map_err(persist)?
            .collect::<rusqlite::Result<_>>()
            .map_err(persist)?;
        contas
            .into_iter()
            .map(|(conta, codigo, nome)| conferir(conexao, ctx.empresa, conta, codigo, nome))
            .collect()
    }
}

#[cfg(feature = "sqlite")]
fn soma(r: &rusqlite::Row<'_>, i: usize) -> rusqlite::Result<Soma> {
    Ok(Soma {
        quantidade: u32::try_from(r.get::<_, i64>(i)?).unwrap_or(u32::MAX),
        valor: Dinheiro::centavos(r.get(i + 1)?),
    })
}

#[cfg(feature = "sqlite")]
fn conferir(
    conexao: &Connection,
    empresa: Id,
    conta: Id,
    codigo: String,
    nome: String,
) -> Resultado<ConferenciaDaConta> {
    let chave = params![blob(empresa), blob(conta)];
    let (pendentes, pendente_mais_antiga): (i64, Option<i64>) = conexao
        .query_row(
            "SELECT COUNT(*), MIN(data) FROM financeiro_item_extrato
             WHERE empresa = ?1 AND conta = ?2 AND estado = 'Pendente'",
            chave,
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(persist)?;
    let (primeira_linha, ultima_linha): (Option<i64>, Option<i64>) = conexao
        .query_row(
            "SELECT MIN(data), MAX(data) FROM financeiro_item_extrato
             WHERE empresa = ?1 AND conta = ?2",
            chave,
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(persist)?;
    let banco = conexao
        .query_row(
            "SELECT data, saldo, origem FROM financeiro_saldo_banco
             WHERE empresa = ?1 AND conta = ?2 ORDER BY data DESC LIMIT 1",
            chave,
            |r| {
                Ok(SaldoDoBanco {
                    data: data_de(r.get(0)?),
                    saldo: Dinheiro::centavos(r.get(1)?),
                    origem: if r.get::<_, String>(2)? == "Digitado" {
                        OrigemDoSaldo::Digitado
                    } else {
                        OrigemDoSaldo::Extrato
                    },
                })
            },
        )
        .optional()
        .map_err(persist)?;
    let fechamento = banco
        .map(|b| fechamento(conexao, empresa, conta, b, primeira_linha))
        .transpose()?;
    Ok(ConferenciaDaConta {
        conta,
        nome,
        codigo,
        pendentes: u32::try_from(pendentes).unwrap_or(u32::MAX),
        pendente_mais_antiga: pendente_mais_antiga.map(data_de),
        ultima_linha: ultima_linha.map(data_de),
        fechamento,
    })
}

#[cfg(feature = "sqlite")]
fn fechamento(
    conexao: &Connection,
    empresa: Id,
    conta: Id,
    banco: SaldoDoBanco,
    primeira_linha: Option<i64>,
) -> Resultado<FechamentoDoSaldo> {
    let ate = i64::from(banco.data.em_dias());
    let sistema: i64 = conexao
        .query_row(
            "SELECT COALESCE(SUM(rp.valor), 0) FROM razao_partida rp
             JOIN razao_lancamento rl ON rl.id = rp.lancamento
             WHERE rl.empresa = ?1 AND rp.conta = ?2
               AND rl.estado IN ('Realizado','Estornado')
               AND COALESCE(rl.liquidacao, rl.competencia) <= ?3",
            params![blob(empresa), blob(conta), ate],
            |r| r.get(0),
        )
        .map_err(persist)?;
    let por_estado = |estado: &str| -> Resultado<Soma> {
        conexao
            .query_row(
                "SELECT COUNT(*), COALESCE(SUM(valor), 0) FROM financeiro_item_extrato
                 WHERE empresa = ?1 AND conta = ?2 AND estado = ?3 AND data <= ?4",
                params![blob(empresa), blob(conta), estado, ate],
                |r| soma(r, 0),
            )
            .map_err(persist)
    };
    // Antes do primeiro extrato importado, o banco e o sistema já contam as mesmas baixas
    // (estão no saldo dos dois); só o período coberto pelos extratos pode ter baixa "só no
    // sistema".
    let mut so_no_sistema = match primeira_linha {
        None => Soma::default(),
        Some(desde) => conexao
            .query_row(
                "SELECT COUNT(*), COALESCE(SUM(rp.valor), 0)
                 FROM financeiro_baixa b
                 JOIN razao_partida rp ON rp.lancamento = b.lancamento AND rp.conta = ?2
                 WHERE b.empresa = ?1 AND b.estornada_em IS NULL AND b.data BETWEEN ?3 AND ?4
                   AND NOT EXISTS (SELECT 1 FROM financeiro_item_extrato i
                                   WHERE i.baixa = b.id AND i.estado = 'Conciliado')",
                params![blob(empresa), blob(conta), desde, ate],
                |r| soma(r, 0),
            )
            .map_err(persist)?,
    };
    // E os outros movimentos da conta (depósito do caixa, transferência, compensação de
    // cheque) no mesmo período, que nenhuma linha conciliou.
    if let Some(desde) = primeira_linha {
        for (_, data, valor, _) in crate::conciliacao::movimentos_soltos(conexao, empresa, conta)? {
            let dia = i64::from(data.em_dias());
            if (desde..=ate).contains(&dia) {
                so_no_sistema.quantidade += 1;
                so_no_sistema.valor += valor;
            }
        }
    }
    Ok(FechamentoDoSaldo {
        banco,
        sistema: Dinheiro::centavos(sistema),
        pendentes: por_estado("Pendente")?,
        ignoradas: por_estado("Ignorado")?,
        so_no_sistema,
    })
}

/// A conta tem de ser do ativo, analítica e ativa na empresa (a mesma regra da importação).
#[cfg(feature = "sqlite")]
pub(crate) fn exigir_conta_bancaria(conn: &Connection, empresa: Id, conta: Id) -> Resultado<()> {
    let ok: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM razao_conta WHERE empresa = ?1 AND id = ?2
             AND natureza = 'Ativo' AND tipo = 'Analitica' AND ativa = 1",
            params![blob(empresa), blob(conta)],
            |r| r.get(0),
        )
        .optional()
        .map_err(persist)?;
    if ok.is_none() {
        return Err(Erro::novo(
            CodigoErro::REGRA_VIOLADA,
            "escolha uma conta bancária ativa do plano de contas",
        ));
    }
    Ok(())
}

/// Grava o saldo do banco numa data (o mais recente para a mesma data vale).
#[cfg(feature = "sqlite")]
pub(crate) fn gravar_saldo(
    conn: &Connection,
    ctx: &Ctx,
    conta: Id,
    saldo: SaldoDoBanco,
) -> Resultado<()> {
    conn.execute(
        "INSERT INTO financeiro_saldo_banco
            (empresa, conta, data, saldo, origem, informado_em, informado_por)
         VALUES (?1,?2,?3,?4,?5,?6,?7)
         ON CONFLICT (empresa, conta, data) DO UPDATE SET
            saldo = excluded.saldo, origem = excluded.origem,
            informado_em = excluded.informado_em, informado_por = excluded.informado_por",
        params![
            blob(ctx.empresa),
            blob(conta),
            i64::from(saldo.data.em_dias()),
            saldo.saldo.em_centavos(),
            match saldo.origem {
                OrigemDoSaldo::Extrato => "Extrato",
                OrigemDoSaldo::Digitado => "Digitado",
            },
            ctx.agora.em_micros(),
            blob(ctx.usuario),
        ],
    )
    .map_err(persist)?;
    Ok(())
}

/// Informa à mão o saldo do banco numa data — para quem importa CSV (que não traz saldo) ou
/// confere pelo aplicativo do banco.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct InformarSaldoDoBanco {
    /// A conta bancária.
    pub conta: Id,
    /// A data do saldo (fim do dia); não pode ser futura.
    pub data: Data,
    /// O saldo.
    pub saldo: Dinheiro,
}

impl Comando for InformarSaldoDoBanco {
    type Saida = ();
    const PERMISSAO: &'static str = "financeiro.conciliacao.importar";
    const RISCO: Risco = Risco::Baixo;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        if self.data > ctx.hoje() {
            return Err(Erro::novo(
                CodigoErro::ENTRADA_INVALIDA,
                "a data do saldo não pode ser futura",
            ));
        }
        let conn = uow.conexao();
        exigir_conta_bancaria(conn, ctx.empresa, self.conta)?;
        gravar_saldo(
            conn,
            ctx,
            self.conta,
            SaldoDoBanco {
                data: self.data,
                saldo: self.saldo,
                origem: OrigemDoSaldo::Digitado,
            },
        )
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn dia(d: u32) -> Data {
        Data::de_ymd(2026, 10, d).unwrap()
    }

    fn fechamento(banco: i64, sistema: i64) -> FechamentoDoSaldo {
        FechamentoDoSaldo {
            banco: SaldoDoBanco {
                data: dia(10),
                saldo: Dinheiro::reais(banco),
                origem: OrigemDoSaldo::Extrato,
            },
            sistema: Dinheiro::reais(sistema),
            pendentes: Soma::default(),
            ignoradas: Soma::default(),
            so_no_sistema: Soma::default(),
        }
    }

    #[test]
    fn diferenca_explicada_pelo_que_esta_so_de_um_lado() {
        // Banco 250, sistema 100: os 150 de diferença são 200 de PIX ainda pendente e −50 de
        // tarifa ignorada.
        let mut f = fechamento(250, 100);
        f.pendentes = Soma {
            quantidade: 1,
            valor: Dinheiro::reais(200),
        };
        f.ignoradas = Soma {
            quantidade: 1,
            valor: Dinheiro::reais(-50),
        };
        assert_eq!(f.diferenca(), Dinheiro::reais(150));
        assert_eq!(f.sem_explicacao(), Dinheiro::ZERO);
        assert!(!f.fechado(), "ainda há linha pendente");

        // Uma baixa de 30 recebida no sistema que o banco não mostrou: o banco tem 30 a menos.
        let mut g = fechamento(70, 100);
        g.so_no_sistema = Soma {
            quantidade: 1,
            valor: Dinheiro::reais(30),
        };
        assert_eq!(g.sem_explicacao(), Dinheiro::ZERO);

        // Tudo casado e a diferença é o saldo inicial que nunca foi lançado.
        let h = fechamento(1_000, 0);
        assert_eq!(h.sem_explicacao(), Dinheiro::reais(1_000));
        assert!(!h.fechado());
        assert!(fechamento(80, 80).fechado());
    }

    #[test]
    fn alertas_de_pendencia_antiga_extrato_velho_e_diferenca() {
        let mut c = ConferenciaDaConta {
            conta: Id::novo(),
            nome: "Banco".into(),
            codigo: "1.1.02".into(),
            pendentes: 3,
            pendente_mais_antiga: Some(dia(5)),
            ultima_linha: Some(dia(12)),
            fechamento: Some(fechamento(80, 80)),
        };
        // Em dia: pendência de 8 dias com tolerância de 10, extrato de ontem, saldo batendo.
        assert!(c.alertas(dia(13), 10).is_empty());
        // Com tolerância de 7, a pendência vira alerta.
        let a = c.alertas(dia(13), 7);
        assert_eq!(a.len(), 1);
        assert!(
            a[0].contains("3 linha(s)") && a[0].contains("8 dias"),
            "{a:?}"
        );
        // Extrato velho e diferença sem explicação.
        c.pendente_mais_antiga = None;
        c.ultima_linha = Some(dia(1));
        c.fechamento = Some(fechamento(90, 80));
        let a = c.alertas(dia(13), 7);
        assert_eq!(a.len(), 2, "{a:?}");
        assert!(a[0].contains("12 dias"));
        assert!(a[1].contains("10,00"));
    }
}
