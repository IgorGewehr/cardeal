//! O **Portal do Contador** (onda 8, seção 15 da especificação): o que liga a empresa ao
//! escritório de contabilidade, além do pacote do mês.
//!
//! - **DE/PARA do plano:** o código de cada conta analítica no sistema do contador (Domínio,
//!   Alterdata, Questor…). Quem preenche é o próprio contador, que entra com o papel de fábrica
//!   Contador (somente leitura e exportação — e isto).
//! - **Exportação dos lançamentos** ([`lancamentos_em_csv`]): o diário do mês, partida a partida,
//!   com o código do contador no lugar do do Cardeal — o leiaute "data; lançamento; conta; D/C;
//!   valor; histórico" que os sistemas contábeis importam. A conta sem DE/PARA sai com o código do
//!   Cardeal, e o pacote avisa quais são.
//! - **Apontamentos:** o contador pede ("falta a nota da compra de 12/10", "classifique o Pix de
//!   R$ 340") e a empresa responde; o contador resolve quando conferiu. A Visão geral avisa os
//!   abertos.

use std::collections::HashMap;

#[cfg(feature = "sqlite")]
use cardeal_kernel::{CodigoErro, Erro, Resultado};
use cardeal_kernel::{Competencia, Id, Instante};
use cardeal_ledger::LancamentoDoDiario;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::{Comando, Consulta, Risco};
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// O mínimo de caracteres de um apontamento e de uma resposta.
pub const TEXTO_MINIMO: usize = 5;

// ── DE/PARA ──────────────────────────────────────────────────────────────

/// Uma conta analítica e o código dela no sistema do contador.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinhaDePara {
    /// A conta.
    pub conta: Id,
    /// O código no Cardeal.
    pub codigo: String,
    /// O nome.
    pub nome: String,
    /// O código no sistema do contador.
    pub externo: Option<String>,
}

/// As contas analíticas ativas, com o DE/PARA, pelo código.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DeParaDoContador;

impl Consulta for DeParaDoContador {
    type Saida = Vec<LinhaDePara>;
    const PERMISSAO: &'static str = "financeiro.contador.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<Vec<LinhaDePara>> {
        sqlite::de_para(c, ctx.empresa)
    }
}

/// Grava o DE/PARA: o código do contador de cada conta (vazio apaga).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefinirDePara {
    /// (conta, código no sistema do contador).
    pub codigos: Vec<(Id, Option<String>)>,
}

impl Comando for DefinirDePara {
    type Saida = usize;
    const PERMISSAO: &'static str = "financeiro.contador.de_para";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<usize> {
        sqlite::definir_de_para(&self, ctx, uow)
    }
}

// ── Exportação ───────────────────────────────────────────────────────────

/// O histórico sem o que quebraria o arquivo (o separador e as quebras de linha).
fn limpar(texto: &str) -> String {
    texto
        .chars()
        .map(|c| if c == ';' || c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// O valor como os sistemas contábeis importam: sem separador de milhar, vírgula decimal.
fn valor_sem_milhar(v: cardeal_kernel::Dinheiro) -> String {
    let c = v.em_centavos().unsigned_abs();
    format!("{},{:02}", c / 100, c % 100)
}

/// Os lançamentos do diário em CSV para o sistema do contador: uma linha por partida, com o
/// código do contador (`de_para`, pelo código do Cardeal) ou, sem ele, o do Cardeal. Devolve o
/// arquivo (com cabeçalho, `;`, CRLF) e as contas que saíram sem DE/PARA (código — nome).
#[must_use]
pub fn lancamentos_em_csv<S: std::hash::BuildHasher>(
    diario: &[LancamentoDoDiario],
    de_para: &HashMap<String, String, S>,
) -> (String, Vec<String>) {
    use std::fmt::Write as _;
    let mut csv = String::from("data;lancamento;conta;dc;valor;historico\r\n");
    let mut sem: Vec<String> = Vec::new();
    for l in diario {
        let historico = limpar(&l.historico);
        for p in &l.partidas {
            let conta = if let Some(c) = de_para.get(&p.codigo) {
                c.clone()
            } else {
                let rotulo = format!("{} — {}", p.codigo, p.nome);
                if !sem.contains(&rotulo) {
                    sem.push(rotulo);
                }
                p.codigo.clone()
            };
            let dc = if p.valor.e_negativo() { "C" } else { "D" };
            let _ = write!(
                csv,
                "{};{};{conta};{dc};{};{historico}\r\n",
                l.data.formatar(),
                l.numero,
                valor_sem_milhar(p.valor)
            );
        }
    }
    (csv, sem)
}

/// O arquivo como os sistemas contábeis (Domínio, Alterdata, Questor…) importam: ISO-8859-1.
/// Os traços e as aspas tipográficas viram os simples; o resto que não cabe vira `?`.
#[must_use]
pub fn em_latin1(texto: &str) -> Vec<u8> {
    texto
        .chars()
        .map(|c| match c {
            '\u{2013}' | '\u{2014}' => b'-',
            '\u{2018}' | '\u{2019}' => b'\'',
            '\u{201C}' | '\u{201D}' => b'"',
            c => u8::try_from(u32::from(c)).unwrap_or(b'?'),
        })
        .collect()
}

// ── Apontamentos ─────────────────────────────────────────────────────────

/// O estado de um apontamento.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EstadoApontamento {
    /// O contador pediu; a empresa ainda não respondeu.
    Aberto,
    /// A empresa respondeu; o contador ainda não conferiu.
    Respondido,
    /// O contador conferiu e fechou.
    Resolvido,
}

impl EstadoApontamento {
    /// O código gravado.
    #[must_use]
    pub const fn codigo(self) -> &'static str {
        match self {
            Self::Aberto => "Aberto",
            Self::Respondido => "Respondido",
            Self::Resolvido => "Resolvido",
        }
    }

    /// O estado a partir do código.
    #[must_use]
    pub fn de_codigo(c: &str) -> Self {
        match c {
            "Respondido" => Self::Respondido,
            "Resolvido" => Self::Resolvido,
            _ => Self::Aberto,
        }
    }
}

/// Um apontamento do contador.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Apontamento {
    /// Identidade.
    pub id: Id,
    /// O mês a que se refere.
    pub competencia: Competencia,
    /// O pedido.
    pub texto: String,
    /// Quem pediu.
    pub autor: String,
    /// Quando.
    pub criado_em: Instante,
    /// O estado.
    pub estado: EstadoApontamento,
    /// A resposta da empresa.
    pub resposta: Option<String>,
    /// Quem respondeu.
    pub respondido_por: Option<String>,
}

/// Os apontamentos, os mais novos primeiro.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Apontamentos {
    /// Só os que não foram resolvidos.
    pub so_pendentes: bool,
}

impl Consulta for Apontamentos {
    type Saida = Vec<Apontamento>;
    const PERMISSAO: &'static str = "financeiro.contador.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<Vec<Apontamento>> {
        sqlite::apontamentos(c, ctx.empresa, self.so_pendentes)
    }
}

/// O contador registra um apontamento.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrarApontamento {
    /// O mês.
    pub competencia: Competencia,
    /// O pedido.
    pub texto: String,
}

impl Comando for RegistrarApontamento {
    type Saida = Id;
    const PERMISSAO: &'static str = "financeiro.contador.apontar";
    const RISCO: Risco = Risco::Baixo;
    const AUDITA: bool = false;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Id> {
        sqlite::registrar(&self, ctx, uow)
    }
}

/// A empresa responde a um apontamento.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponderApontamento {
    /// O apontamento.
    pub apontamento: Id,
    /// A resposta.
    pub resposta: String,
}

impl Comando for ResponderApontamento {
    type Saida = ();
    const PERMISSAO: &'static str = "financeiro.apontamento.responder";
    const RISCO: Risco = Risco::Baixo;
    const AUDITA: bool = false;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<()> {
        sqlite::responder(&self, ctx, uow)
    }
}

/// O contador resolve (conferiu) ou reabre um apontamento.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ResolverApontamento {
    /// O apontamento.
    pub apontamento: Id,
    /// `false` reabre (a resposta não bastou).
    pub resolvido: bool,
}

impl Comando for ResolverApontamento {
    type Saida = ();
    const PERMISSAO: &'static str = "financeiro.contador.apontar";
    const RISCO: Risco = Risco::Baixo;
    const AUDITA: bool = false;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<()> {
        sqlite::resolver(self, ctx, uow)
    }
}

#[cfg(feature = "sqlite")]
fn texto_valido(t: &str, campo: &str) -> Resultado<String> {
    let t = t.trim().to_owned();
    if t.chars().count() < TEXTO_MINIMO {
        return Err(Erro::novo(
            CodigoErro::ENTRADA_INVALIDA,
            format!("escreva ao menos {TEXTO_MINIMO} caracteres"),
        )
        .no_campo(campo));
    }
    Ok(t)
}

#[cfg(feature = "sqlite")]
mod sqlite {
    use cardeal_kernel::{CodigoErro, Competencia, Erro, Id, Instante, Resultado};
    use cardeal_modkit::Ctx;
    use cardeal_storage::UnidadeDeTrabalho;
    use rusqlite::{params, Connection, OptionalExtension};

    use super::{
        texto_valido, Apontamento, DefinirDePara, EstadoApontamento, LinhaDePara,
        RegistrarApontamento, ResolverApontamento, ResponderApontamento,
    };

    #[allow(clippy::needless_pass_by_value)]
    fn falha(e: rusqlite::Error) -> Erro {
        Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string())
    }

    fn id_de(b: &[u8]) -> Id {
        <[u8; 16]>::try_from(b).map_or(Id::NULO, Id::de_bytes)
    }

    fn b(id: Id) -> Vec<u8> {
        id.em_bytes().to_vec()
    }

    fn competencia_de(n: i64) -> Competencia {
        let n = u32::try_from(n).unwrap_or(0);
        Competencia::nova(i32::try_from(n / 100).unwrap_or(0), n % 100)
    }

    pub(super) fn de_para(c: &Connection, empresa: Id) -> Resultado<Vec<LinhaDePara>> {
        let mut st = c
            .prepare(
                "SELECT k.id, k.codigo, k.nome, d.externo
                 FROM razao_conta k
                 LEFT JOIN financeiro_de_para d ON d.empresa = k.empresa AND d.conta = k.id
                 WHERE k.empresa = ?1 AND k.tipo <> 'Sintetica' AND k.ativa = 1
                 ORDER BY k.codigo",
            )
            .map_err(falha)?;
        let v = st
            .query_map([b(empresa)], |r| {
                Ok(LinhaDePara {
                    conta: id_de(&r.get::<_, Vec<u8>>(0)?),
                    codigo: r.get(1)?,
                    nome: r.get(2)?,
                    externo: r.get(3)?,
                })
            })
            .map_err(falha)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(falha);
        v
    }

    pub(super) fn definir_de_para(
        cmd: &DefinirDePara,
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
    ) -> Resultado<usize> {
        let c = uow.conexao();
        let mut n = 0;
        for (conta, externo) in &cmd.codigos {
            let existe: bool = c
                .query_row(
                    "SELECT EXISTS (SELECT 1 FROM razao_conta WHERE empresa = ?1 AND id = ?2)",
                    params![b(ctx.empresa), b(*conta)],
                    |r| r.get(0),
                )
                .map_err(falha)?;
            if !existe {
                return Err(Erro::novo(CodigoErro::NAO_ENCONTRADO, "conta do plano"));
            }
            match externo.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                Some(cod) => {
                    if cod.contains(';') || cod.chars().count() > 40 {
                        return Err(Erro::novo(
                            CodigoErro::ENTRADA_INVALIDA,
                            format!("código do contador inválido: {cod}"),
                        ));
                    }
                    c.execute(
                        "INSERT INTO financeiro_de_para (empresa, conta, externo) VALUES (?1, ?2, ?3)
                         ON CONFLICT (empresa, conta) DO UPDATE SET externo = excluded.externo",
                        params![b(ctx.empresa), b(*conta), cod],
                    )
                    .map_err(falha)?;
                }
                None => {
                    c.execute(
                        "DELETE FROM financeiro_de_para WHERE empresa = ?1 AND conta = ?2",
                        params![b(ctx.empresa), b(*conta)],
                    )
                    .map_err(falha)?;
                }
            }
            n += 1;
        }
        Ok(n)
    }

    pub(super) fn apontamentos(
        c: &Connection,
        empresa: Id,
        so_pendentes: bool,
    ) -> Resultado<Vec<Apontamento>> {
        let mut st = c
            .prepare(
                "SELECT a.id, a.competencia, a.texto, COALESCE(u.nome, ''), a.criado_em, a.estado,
                        a.resposta, r.nome
                 FROM financeiro_apontamento a
                 LEFT JOIN nucleo_usuario u ON u.id = a.autor
                 LEFT JOIN nucleo_usuario r ON r.id = a.respondido_por
                 WHERE a.empresa = ?1 AND (?2 = 0 OR a.estado <> 'Resolvido')
                 ORDER BY a.criado_em DESC
                 LIMIT 2000",
            )
            .map_err(falha)?;
        let v = st
            .query_map(params![b(empresa), i64::from(so_pendentes)], |r| {
                Ok(Apontamento {
                    id: id_de(&r.get::<_, Vec<u8>>(0)?),
                    competencia: competencia_de(r.get(1)?),
                    texto: r.get(2)?,
                    autor: r.get(3)?,
                    criado_em: Instante::de_micros(r.get(4)?),
                    estado: EstadoApontamento::de_codigo(&r.get::<_, String>(5)?),
                    resposta: r.get(6)?,
                    respondido_por: r.get(7)?,
                })
            })
            .map_err(falha)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(falha);
        v
    }

    pub(super) fn registrar(
        cmd: &RegistrarApontamento,
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
    ) -> Resultado<Id> {
        let texto = texto_valido(&cmd.texto, "texto")?;
        let id = Id::novo();
        uow.conexao()
            .execute(
                "INSERT INTO financeiro_apontamento
                   (id, empresa, competencia, texto, autor, criado_em, estado)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'Aberto')",
                params![
                    b(id),
                    b(ctx.empresa),
                    i64::from(cmd.competencia.em_aaaamm()),
                    texto,
                    b(ctx.usuario),
                    ctx.agora.em_micros()
                ],
            )
            .map_err(falha)?;
        Ok(id)
    }

    fn estado_de(c: &Connection, empresa: Id, id: Id) -> Resultado<EstadoApontamento> {
        c.query_row(
            "SELECT estado FROM financeiro_apontamento WHERE empresa = ?1 AND id = ?2",
            params![b(empresa), b(id)],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .map_err(falha)?
        .map(|s| EstadoApontamento::de_codigo(&s))
        .ok_or_else(|| Erro::nao_encontrado("apontamento"))
    }

    pub(super) fn responder(
        cmd: &ResponderApontamento,
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
    ) -> Resultado<()> {
        let resposta = texto_valido(&cmd.resposta, "resposta")?;
        let c = uow.conexao();
        if estado_de(c, ctx.empresa, cmd.apontamento)? == EstadoApontamento::Resolvido {
            return Err(Erro::novo(
                CodigoErro::ESTADO_INVALIDO,
                "este apontamento já foi resolvido pelo contador",
            ));
        }
        c.execute(
            "UPDATE financeiro_apontamento
             SET estado = 'Respondido', resposta = ?3, respondido_por = ?4, respondido_em = ?5
             WHERE empresa = ?1 AND id = ?2",
            params![
                b(ctx.empresa),
                b(cmd.apontamento),
                resposta,
                b(ctx.usuario),
                ctx.agora.em_micros()
            ],
        )
        .map_err(falha)?;
        Ok(())
    }

    pub(super) fn resolver(
        cmd: ResolverApontamento,
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
    ) -> Resultado<()> {
        let c = uow.conexao();
        let _ = estado_de(c, ctx.empresa, cmd.apontamento)?;
        let (estado, quando) = if cmd.resolvido {
            ("Resolvido", Some(ctx.agora.em_micros()))
        } else {
            ("Aberto", None)
        };
        c.execute(
            "UPDATE financeiro_apontamento SET estado = ?3, resolvido_em = ?4
             WHERE empresa = ?1 AND id = ?2",
            params![b(ctx.empresa), b(cmd.apontamento), estado, quando],
        )
        .map_err(falha)?;
        Ok(())
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use cardeal_kernel::{Data, Dinheiro};
    use cardeal_ledger::PartidaDoDiario;

    #[test]
    fn o_csv_usa_o_codigo_do_contador_e_lista_as_contas_sem_de_para() {
        let diario = vec![LancamentoDoDiario {
            numero: 7,
            data: Data::de_ymd(2026, 10, 5).unwrap(),
            historico: "Venda; cupom 12\nPIX".into(),
            estado: "Confirmado".into(),
            partidas: vec![
                PartidaDoDiario {
                    codigo: "1.1.01".into(),
                    nome: "Caixa".into(),
                    valor: Dinheiro::centavos(12_345),
                },
                PartidaDoDiario {
                    codigo: "3.1.01".into(),
                    nome: "Receita de vendas".into(),
                    valor: Dinheiro::centavos(-12_345),
                },
            ],
        }];
        let de_para: HashMap<String, String> = [("1.1.01".to_owned(), "5".to_owned())].into();
        let (csv, sem) = lancamentos_em_csv(&diario, &de_para);
        let linhas: Vec<&str> = csv.split("\r\n").collect();
        assert_eq!(linhas[0], "data;lancamento;conta;dc;valor;historico");
        assert_eq!(linhas[1], "05/10/2026;7;5;D;123,45;Venda cupom 12 PIX");
        assert_eq!(linhas[2], "05/10/2026;7;3.1.01;C;123,45;Venda cupom 12 PIX");
        assert_eq!(sem, vec!["3.1.01 — Receita de vendas".to_owned()]);
    }

    #[test]
    fn o_arquivo_sai_em_latin1() {
        assert_eq!(em_latin1("Ação — “x” ’"), b"A\xe7\xe3o - \"x\" '".to_vec());
        assert_eq!(em_latin1("€"), b"?".to_vec());
    }
}
