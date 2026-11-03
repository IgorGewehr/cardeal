//! As notas de compra que chegam pela SEFAZ (distribuição `DFe`): a tarefa que busca, as notas
//! que esperam a Ciência da Operação e a situação da busca, para a tela.
//!
//! Lições do `gestao-raiz` (`purchase-notes.service.ts`, `sefaz-distribuicao.ts`), em produção:
//! - **o NSU nunca volta**: resposta ruim não grava NSU, e a base guarda sempre o maior — zerar
//!   o NSU é baixar tudo de novo e tomar 656 ("consumo indevido");
//! - em dia (o último NSU alcançou o maior) ou com 656, só se pede de novo **depois de uma
//!   hora**; e uma rodada por vez por empresa — a vez é tomada **na base**, não na memória, para
//!   dois terminais da mesma empresa não consultarem juntos;
//! - documento que não se importa **não trava a fila**: fica guardado, com o XML e o motivo, e
//!   o NSU anda;
//! - a nota que chega só como resumo recebe a **Ciência da Operação** sozinha (como lá); a
//!   completa vem pela consulta por chave ou pelo próximo NSU.
//!
//! Aqui, além disso:
//! - o NSU é **por ambiente**: passar a empresa de homologação para produção não pula nota;
//! - nota de homologação não entra no estoque (é teste, sem valor fiscal): fica registrada;
//! - o cancelamento de uma nota pelo fornecedor chega como evento: a que esperava some da fila,
//!   e a que já foi lançada vira aviso.

use cardeal_kernel::{Data, Dinheiro, Instante};
use cardeal_modkit::{Comando, Consulta};
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;

/// A tarefa que busca as notas na SEFAZ.
pub const TAREFA_BUSCAR_NA_SEFAZ: &str = "compras.buscar_notas_na_sefaz";

/// Em que pé está uma nota que a SEFAZ entregou.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EstadoNaSefaz {
    /// Chegou só o resumo: a Ciência da Operação libera a nota completa.
    Aguardando,
    /// A nota completa chegou e virou nota de entrada.
    Baixada,
    /// Não é compra da empresa (ou foi cancelada pelo fornecedor, ou é de homologação).
    Ignorada,
    /// A nota chegou, mas não deu para importar — o XML está guardado.
    Erro,
}

impl EstadoNaSefaz {
    /// O texto na base.
    #[must_use]
    pub const fn como_str(self) -> &'static str {
        match self {
            Self::Aguardando => "Aguardando",
            Self::Baixada => "Baixada",
            Self::Ignorada => "Ignorada",
            Self::Erro => "Erro",
        }
    }

    /// O contrário de [`Self::como_str`].
    #[must_use]
    pub fn de_str(s: &str) -> Option<Self> {
        Some(match s {
            "Aguardando" => Self::Aguardando,
            "Baixada" => Self::Baixada,
            "Ignorada" => Self::Ignorada,
            "Erro" => Self::Erro,
            _ => return None,
        })
    }

    /// Para a tela.
    #[must_use]
    pub const fn rotulo(self) -> &'static str {
        match self {
            Self::Aguardando => "Aguardando a SEFAZ",
            Self::Baixada => "Lançada",
            Self::Ignorada => "Ignorada",
            Self::Erro => "Não importou",
        }
    }
}

/// Uma nota que a SEFAZ entregou.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotaNaSefaz {
    /// A chave de acesso.
    pub chave: String,
    /// O emitente (o fornecedor).
    pub emitente: String,
    /// CNPJ (ou CPF) do emitente.
    pub emitente_documento: String,
    /// O dia da emissão.
    pub emissao: Option<Data>,
    /// O valor da nota.
    pub valor: Dinheiro,
    /// O estado.
    pub estado: EstadoNaSefaz,
    /// Por que está assim (ignorada, erro, à espera).
    pub motivo: Option<String>,
    /// Quando a empresa deu ciência.
    pub ciencia_em: Option<Instante>,
    /// Quando chegou.
    pub recebida_em: Instante,
}

/// A situação da busca, para a tela de Compras.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SituacaoDaBusca {
    /// O último NSU lido, no ambiente atual.
    pub ultimo_nsu: u64,
    /// Quando foi a última consulta.
    pub ultima_consulta: Option<Instante>,
    /// A partir de quando a SEFAZ aceita a próxima (`None`: já).
    pub proxima_consulta: Option<Instante>,
    /// O resumo da última rodada.
    pub ultimo_resultado: Option<String>,
    /// As notas que ainda pedem atenção (aguardando, com erro) e as últimas ignoradas.
    pub notas: Vec<NotaNaSefaz>,
}

/// A situação da busca de notas na SEFAZ.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct NotasNaSefaz;

impl Consulta for NotasNaSefaz {
    type Saida = SituacaoDaBusca;
    const PERMISSAO: &'static str = "compras.entrada.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<SituacaoDaBusca> {
        let ambiente = mod_fiscal::ambiente_da_empresa(c, ctx.empresa)?;
        sqlite::situacao(c, ctx.empresa, ambiente, ctx.agora)
    }
}

/// Se a hora da próxima busca chegou — a vigia do desktop pergunta a cada minuto.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct HoraDeBuscarNaSefaz;

impl Consulta for HoraDeBuscarNaSefaz {
    type Saida = bool;
    const PERMISSAO: &'static str = "compras.entrada.importar";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<bool> {
        let ambiente = mod_fiscal::ambiente_da_empresa(c, ctx.empresa)?;
        sqlite::hora_de_buscar(c, ctx.empresa, ambiente, ctx.agora)
    }
}

/// Importa de novo uma nota que chegou da SEFAZ e não importou (o XML ficou guardado) —
/// depois de cadastrar o que faltava, por exemplo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportarDeNovoDaSefaz {
    /// A chave da nota.
    pub chave: String,
}

impl Comando for ImportarDeNovoDaSefaz {
    type Saida = crate::RelatorioImportacao;
    const PERMISSAO: &'static str = "compras.entrada.importar";

    #[cfg(feature = "sqlite")]
    fn executar(
        self,
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
    ) -> Resultado<crate::RelatorioImportacao> {
        sqlite::importar_de_novo(ctx, uow, &self.chave)
    }
}

#[cfg(feature = "sqlite")]
pub(crate) mod sqlite {
    use std::fmt::Write as _;

    use cardeal_fiscal::{
        interpretar, ler_evento, ler_resumo_nfe, AmbienteFiscal, ChaveAcesso, DocumentoDfe,
        Manifestacao, Nsu, PortaFiscal, SituacaoDoResumo, SituacaoRetorno, TipoDfe,
    };
    use cardeal_kernel::{Cnpj, CodigoErro, Data, Dinheiro, Erro, Id, Instante, Resultado, Uf};
    use cardeal_modkit::{ContextoTarefa, Ctx};
    use cardeal_storage::UnidadeDeTrabalho;
    use rusqlite::{params, Connection, OptionalExtension};

    use super::{EstadoNaSefaz, NotaNaSefaz, SituacaoDaBusca};
    use crate::comandos::importar_nota_da_sefaz;
    use crate::RelatorioImportacao;

    /// Páginas (de até 50 documentos) por rodada.
    const PAGINAS_POR_RODADA: usize = 10;
    /// Ciências (e consultas por chave) por rodada.
    const CIENCIAS_POR_RODADA: usize = 20;
    /// Em dia ou com 656: a SEFAZ só aceita de novo depois disso.
    const UMA_HORA: i64 = 60 * 60;
    /// A vez de uma rodada em andamento (se ela cair no meio, a vez vence sozinha).
    const VEZ_SEGUNDOS: i64 = 10 * 60;
    /// A API não respondeu: tenta de novo depois disso.
    const SEM_RESPOSTA_SEGUNDOS: i64 = 5 * 60;
    /// Quantas vezes a ciência de uma nota é tentada antes de ficar só à espera do usuário.
    const TENTATIVAS_DA_CIENCIA: i64 = 5;

    #[allow(clippy::needless_pass_by_value)]
    fn falha(e: rusqlite::Error) -> Erro {
        Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string())
    }

    /// A situação do resumo (`cSitNFe`) em texto, para o motivo.
    const fn motivo_da_situacao(s: SituacaoDoResumo) -> Option<&'static str> {
        match s {
            SituacaoDoResumo::Autorizada => None,
            SituacaoDoResumo::Denegada => Some("nota denegada pela SEFAZ"),
            SituacaoDoResumo::Cancelada => Some("cancelada pelo fornecedor"),
        }
    }

    /// O CNPJ cadastrado da empresa.
    fn cnpj_da_empresa(c: &Connection, empresa: Id) -> Resultado<Cnpj> {
        let texto: String = c
            .query_row(
                "SELECT cnpj FROM nucleo_empresa WHERE id = ?1",
                [empresa.em_bytes().as_slice()],
                |r| r.get(0),
            )
            .map_err(falha)?;
        Cnpj::novo(&texto).map_err(|_| {
            Erro::novo(
                CodigoErro::ESTADO_INVALIDO,
                "o CNPJ cadastrado da empresa é inválido",
            )
        })
    }

    fn ambiente_str(a: AmbienteFiscal) -> &'static str {
        match a {
            AmbienteFiscal::Producao => "producao",
            AmbienteFiscal::Homologacao => "homologacao",
        }
    }

    fn micros(segundos: i64) -> i64 {
        segundos * 1_000_000
    }

    fn i64_de(n: u64) -> i64 {
        i64::try_from(n).unwrap_or(i64::MAX)
    }

    // ------------------------------------------------------------------ estado da busca

    /// Toma a vez da rodada: só se a hora chegou e ninguém está rodando. Devolve o NSU de onde
    /// partir.
    fn tomar_a_vez(
        c: &Connection,
        empresa: Id,
        ambiente: AmbienteFiscal,
        agora: Instante,
    ) -> Resultado<Option<u64>> {
        let (e, a) = (empresa.em_bytes(), ambiente_str(ambiente));
        c.execute(
            "INSERT INTO compras_dfe_estado (empresa, ambiente) VALUES (?1, ?2)
             ON CONFLICT (empresa, ambiente) DO NOTHING",
            params![e.as_slice(), a],
        )
        .map_err(falha)?;
        let tomou = c
            .execute(
                "UPDATE compras_dfe_estado SET proxima_consulta = ?3
                 WHERE empresa = ?1 AND ambiente = ?2 AND proxima_consulta <= ?4",
                params![
                    e.as_slice(),
                    a,
                    agora.em_micros() + micros(VEZ_SEGUNDOS),
                    agora.em_micros()
                ],
            )
            .map_err(falha)?;
        if tomou == 0 {
            return Ok(None);
        }
        // A nota que esperava e entrou por outro caminho (o arquivo XML) sai da espera.
        c.execute(
            "UPDATE compras_dfe_nota SET estado = 'Baixada', motivo = NULL
             WHERE empresa = ?1 AND estado IN ('Aguardando','Erro')
               AND chave IN (SELECT chave_acesso FROM compras_nota_entrada
                             WHERE empresa = ?1 AND chave_acesso IS NOT NULL)",
            params![e.as_slice()],
        )
        .map_err(falha)?;
        c.query_row(
            "SELECT ultimo_nsu FROM compras_dfe_estado WHERE empresa = ?1 AND ambiente = ?2",
            params![e.as_slice(), a],
            |r| r.get::<_, i64>(0),
        )
        .map(|n| Some(u64::try_from(n).unwrap_or(0)))
        .map_err(falha)
    }

    /// Avança o NSU — nunca para trás.
    fn avancar_nsu(
        c: &Connection,
        empresa: Id,
        ambiente: AmbienteFiscal,
        nsu: u64,
    ) -> Resultado<()> {
        c.execute(
            "UPDATE compras_dfe_estado SET ultimo_nsu = MAX(ultimo_nsu, ?3)
             WHERE empresa = ?1 AND ambiente = ?2",
            params![
                empresa.em_bytes().as_slice(),
                ambiente_str(ambiente),
                i64_de(nsu)
            ],
        )
        .map(|_| ())
        .map_err(falha)
    }

    /// Fecha a rodada: quando foi, quando pode a próxima e o que aconteceu.
    fn fechar_rodada(
        c: &Connection,
        empresa: Id,
        ambiente: AmbienteFiscal,
        agora: Instante,
        proxima: Instante,
        resultado: &str,
    ) -> Resultado<()> {
        c.execute(
            "UPDATE compras_dfe_estado
             SET ultima_consulta = ?3, proxima_consulta = ?4, ultimo_resultado = ?5
             WHERE empresa = ?1 AND ambiente = ?2",
            params![
                empresa.em_bytes().as_slice(),
                ambiente_str(ambiente),
                agora.em_micros(),
                proxima.em_micros(),
                resultado
            ],
        )
        .map(|_| ())
        .map_err(falha)
    }

    pub(crate) fn hora_de_buscar(
        c: &Connection,
        empresa: Id,
        ambiente: AmbienteFiscal,
        agora: Instante,
    ) -> Resultado<bool> {
        let proxima: Option<i64> = c
            .query_row(
                "SELECT proxima_consulta FROM compras_dfe_estado
                 WHERE empresa = ?1 AND ambiente = ?2",
                params![empresa.em_bytes().as_slice(), ambiente_str(ambiente)],
                |r| r.get(0),
            )
            .optional()
            .map_err(falha)?;
        Ok(proxima.is_none_or(|p| p <= agora.em_micros()))
    }

    pub(crate) fn situacao(
        c: &Connection,
        empresa: Id,
        ambiente: AmbienteFiscal,
        agora: Instante,
    ) -> Resultado<SituacaoDaBusca> {
        let e = empresa.em_bytes();
        let estado = c
            .query_row(
                "SELECT ultimo_nsu, ultima_consulta, proxima_consulta, ultimo_resultado
                 FROM compras_dfe_estado WHERE empresa = ?1 AND ambiente = ?2",
                params![e.as_slice(), ambiente_str(ambiente)],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, Option<i64>>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, Option<String>>(3)?,
                    ))
                },
            )
            .optional()
            .map_err(falha)?;
        let mut stmt = c
            .prepare(
                "SELECT chave, emitente_nome, emitente_documento, emissao, valor, estado, motivo,
                        ciencia_em, recebida_em
                 FROM compras_dfe_nota
                 WHERE empresa = ?1 AND ambiente = ?2 AND estado <> 'Baixada'
                   AND NOT EXISTS (SELECT 1 FROM compras_nota_entrada e
                                   WHERE e.empresa = compras_dfe_nota.empresa
                                     AND e.chave_acesso = compras_dfe_nota.chave)
                 ORDER BY CASE estado WHEN 'Erro' THEN 0 WHEN 'Aguardando' THEN 1 ELSE 2 END,
                          recebida_em DESC
                 LIMIT 200",
            )
            .map_err(falha)?;
        let notas = stmt
            .query_map(params![e.as_slice(), ambiente_str(ambiente)], |r| {
                Ok(NotaNaSefaz {
                    chave: r.get(0)?,
                    emitente: r.get(1)?,
                    emitente_documento: r.get(2)?,
                    emissao: r.get::<_, Option<i32>>(3)?.map(Data::de_dias),
                    valor: Dinheiro::centavos(r.get(4)?),
                    estado: EstadoNaSefaz::de_str(&r.get::<_, String>(5)?)
                        .unwrap_or(EstadoNaSefaz::Aguardando),
                    motivo: r.get(6)?,
                    ciencia_em: r.get::<_, Option<i64>>(7)?.map(Instante::de_micros),
                    recebida_em: Instante::de_micros(r.get(8)?),
                })
            })
            .map_err(falha)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(falha)?;
        Ok(match estado {
            Some((nsu, ultima, proxima, resultado)) => SituacaoDaBusca {
                ultimo_nsu: u64::try_from(nsu).unwrap_or(0),
                ultima_consulta: ultima.map(Instante::de_micros),
                proxima_consulta: (proxima > agora.em_micros())
                    .then(|| Instante::de_micros(proxima)),
                ultimo_resultado: resultado,
                notas,
            },
            None => SituacaoDaBusca {
                notas,
                ..SituacaoDaBusca::default()
            },
        })
    }

    // ------------------------------------------------------------------ as notas

    /// O que se sabe de uma nota para registrá-la.
    struct Registro<'a> {
        chave: &'a str,
        nsu: u64,
        emitente_nome: &'a str,
        emitente_documento: &'a str,
        emissao: Option<Data>,
        valor: Dinheiro,
        situacao: SituacaoDoResumo,
        estado: EstadoNaSefaz,
        motivo: Option<&'a str>,
        xml: Option<&'a str>,
    }

    /// Registra a nota (ou atualiza o estado, o motivo e o XML de uma já registrada — a
    /// `Baixada` não volta atrás).
    fn registrar(
        c: &Connection,
        empresa: Id,
        ambiente: AmbienteFiscal,
        r: &Registro<'_>,
        agora: Instante,
    ) -> Resultado<()> {
        c.execute(
            "INSERT INTO compras_dfe_nota
               (empresa, chave, ambiente, nsu, emitente_documento, emitente_nome, emissao, valor,
                situacao, estado, motivo, xml, recebida_em)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)
             ON CONFLICT (empresa, chave) DO UPDATE SET
               estado = CASE WHEN estado = 'Baixada' THEN estado ELSE excluded.estado END,
               motivo = CASE WHEN estado = 'Baixada' THEN motivo ELSE excluded.motivo END,
               xml = COALESCE(excluded.xml, xml),
               emitente_nome = CASE WHEN excluded.emitente_nome = '' THEN emitente_nome
                                    ELSE excluded.emitente_nome END",
            params![
                empresa.em_bytes().as_slice(),
                r.chave,
                ambiente_str(ambiente),
                i64_de(r.nsu),
                r.emitente_documento,
                r.emitente_nome,
                r.emissao.map(Data::em_dias),
                r.valor.em_centavos(),
                r.situacao.codigo(),
                r.estado.como_str(),
                r.motivo,
                r.xml,
                agora.em_micros()
            ],
        )
        .map(|_| ())
        .map_err(falha)
    }

    fn marcar(
        c: &Connection,
        empresa: Id,
        chave: &str,
        estado: EstadoNaSefaz,
        motivo: Option<&str>,
    ) -> Resultado<usize> {
        c.execute(
            "UPDATE compras_dfe_nota SET estado = ?3, motivo = ?4
             WHERE empresa = ?1 AND chave = ?2 AND estado <> 'Baixada'",
            params![
                empresa.em_bytes().as_slice(),
                chave,
                estado.como_str(),
                motivo
            ],
        )
        .map_err(falha)
    }

    fn estado_da_nota(
        c: &Connection,
        empresa: Id,
        chave: &str,
    ) -> Resultado<Option<EstadoNaSefaz>> {
        c.query_row(
            "SELECT estado FROM compras_dfe_nota WHERE empresa = ?1 AND chave = ?2",
            params![empresa.em_bytes().as_slice(), chave],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .map(|s| s.as_deref().and_then(EstadoNaSefaz::de_str))
        .map_err(falha)
    }

    fn ja_lancada(c: &Connection, empresa: Id, chave: &str) -> Resultado<bool> {
        c.query_row(
            "SELECT 1 FROM compras_nota_entrada WHERE empresa = ?1 AND chave_acesso = ?2",
            params![empresa.em_bytes().as_slice(), chave],
            |_| Ok(()),
        )
        .optional()
        .map(|o| o.is_some())
        .map_err(falha)
    }

    /// O que a rodada fez.
    #[derive(Default)]
    struct Rodada {
        importadas: u32,
        aguardando: u32,
        ignoradas: u32,
        erros: u32,
        avisos: Vec<String>,
    }

    impl Rodada {
        /// O resumo; `esperando` é quantas notas, ao fim da rodada, ainda esperam a SEFAZ.
        fn resumo(&self, esperando: u32) -> String {
            let mut t = format!(
                "{} nota(s) lançada(s), {} ignorada(s)",
                self.importadas, self.ignoradas
            );
            if self.erros > 0 {
                let _ = write!(t, ", {} não importou(aram)", self.erros);
            }
            let _ = write!(t, ", {esperando} aguardando a SEFAZ");
            for a in &self.avisos {
                let _ = write!(t, "; {a}");
            }
            t
        }
    }

    /// O que vale para a rodada inteira.
    struct Contexto<'a> {
        t: &'a ContextoTarefa<'a>,
        porta: &'a dyn PortaFiscal,
        cnpj: Cnpj,
        uf: Uf,
        ambiente: AmbienteFiscal,
        agora: Instante,
    }

    impl Contexto<'_> {
        fn empresa(&self) -> Id {
            self.t.ctx.empresa
        }

        fn escrever<T: Send + 'static>(
            &self,
            f: impl for<'u> FnOnce(&mut UnidadeDeTrabalho<'u>, &Ctx) -> Resultado<T> + Send + 'static,
        ) -> Resultado<T> {
            let ctx = self.t.ctx.clone();
            self.t.escrever(move |uow| f(uow, &ctx))
        }
    }

    /// Uma nota completa: importa se é compra (em produção), senão registra por quê.
    fn processar_nota(x: &Contexto<'_>, doc: &DocumentoDfe, r: &mut Rodada) -> Resultado<()> {
        let (empresa, ambiente, agora) = (x.empresa(), x.ambiente, x.agora);
        let nota = match interpretar(&doc.xml) {
            Ok(n) => n,
            Err(e) => {
                let Some(chave) = doc.chave.as_ref().map(|c| c.como_str().to_owned()) else {
                    r.erros += 1;
                    return Ok(());
                };
                let (motivo, xml, nsu) = (format!("XML ilegível: {e}"), doc.xml.clone(), doc.nsu.0);
                x.escrever(move |uow, _| {
                    registrar(
                        uow.conexao(),
                        empresa,
                        ambiente,
                        &Registro {
                            chave: &chave,
                            nsu,
                            emitente_nome: "",
                            emitente_documento: "",
                            emissao: None,
                            valor: Dinheiro::ZERO,
                            situacao: SituacaoDoResumo::Autorizada,
                            estado: EstadoNaSefaz::Erro,
                            motivo: Some(&motivo),
                            xml: Some(&xml),
                        },
                        agora,
                    )
                })?;
                r.erros += 1;
                return Ok(());
            }
        };
        let chave = nota.chave_acesso.como_str().to_owned();
        let recusa = nota.classificar_para(&x.cnpj).motivo_da_recusa();
        let homologacao = ambiente == AmbienteFiscal::Homologacao;
        let registro = |estado, motivo: Option<String>, xml: Option<String>| {
            let (chave, nome, doc_emit) = (
                chave.clone(),
                nota.fornecedor_nome.clone(),
                nota.fornecedor_cnpj.sem_mascara(),
            );
            let (emissao, valor, nsu) = (nota.emissao, nota.valor_total, doc.nsu.0);
            move |uow: &mut UnidadeDeTrabalho<'_>, _: &Ctx| {
                registrar(
                    uow.conexao(),
                    empresa,
                    ambiente,
                    &Registro {
                        chave: &chave,
                        nsu,
                        emitente_nome: &nome,
                        emitente_documento: &doc_emit,
                        emissao: Some(emissao),
                        valor,
                        situacao: SituacaoDoResumo::Autorizada,
                        estado,
                        motivo: motivo.as_deref(),
                        xml: xml.as_deref(),
                    },
                    agora,
                )
            }
        };
        if let Some(m) = recusa {
            x.escrever(registro(EstadoNaSefaz::Ignorada, Some(m.to_owned()), None))?;
            r.ignoradas += 1;
            return Ok(());
        }
        if homologacao {
            let m = "nota de homologação (teste, sem valor fiscal): não entra no estoque";
            x.escrever(registro(EstadoNaSefaz::Ignorada, Some(m.to_owned()), None))?;
            r.ignoradas += 1;
            return Ok(());
        }
        // A importação e o "lançada" na mesma transação; se ela falhar, a nota fica guardada
        // com o motivo, e a fila segue.
        let lancar = registro(EstadoNaSefaz::Baixada, None, None);
        let (nota_i, xml_i) = (nota.clone(), doc.xml.clone());
        let feito = x.escrever(move |uow, ctx| {
            // A nota que já entrou (pelo arquivo, ou numa rodada anterior) não entra de novo.
            let entra_agora =
                !ja_lancada(uow.conexao(), ctx.empresa, nota_i.chave_acesso.como_str())?;
            if entra_agora {
                importar_nota_da_sefaz(&nota_i, &xml_i, ctx, uow)?;
            }
            lancar(uow, ctx)?;
            Ok(entra_agora)
        });
        match feito {
            Ok(entrou) => r.importadas += u32::from(entrou),
            Err(e) if e.codigo == CodigoErro::FALHA_INTERNA => return Err(e),
            Err(e) => {
                x.escrever(registro(
                    EstadoNaSefaz::Erro,
                    Some(e.mensagem),
                    Some(doc.xml.clone()),
                ))?;
                r.erros += 1;
            }
        }
        Ok(())
    }

    /// Um resumo: registra à espera da ciência (ou por que não espera).
    fn processar_resumo(x: &Contexto<'_>, doc: &DocumentoDfe, r: &mut Rodada) -> Resultado<()> {
        let Ok(resumo) = ler_resumo_nfe(&doc.xml) else {
            r.erros += 1;
            return Ok(());
        };
        let (empresa, ambiente, agora) = (x.empresa(), x.ambiente, x.agora);
        let chave = resumo.chave.como_str().to_owned();
        let propria = resumo.emitente_documento == x.cnpj.sem_mascara();
        let motivo = if propria {
            Some("emitida pela sua própria empresa")
        } else {
            motivo_da_situacao(resumo.situacao)
        };
        let estado = if motivo.is_some() {
            EstadoNaSefaz::Ignorada
        } else {
            EstadoNaSefaz::Aguardando
        };
        let nsu = doc.nsu.0;
        let novo = x.escrever(move |uow, _| {
            let c = uow.conexao();
            if ja_lancada(c, empresa, &chave)? {
                return Ok(false);
            }
            if estado == EstadoNaSefaz::Aguardando && estado_da_nota(c, empresa, &chave)?.is_some()
            {
                return Ok(false);
            }
            registrar(
                c,
                empresa,
                ambiente,
                &Registro {
                    chave: &chave,
                    nsu,
                    emitente_nome: &resumo.emitente_nome,
                    emitente_documento: &resumo.emitente_documento,
                    emissao: resumo.emissao,
                    valor: resumo.valor,
                    situacao: resumo.situacao,
                    estado,
                    motivo,
                    xml: None,
                },
                agora,
            )?;
            Ok(true)
        })?;
        if novo {
            match estado {
                EstadoNaSefaz::Aguardando => r.aguardando += 1,
                _ => r.ignoradas += 1,
            }
        }
        Ok(())
    }

    /// Um evento: o cancelamento pelo fornecedor tira a nota da espera, ou vira aviso se ela
    /// já foi lançada.
    fn processar_evento(x: &Contexto<'_>, doc: &DocumentoDfe, r: &mut Rodada) -> Resultado<()> {
        let Ok(evento) = ler_evento(&doc.xml) else {
            return Ok(());
        };
        if !evento.e_cancelamento() {
            return Ok(());
        }
        let empresa = x.empresa();
        let chave = evento.chave.como_str().to_owned();
        let lancada = x.escrever(move |uow, _| {
            let c = uow.conexao();
            marcar(
                c,
                empresa,
                &chave,
                EstadoNaSefaz::Ignorada,
                Some("cancelada pelo fornecedor"),
            )?;
            ja_lancada(c, empresa, &chave)
        })?;
        if lancada {
            r.avisos.push(format!(
                "o fornecedor cancelou a nota {} que já foi lançada — confira a entrada",
                evento.chave.como_str()
            ));
        }
        Ok(())
    }

    fn processar(x: &Contexto<'_>, doc: &DocumentoDfe, r: &mut Rodada) -> Resultado<()> {
        match doc.tipo {
            TipoDfe::Nota => processar_nota(x, doc, r),
            TipoDfe::ResumoDaNota => processar_resumo(x, doc, r),
            TipoDfe::ResumoDeEvento | TipoDfe::Evento => processar_evento(x, doc, r),
            TipoDfe::Outro => Ok(()),
        }
    }

    // ------------------------------------------------------------------ a ciência

    struct AEsperar {
        chave: String,
        ciencia: bool,
        tentativas: i64,
    }

    fn esperando(c: &Connection, empresa: Id, ambiente: AmbienteFiscal) -> Resultado<u32> {
        c.query_row(
            "SELECT COUNT(*) FROM compras_dfe_nota
             WHERE empresa = ?1 AND ambiente = ?2 AND estado = 'Aguardando'",
            params![empresa.em_bytes().as_slice(), ambiente_str(ambiente)],
            |r| r.get(0),
        )
        .map_err(falha)
    }

    fn a_esperar(
        c: &Connection,
        empresa: Id,
        ambiente: AmbienteFiscal,
        agora: Instante,
    ) -> Resultado<Vec<AEsperar>> {
        let mut stmt = c
            .prepare(
                "SELECT chave, ciencia_em IS NOT NULL, tentativas FROM compras_dfe_nota
                 WHERE empresa = ?1 AND ambiente = ?2 AND estado = 'Aguardando'
                   AND proxima_tentativa <= ?3
                 ORDER BY recebida_em LIMIT ?4",
            )
            .map_err(falha)?;
        let v = stmt
            .query_map(
                params![
                    empresa.em_bytes().as_slice(),
                    ambiente_str(ambiente),
                    agora.em_micros(),
                    i64::try_from(CIENCIAS_POR_RODADA).unwrap_or(20)
                ],
                |r| {
                    Ok(AEsperar {
                        chave: r.get(0)?,
                        ciencia: r.get(1)?,
                        tentativas: r.get(2)?,
                    })
                },
            )
            .map_err(falha)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(falha);
        v
    }

    fn adiar(
        x: &Contexto<'_>,
        chave: &str,
        motivo: String,
        conta_tentativa: bool,
    ) -> Resultado<()> {
        let (empresa, chave, proxima) = (
            x.empresa(),
            chave.to_owned(),
            x.agora.em_micros() + micros(UMA_HORA),
        );
        x.escrever(move |uow, _| {
            uow.conexao()
                .execute(
                    "UPDATE compras_dfe_nota SET motivo = ?3, proxima_tentativa = ?4,
                       tentativas = tentativas + ?5
                     WHERE empresa = ?1 AND chave = ?2",
                    params![
                        empresa.em_bytes().as_slice(),
                        chave,
                        motivo,
                        proxima,
                        i64::from(conta_tentativa)
                    ],
                )
                .map(|_| ())
                .map_err(falha)
        })
    }

    enum Parar {
        Nao,
        Sim,
    }

    /// Dá ciência a uma nota que espera e busca a completa.
    fn cuidar_da_espera(x: &Contexto<'_>, n: &AEsperar, r: &mut Rodada) -> Resultado<Parar> {
        let Ok(chave) = ChaveAcesso::nova(&n.chave) else {
            return Ok(Parar::Nao);
        };
        if !n.ciencia {
            if n.tentativas >= TENTATIVAS_DA_CIENCIA {
                return Ok(Parar::Nao);
            }
            match x
                .porta
                .manifestar(&x.cnpj, &chave, Manifestacao::Ciencia, None)
            {
                Ok(ret) if ret.situacao == SituacaoRetorno::Autorizado || ret.codigo == "573" => {
                    let (empresa, c, agora) = (x.empresa(), n.chave.clone(), x.agora);
                    x.escrever(move |uow, _| {
                        uow.conexao()
                            .execute(
                                "UPDATE compras_dfe_nota SET ciencia_em = ?3,
                                   ciencia_protocolo = ?4, ciencia_xml = ?5, motivo = NULL
                                 WHERE empresa = ?1 AND chave = ?2",
                                params![
                                    empresa.em_bytes().as_slice(),
                                    c,
                                    agora.em_micros(),
                                    ret.protocolo,
                                    ret.xml
                                ],
                            )
                            .map(|_| ())
                            .map_err(falha)
                    })?;
                }
                // 596: a ciência só vale até 10 dias da emissão — a SEFAZ não libera mais a nota;
                // o XML vem do fornecedor (e a nota sai da espera ao ser importada).
                Ok(ret) if ret.codigo == "596" => {
                    let motivo = "passou o prazo da ciência (10 dias da emissão): peça o XML ao \
                                  fornecedor e use Importar XML"
                        .to_owned();
                    adiar(x, &n.chave, motivo, true)?;
                    let (empresa, c) = (x.empresa(), n.chave.clone());
                    x.escrever(move |uow, _| {
                        uow.conexao()
                            .execute(
                                "UPDATE compras_dfe_nota SET tentativas = ?3
                                 WHERE empresa = ?1 AND chave = ?2",
                                params![empresa.em_bytes().as_slice(), c, TENTATIVAS_DA_CIENCIA],
                            )
                            .map(|_| ())
                            .map_err(falha)
                    })?;
                    return Ok(Parar::Nao);
                }
                Ok(ret) => {
                    let motivo = format!("a ciência foi recusada ({}): {}", ret.codigo, ret.motivo);
                    adiar(x, &n.chave, motivo, true)?;
                    return Ok(Parar::Nao);
                }
                Err(e) if e.codigo == CodigoErro::FISCAL_INDISPONIVEL => return Ok(Parar::Sim),
                Err(e) => {
                    adiar(x, &n.chave, e.mensagem, true)?;
                    return Ok(Parar::Nao);
                }
            }
        }
        match x.porta.consultar_dfe_por_chave(&x.cnpj, x.uf, &chave) {
            Ok(p) if p.consumo_indevido => Ok(Parar::Sim),
            Ok(p) => {
                match p.documentos.iter().find(|d| d.tipo == TipoDfe::Nota) {
                    Some(nota) => processar_nota(x, nota, r)?,
                    None => adiar(
                        x,
                        &n.chave,
                        "com ciência; a SEFAZ ainda não liberou a nota completa".into(),
                        false,
                    )?,
                }
                Ok(Parar::Nao)
            }
            Err(e) if e.codigo == CodigoErro::FISCAL_INDISPONIVEL => Ok(Parar::Sim),
            Err(e) => {
                adiar(x, &n.chave, e.mensagem, false)?;
                Ok(Parar::Nao)
            }
        }
    }

    // ------------------------------------------------------------------ a tarefa

    /// Busca as notas da empresa na SEFAZ: as páginas da distribuição desde o último NSU, e
    /// a ciência das que chegaram só como resumo.
    ///
    /// # Errors
    /// A instalação não consegue falar com a API (sem certificado, sem provedor) ou a API não
    /// respondeu — a rodada fica registrada com o motivo.
    pub(crate) fn buscar_notas_na_sefaz(t: &ContextoTarefa<'_>) -> Resultado<String> {
        let empresa = t.ctx.empresa;
        let agora = Instante::agora();
        let ambiente = t.ler(|c| mod_fiscal::ambiente_da_empresa(c, empresa))?;
        let Some(nsu_inicial) =
            t.escrever(move |uow| tomar_a_vez(uow.conexao(), empresa, ambiente, agora))?
        else {
            let atual = t.ler(|c| situacao(c, empresa, ambiente, agora))?;
            return Ok(match atual.proxima_consulta {
                Some(p) => format!(
                    "a SEFAZ só aceita nova consulta às {}",
                    p.hora(t.ctx.fuso).formatar()
                ),
                None => "já há uma busca em andamento".into(),
            });
        };
        let preparo = t.ler(|c| {
            let cnpj = cnpj_da_empresa(c, empresa)?;
            let uf = mod_empresa::sql::dados_fiscais(c, empresa)?
                .uf()
                .ok_or_else(|| {
                    Erro::novo(
                        CodigoErro::ESTADO_INVALIDO,
                        "a UF da empresa não está cadastrada (Configurações › Fiscal)",
                    )
                })?;
            let porta = mod_fiscal::porta_da_empresa_no_ambiente(&t.ctx, c, ambiente)?;
            Ok((cnpj, uf, porta))
        });
        let (cnpj, uf, porta) = match preparo {
            Ok(p) => p,
            Err(e) => {
                let m = format!("não deu para buscar: {}", e.mensagem);
                let proxima = agora.mais_segundos(UMA_HORA);
                t.escrever(move |uow| {
                    fechar_rodada(uow.conexao(), empresa, ambiente, agora, proxima, &m)
                })?;
                return Err(e);
            }
        };
        let contexto = Contexto {
            t,
            porta: porta.as_ref(),
            cnpj,
            uf,
            ambiente,
            agora,
        };
        let mut rodada = Rodada::default();
        let (proxima, erro) = paginas(&contexto, nsu_inicial, &mut rodada)?;
        if erro.is_none() {
            for n in t.ler(|c| a_esperar(c, empresa, ambiente, agora))? {
                if matches!(cuidar_da_espera(&contexto, &n, &mut rodada)?, Parar::Sim) {
                    break;
                }
            }
        }
        let esperando = t.ler(|c| esperando(c, empresa, ambiente))?;
        let mut texto = rodada.resumo(esperando);
        if let Some(e) = &erro {
            let _ = write!(texto, "; a SEFAZ não respondeu ({})", e.mensagem);
        }
        let fechamento = texto.clone();
        t.escrever(move |uow| {
            fechar_rodada(
                uow.conexao(),
                empresa,
                ambiente,
                agora,
                proxima,
                &fechamento,
            )
        })?;
        match erro {
            Some(e)
                if rodada.importadas + rodada.aguardando + rodada.ignoradas + rodada.erros == 0 =>
            {
                Err(e)
            }
            _ => Ok(texto),
        }
    }

    /// As páginas da distribuição. Devolve quando pode a próxima rodada e o erro da API, se
    /// ela parou de responder.
    fn paginas(
        x: &Contexto<'_>,
        desde: u64,
        r: &mut Rodada,
    ) -> Resultado<(Instante, Option<Erro>)> {
        let (empresa, ambiente) = (x.empresa(), x.ambiente);
        let mut nsu = desde;
        for _ in 0..PAGINAS_POR_RODADA {
            let pagina = match x.porta.distribuicao_dfe(&x.cnpj, x.uf, Nsu(nsu)) {
                Ok(p) => p,
                Err(e) => {
                    return Ok((x.agora.mais_segundos(SEM_RESPOSTA_SEGUNDOS), Some(e)));
                }
            };
            if pagina.consumo_indevido {
                r.avisos
                    .push("a SEFAZ pediu uma hora de espera (consumo indevido)".into());
                return Ok((x.agora.mais_segundos(UMA_HORA), None));
            }
            for doc in &pagina.documentos {
                processar(x, doc, r)?;
            }
            let ultimo = pagina.ultimo_nsu.0;
            if ultimo > nsu {
                x.escrever(move |uow, _| avancar_nsu(uow.conexao(), empresa, ambiente, ultimo))?;
                nsu = ultimo;
            }
            if pagina.ultimo_nsu >= pagina.maior_nsu || pagina.documentos.is_empty() {
                return Ok((x.agora.mais_segundos(UMA_HORA), None));
            }
        }
        // Ainda há páginas: a próxima rodada continua já.
        Ok((x.agora, None))
    }

    /// [`super::ImportarDeNovoDaSefaz`].
    pub(crate) fn importar_de_novo(
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
        chave: &str,
    ) -> Resultado<RelatorioImportacao> {
        let xml: Option<String> = uow
            .conexao()
            .query_row(
                "SELECT xml FROM compras_dfe_nota WHERE empresa = ?1 AND chave = ?2",
                params![ctx.empresa.em_bytes().as_slice(), chave],
                |r| r.get(0),
            )
            .optional()
            .map_err(falha)?
            .flatten();
        let xml = xml.ok_or_else(|| Erro::nao_encontrado("XML da nota que chegou da SEFAZ"))?;
        let nota = interpretar(&xml).map_err(|e| Erro::de_dominio(&e))?;
        let relatorio = importar_nota_da_sefaz(&nota, &xml, ctx, uow)?;
        marcar(
            uow.conexao(),
            ctx.empresa,
            chave,
            EstadoNaSefaz::Baixada,
            None,
        )?;
        Ok(relatorio)
    }
}
