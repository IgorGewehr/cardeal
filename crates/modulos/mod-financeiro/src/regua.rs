//! A régua de cobrança: em que dias, contados do vencimento, cada cliente é lembrado ou cobrado
//! — e com que mensagem. Todo dia o sistema monta a **fila de hoje**: para cada cliente com
//! parcela em aberto, a etapa mais recente que já chegou e ainda não foi enviada. Um cliente
//! com várias parcelas recebe uma mensagem só (na etapa mais grave, com o total); cada etapa
//! sai uma vez por parcela; quem começa a usar com um atraso grande cai direto na etapa atual,
//! sem receber as anteriores. O envio é de um clique (`WhatsApp`) e fica registrado; quando
//! houver uma API de `WhatsApp`, quem envia passa a ser o sistema — a fila é a mesma.

use std::collections::BTreeMap;

#[cfg(feature = "sqlite")]
use cardeal_kernel::{CodigoErro, Erro, Resultado};
use cardeal_kernel::{Data, Dinheiro, Id};
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::{Comando, Consulta, Risco};
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
#[cfg(feature = "sqlite")]
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use crate::repositorio::{blob, persist};

/// Uma etapa da régua.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EtapaRegua {
    /// Dias contados do vencimento: negativo = antes (lembrete), 0 = no dia, positivo = atraso.
    pub dias: i32,
    /// O nome ("Lembrete", "7 dias de atraso").
    pub nome: String,
    /// O modelo da mensagem, com `{nome}`, `{valor}`, `{vencimento}`, `{dias}` e `{empresa}`.
    pub mensagem: String,
}

fn etapa(dias: i32, nome: &str, mensagem: &str) -> EtapaRegua {
    EtapaRegua {
        dias,
        nome: nome.to_owned(),
        mensagem: mensagem.to_owned(),
    }
}

/// A régua que vale enquanto a empresa não configurar a sua.
#[must_use]
pub fn regua_padrao() -> Vec<EtapaRegua> {
    vec![
        etapa(
            -3,
            "Lembrete",
            "Olá, {nome}! Passando para lembrar que {valor} vence em {vencimento}. Qualquer \
             dúvida, estamos à disposição. {empresa}",
        ),
        etapa(
            0,
            "Vence hoje",
            "Olá, {nome}! Hoje vence {valor}. Se já pagou, pode desconsiderar. {empresa}",
        ),
        etapa(
            1,
            "1 dia de atraso",
            "Olá, {nome}! Não identificamos o pagamento de {valor}, que venceu em {vencimento}. \
             Se já pagou, é só nos mandar o comprovante. {empresa}",
        ),
        etapa(
            7,
            "7 dias de atraso",
            "Olá, {nome}! Consta em aberto {valor}, vencido há {dias} dias. Podemos combinar o \
             pagamento? {empresa}",
        ),
        etapa(
            15,
            "15 dias de atraso",
            "Olá, {nome}. O valor de {valor} está em atraso há {dias} dias. Precisamos \
             regularizar — podemos negociar uma forma de pagamento? {empresa}",
        ),
        etapa(
            30,
            "30 dias de atraso",
            "Olá, {nome}. Seu débito de {valor} completou {dias} dias de atraso. Entre em \
             contato para combinarmos a quitação. {empresa}",
        ),
    ]
}

/// Por que uma régua foi recusada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReguaInvalida {
    /// Nenhuma etapa.
    Vazia,
    /// Mais etapas que o limite.
    Demais,
    /// Dia fora de −30..=365.
    DiaForaDoLimite(i32),
    /// Dois passos no mesmo dia.
    DiaRepetido(i32),
    /// Etapa sem nome ou sem mensagem.
    EtapaIncompleta(i32),
}

impl ReguaInvalida {
    /// A mensagem para o usuário.
    #[must_use]
    pub fn mensagem(&self) -> String {
        match self {
            Self::Vazia => "A régua precisa de pelo menos uma etapa".to_owned(),
            Self::Demais => "A régua aceita até 12 etapas".to_owned(),
            Self::DiaForaDoLimite(d) => {
                format!("{d} dias: use de 30 dias antes até 365 dias depois do vencimento")
            }
            Self::DiaRepetido(d) => format!("Duas etapas no mesmo dia ({d})"),
            Self::EtapaIncompleta(d) => format!("A etapa de {d} dias precisa de nome e mensagem"),
        }
    }
}

/// Confere a régua e devolve as etapas em ordem de dia, com nome e mensagem aparados.
///
/// # Errors
/// [`ReguaInvalida`] com o primeiro problema encontrado.
pub fn validar(mut etapas: Vec<EtapaRegua>) -> Result<Vec<EtapaRegua>, ReguaInvalida> {
    if etapas.is_empty() {
        return Err(ReguaInvalida::Vazia);
    }
    if etapas.len() > 12 {
        return Err(ReguaInvalida::Demais);
    }
    etapas.sort_by_key(|e| e.dias);
    for e in &mut etapas {
        e.nome = e.nome.trim().to_owned();
        e.mensagem = e.mensagem.trim().to_owned();
        if !(-30..=365).contains(&e.dias) {
            return Err(ReguaInvalida::DiaForaDoLimite(e.dias));
        }
        if e.nome.is_empty() || e.mensagem.is_empty() {
            return Err(ReguaInvalida::EtapaIncompleta(e.dias));
        }
    }
    if let Some(w) = etapas.windows(2).find(|w| w[0].dias == w[1].dias) {
        return Err(ReguaInvalida::DiaRepetido(w[0].dias));
    }
    Ok(etapas)
}

/// Uma parcela a receber em aberto, com as etapas já enviadas (ou puladas) para ela.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParcelaACobrar {
    /// A parcela.
    pub parcela: Id,
    /// O cliente.
    pub cliente: Id,
    /// O vencimento.
    pub vencimento: Data,
    /// O saldo em aberto.
    pub saldo: Dinheiro,
    /// Os dias das etapas já registradas para esta parcela.
    pub ja_registradas: Vec<i32>,
}

/// A etapa que cabe hoje a uma parcela: a mais recente cujo dia já chegou — se nem ela nem uma
/// posterior foi registrada.
#[must_use]
pub fn etapa_devida<'a>(
    etapas: &'a [EtapaRegua],
    dias_do_vencimento: i32,
    ja_registradas: &[i32],
) -> Option<&'a EtapaRegua> {
    let devida = etapas
        .iter()
        .filter(|e| e.dias <= dias_do_vencimento)
        .max_by_key(|e| e.dias)?;
    (!ja_registradas.iter().any(|d| *d >= devida.dias)).then_some(devida)
}

/// Um cliente a cobrar hoje.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemDaFila {
    /// O cliente.
    pub cliente: Id,
    /// A etapa (a mais grave entre as parcelas dele).
    pub etapa: EtapaRegua,
    /// As parcelas incluídas e a etapa de cada uma — o que o registro do envio marca.
    pub parcelas: Vec<(Id, i32)>,
    /// O total dessas parcelas.
    pub valor: Dinheiro,
    /// O vencimento mais antigo entre elas.
    pub vencimento: Data,
    /// Dias desde esse vencimento (negativo = ainda vai vencer).
    pub dias: i32,
}

/// A fila de hoje: um item por cliente, os mais atrasados primeiro.
#[must_use]
pub fn montar_fila(
    etapas: &[EtapaRegua],
    parcelas: &[ParcelaACobrar],
    hoje: Data,
) -> Vec<ItemDaFila> {
    let mut por_cliente: BTreeMap<Id, ItemDaFila> = BTreeMap::new();
    for p in parcelas {
        let dias = hoje.em_dias() - p.vencimento.em_dias();
        let Some(e) = etapa_devida(etapas, dias, &p.ja_registradas) else {
            continue;
        };
        let item = por_cliente.entry(p.cliente).or_insert_with(|| ItemDaFila {
            cliente: p.cliente,
            etapa: e.clone(),
            parcelas: Vec::new(),
            valor: Dinheiro::ZERO,
            vencimento: p.vencimento,
            dias,
        });
        if e.dias > item.etapa.dias {
            item.etapa = e.clone();
        }
        if p.vencimento < item.vencimento {
            item.vencimento = p.vencimento;
            item.dias = dias;
        }
        item.parcelas.push((p.parcela, e.dias));
        item.valor += p.saldo;
    }
    let mut fila: Vec<ItemDaFila> = por_cliente.into_values().collect();
    fila.sort_by_key(|i| std::cmp::Reverse(i.dias));
    fila
}

/// A mensagem pronta: o modelo com o primeiro nome do cliente, o valor, o vencimento, os dias
/// (sempre positivos: "vence em 3", "vencido há 7") e o nome da empresa.
#[must_use]
pub fn redigir(modelo: &str, nome: &str, item: &ItemDaFila, empresa: &str) -> String {
    let primeiro = nome.split_whitespace().next().unwrap_or(nome);
    modelo
        .replace("{nome}", primeiro)
        .replace("{valor}", &item.valor.formatar_com_simbolo())
        .replace("{vencimento}", &item.vencimento.formatar())
        .replace("{dias}", &item.dias.unsigned_abs().to_string())
        .replace("{empresa}", empresa)
        .trim()
        .to_owned()
}

#[cfg(feature = "sqlite")]
fn etapas_da_empresa(conexao: &Connection, empresa: Id) -> Resultado<Vec<EtapaRegua>> {
    let mut stmt = conexao
        .prepare(
            "SELECT dias, nome, mensagem FROM financeiro_regua_etapa
             WHERE empresa = ?1 ORDER BY dias",
        )
        .map_err(persist)?;
    let etapas: Vec<EtapaRegua> = stmt
        .query_map([blob(empresa)], |r| {
            Ok(EtapaRegua {
                dias: r.get(0)?,
                nome: r.get(1)?,
                mensagem: r.get(2)?,
            })
        })
        .map_err(persist)?
        .collect::<rusqlite::Result<_>>()
        .map_err(persist)?;
    Ok(if etapas.is_empty() {
        regua_padrao()
    } else {
        etapas
    })
}

/// A régua da empresa (a padrão, se ela não configurou a sua).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ReguaDeCobranca;

impl Consulta for ReguaDeCobranca {
    type Saida = Vec<EtapaRegua>;
    const PERMISSAO: &'static str = "financeiro.receber.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        etapas_da_empresa(conexao, ctx.empresa)
    }
}

/// A fila de cobrança do dia.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilaDoDia {
    /// O nome da empresa, para a assinatura das mensagens.
    pub empresa: String,
    /// Os clientes a cobrar.
    pub itens: Vec<ItemDaFila>,
}

/// Quem cobrar hoje, segundo a régua.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct FilaDeCobranca;

impl Consulta for FilaDeCobranca {
    type Saida = FilaDoDia;
    const PERMISSAO: &'static str = "financeiro.receber.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let etapas = etapas_da_empresa(conexao, ctx.empresa)?;
        let mut registradas: BTreeMap<Id, Vec<i32>> = BTreeMap::new();
        {
            let mut stmt = conexao
                .prepare(
                    "SELECT parcela, etapa_dias FROM financeiro_cobranca_registro
                     WHERE empresa = ?1",
                )
                .map_err(persist)?;
            let linhas = stmt
                .query_map([blob(ctx.empresa)], |r| {
                    Ok((crate::repositorio::id_de(r.get(0)?), r.get::<_, i32>(1)?))
                })
                .map_err(persist)?;
            for l in linhas {
                let (p, d) = l.map_err(persist)?;
                registradas.entry(p).or_default().push(d);
            }
        }
        let parcelas: Vec<ParcelaACobrar> = crate::consultas::titulos_em_aberto(
            conexao,
            ctx.empresa,
            crate::titulo::EspecieTitulo::Receber,
        )?
        .into_iter()
        .filter_map(|t| {
            let cardeal_ledger::Contraparte::Cliente(cliente) = t.contraparte? else {
                return None;
            };
            Some(ParcelaACobrar {
                parcela: t.parcela,
                cliente,
                vencimento: t.vencimento,
                saldo: t.saldo(),
                ja_registradas: registradas.get(&t.parcela).cloned().unwrap_or_default(),
            })
        })
        .collect();
        let empresa: String = conexao
            .query_row(
                "SELECT COALESCE(NULLIF(nome_fantasia, ''), razao_social) FROM nucleo_empresa
                 WHERE id = ?1",
                [blob(ctx.empresa)],
                |r| r.get(0),
            )
            .optional()
            .map_err(persist)?
            .unwrap_or_default();
        Ok(FilaDoDia {
            empresa,
            itens: montar_fila(&etapas, &parcelas, ctx.hoje()),
        })
    }
}

/// Grava a régua da empresa (substitui a anterior inteira).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefinirReguaDeCobranca {
    /// As etapas.
    pub etapas: Vec<EtapaRegua>,
}

impl Comando for DefinirReguaDeCobranca {
    type Saida = ();
    const PERMISSAO: &'static str = "financeiro.cobranca.configurar";
    const RISCO: Risco = Risco::Medio;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let etapas = validar(self.etapas)
            .map_err(|e| Erro::novo(CodigoErro::ENTRADA_INVALIDA, e.mensagem()))?;
        let conn = uow.conexao();
        conn.execute(
            "DELETE FROM financeiro_regua_etapa WHERE empresa = ?1",
            [blob(ctx.empresa)],
        )
        .map_err(persist)?;
        for e in &etapas {
            conn.execute(
                "INSERT INTO financeiro_regua_etapa (empresa, dias, nome, mensagem)
                 VALUES (?1, ?2, ?3, ?4)",
                params![blob(ctx.empresa), e.dias, e.nome, e.mensagem],
            )
            .map_err(persist)?;
        }
        Ok(())
    }
}

/// Marca as parcelas de um item da fila como cobradas (enviado) ou puladas, cada uma na sua
/// etapa — elas saem da fila até a próxima etapa chegar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrarCobranca {
    /// (parcela, dias da etapa), como vieram no item da fila.
    pub parcelas: Vec<(Id, i32)>,
    /// `true` = pulada (não enviada); `false` = enviada.
    pub pulada: bool,
}

impl Comando for RegistrarCobranca {
    type Saida = ();
    const PERMISSAO: &'static str = "financeiro.cobranca.enviar";
    const RISCO: Risco = Risco::Baixo;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let conn = uow.conexao();
        for (parcela, dias) in &self.parcelas {
            let da_empresa: Option<i64> = conn
                .query_row(
                    "SELECT 1 FROM financeiro_parcela WHERE id = ?1 AND empresa = ?2",
                    params![blob(*parcela), blob(ctx.empresa)],
                    |r| r.get(0),
                )
                .optional()
                .map_err(persist)?;
            if da_empresa.is_none() {
                return Err(Erro::nao_encontrado("parcela"));
            }
            conn.execute(
                "INSERT OR IGNORE INTO financeiro_cobranca_registro
                    (empresa, parcela, etapa_dias, situacao, em, por)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    blob(ctx.empresa),
                    blob(*parcela),
                    dias,
                    if self.pulada { "Pulada" } else { "Enviada" },
                    ctx.agora.em_micros(),
                    blob(ctx.usuario),
                ],
            )
            .map_err(persist)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn dia(d: u32) -> Data {
        Data::de_ymd(2026, 10, d).unwrap()
    }

    fn parcela(cliente: Id, vence: u32, reais: i64, registradas: &[i32]) -> ParcelaACobrar {
        ParcelaACobrar {
            parcela: Id::novo(),
            cliente,
            vencimento: dia(vence),
            saldo: Dinheiro::reais(reais),
            ja_registradas: registradas.to_vec(),
        }
    }

    #[test]
    fn etapa_devida_e_a_mais_recente_e_so_uma_vez() {
        let r = regua_padrao();
        assert_eq!(etapa_devida(&r, -5, &[]), None, "antes do lembrete");
        assert_eq!(etapa_devida(&r, -3, &[]).unwrap().nome, "Lembrete");
        assert_eq!(etapa_devida(&r, -1, &[]).unwrap().nome, "Lembrete");
        assert_eq!(etapa_devida(&r, -1, &[-3]), None, "lembrete já enviado");
        assert_eq!(etapa_devida(&r, 0, &[-3]).unwrap().nome, "Vence hoje");
        // Começou a usar com 20 dias de atraso: cai direto nos 15, sem os anteriores.
        assert_eq!(etapa_devida(&r, 20, &[]).unwrap().dias, 15);
        // Já registrou uma etapa posterior (pulou para 30): a de 15 não volta.
        assert_eq!(etapa_devida(&r, 20, &[30]), None);
    }

    #[test]
    fn fila_agrupa_por_cliente_na_etapa_mais_grave() {
        let (ana, beto) = (Id::novo(), Id::novo());
        let parcelas = [
            parcela(ana, 6, 100, &[]),  // 8 dias → 7
            parcela(ana, 13, 50, &[]),  // 1 dia → 1
            parcela(ana, 20, 30, &[]),  // vence em 6: nada
            parcela(beto, 17, 80, &[]), // vence em 3 → lembrete
        ];
        let fila = montar_fila(&regua_padrao(), &parcelas, dia(14));
        assert_eq!(fila.len(), 2);
        let a = &fila[0];
        assert_eq!(a.cliente, ana, "o mais atrasado primeiro");
        assert_eq!(a.etapa.dias, 7);
        assert_eq!(a.valor, Dinheiro::reais(150));
        assert_eq!((a.vencimento, a.dias), (dia(6), 8));
        assert_eq!(a.parcelas.iter().map(|p| p.1).collect::<Vec<_>>(), [7, 1]);
        assert_eq!(fila[1].etapa.nome, "Lembrete");
        assert_eq!(fila[1].dias, -3);
    }

    #[test]
    fn mensagem_preenchida_e_dias_sempre_positivos() {
        let (ana, r) = (Id::novo(), regua_padrao());
        let fila = montar_fila(&r, &[parcela(ana, 17, 80, &[])], dia(14));
        let m = redigir(
            &fila[0].etapa.mensagem,
            "Ana Paula Souza",
            &fila[0],
            "Loja X",
        );
        assert_eq!(
            m,
            "Olá, Ana! Passando para lembrar que R$ 80,00 vence em 17/10/2026. Qualquer dúvida, \
             estamos à disposição. Loja X"
        );
        let atrasada = montar_fila(&r, &[parcela(ana, 1, 80, &[])], dia(14));
        assert!(
            redigir(&atrasada[0].etapa.mensagem, "Ana", &atrasada[0], "")
                .contains("vencido há 13 dias")
        );
    }

    #[test]
    fn validar_ordena_e_recusa() {
        let ok = validar(vec![etapa(7, " Sete ", " m "), etapa(-3, "L", "m")]).unwrap();
        assert_eq!(ok.iter().map(|e| e.dias).collect::<Vec<_>>(), [-3, 7]);
        assert_eq!(ok[1].nome, "Sete");
        assert_eq!(validar(vec![]), Err(ReguaInvalida::Vazia));
        assert_eq!(
            validar(vec![etapa(1, "a", "m"), etapa(1, "b", "m")]),
            Err(ReguaInvalida::DiaRepetido(1))
        );
        assert_eq!(
            validar(vec![etapa(-40, "a", "m")]),
            Err(ReguaInvalida::DiaForaDoLimite(-40))
        );
        assert_eq!(
            validar(vec![etapa(2, "a", " ")]),
            Err(ReguaInvalida::EtapaIncompleta(2))
        );
    }
}
