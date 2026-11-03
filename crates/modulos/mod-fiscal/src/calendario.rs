//! O **calendário de obrigações**: o que vence no mês, pelo regime da empresa — as federais de
//! data fixa (DAS/PGDAS-D, DEFIS, DASN-SIMEI, PIS/COFINS, IRPJ/CSLL, EFD-Contribuições,
//! DCTFWeb/EFD-Reinf, ECD, ECF; com funcionários, eSocial e FGTS Digital), o ICMS e a EFD ICMS/IPI
//! sem data (o prazo é do estado e da atividade), e as do próprio Cardeal (inutilizar os números
//! sem desfecho até o dia 10; o pacote do contador).
//!
//! Os dias úteis descontam os fins de semana e os feriados nacionais (com a Sexta-feira Santa e a
//! segunda e a terça de Carnaval, em que os bancos não abrem). Feriado estadual ou municipal não
//! entra: as datas são as da regra, e a tela pede para conferir com o contador.

#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;
use cardeal_kernel::{Competencia, Data};
use cardeal_modkit::Consulta;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// O domingo de Páscoa (algoritmo de Meeus/Jones/Butcher).
#[must_use]
#[allow(clippy::many_single_char_names)] // as letras do algoritmo publicado
pub fn pascoa(ano: i32) -> Data {
    let a = ano % 19;
    let b = ano / 100;
    let c = ano % 100;
    let d = b / 4;
    let e = b % 4;
    let f = (b + 8) / 25;
    let g = (b - f + 1) / 3;
    let h = (19 * a + b - d - g + 15) % 30;
    let i = c / 4;
    let k = c % 4;
    let l = (32 + 2 * e + 2 * i - h - k) % 7;
    let m = (a + 11 * h + 22 * l) / 451;
    let mes = (h + l - 7 * m + 114) / 31;
    let dia = ((h + l - 7 * m + 114) % 31) + 1;
    Data::de_ymd(
        ano,
        u32::try_from(mes).unwrap_or(4),
        u32::try_from(dia).unwrap_or(1),
    )
    .unwrap_or_else(|_| Data::de_dias(0))
}

/// Os feriados nacionais em que os bancos não abrem.
#[must_use]
pub fn feriados(ano: i32) -> Vec<Data> {
    let fixo = |m, d| Data::de_ymd(ano, m, d).ok();
    let p = pascoa(ano);
    let mut v: Vec<Data> = [
        fixo(1, 1),
        fixo(4, 21),
        fixo(5, 1),
        fixo(9, 7),
        fixo(10, 12),
        fixo(11, 2),
        fixo(11, 15),
        fixo(11, 20),
        fixo(12, 25),
    ]
    .into_iter()
    .flatten()
    .collect();
    // Carnaval (segunda e terça) e Sexta-feira Santa.
    v.extend([p.mais_dias(-48), p.mais_dias(-47), p.mais_dias(-2)]);
    v.sort();
    v
}

/// Dia útil: nem fim de semana nem feriado nacional.
#[must_use]
pub fn e_dia_util(d: Data) -> bool {
    !d.dia_da_semana().e_fim_de_semana() && !feriados(d.ano()).contains(&d)
}

/// O próprio dia, se útil; senão o próximo útil (a data que **prorroga**).
#[must_use]
pub fn prorrogar(mut d: Data) -> Data {
    while !e_dia_util(d) {
        d = d.mais_dias(1);
    }
    d
}

/// O próprio dia, se útil; senão o útil anterior (a data que **antecipa**).
#[must_use]
pub fn antecipar(mut d: Data) -> Data {
    while !e_dia_util(d) {
        d = d.mais_dias(-1);
    }
    d
}

/// O último dia útil do mês.
#[must_use]
pub fn ultimo_dia_util(mes: Competencia) -> Data {
    antecipar(mes.ultimo_dia())
}

/// O n-ésimo dia útil do mês.
#[must_use]
pub fn dia_util(mes: Competencia, n: u32) -> Data {
    let mut d = prorrogar(mes.primeiro_dia());
    for _ in 1..n {
        d = prorrogar(d.mais_dias(1));
    }
    d
}

/// De onde vem a obrigação.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Esfera {
    /// Receita Federal / governo federal.
    Federal,
    /// O estado (ICMS).
    Estadual,
    /// Do próprio Cardeal (o que a empresa precisa fazer aqui).
    Cardeal,
}

/// Uma obrigação do mês.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Obrigacao {
    /// O nome ("DAS e PGDAS-D").
    pub nome: String,
    /// Quando vence (`None`: depende do estado — conferir com o contador).
    pub quando: Option<Data>,
    /// A que se refere ("09/2026", "ano de 2025", "3º trimestre de 2026").
    pub referencia: String,
    /// O que é, numa frase.
    pub descricao: String,
    /// Só vale para quem tem funcionários.
    pub so_com_funcionarios: bool,
    /// De onde vem.
    pub esfera: Esfera,
}

fn ref_mes(c: Competencia) -> String {
    format!("{:02}/{}", c.mes(), c.ano())
}

fn ob(
    nome: &str,
    quando: Option<Data>,
    referencia: String,
    descricao: &str,
    so_com_funcionarios: bool,
    esfera: Esfera,
) -> Obrigacao {
    Obrigacao {
        nome: nome.into(),
        quando,
        referencia,
        descricao: descricao.into(),
        so_com_funcionarios,
        esfera,
    }
}

/// O mês anterior.
fn anterior(c: Competencia) -> Competencia {
    if c.mes() == 1 {
        Competencia::nova(c.ano() - 1, 12)
    } else {
        Competencia::nova(c.ano(), c.mes() - 1)
    }
}

/// As obrigações federais e estaduais que vencem em `mes`, pelo regime (`MEI`,
/// `SimplesNacional`, `LucroPresumido`, `LucroReal`), na ordem da data.
#[must_use]
pub fn obrigacoes_do_mes(regime: &str, mes: Competencia) -> Vec<Obrigacao> {
    let p = anterior(mes);
    let dia = |d: u32| Data::de_ymd(mes.ano(), mes.mes(), d).ok();
    let mut v = Vec::new();
    // Com funcionários, em qualquer regime (o MEI pode ter um).
    v.push(ob(
        "eSocial (folha do mês)",
        dia(15).map(antecipar),
        ref_mes(p),
        "Os eventos periódicos da folha (remunerações e pagamentos) do mês anterior.",
        true,
        Esfera::Federal,
    ));
    v.push(ob(
        "FGTS Digital",
        dia(20).map(antecipar),
        ref_mes(p),
        "A guia do FGTS da folha do mês anterior, gerada no FGTS Digital a partir do eSocial.",
        true,
        Esfera::Federal,
    ));
    match regime {
        "MEI" => do_mei(mes, p, &mut v),
        "LucroPresumido" | "LucroReal" => do_lucro(regime, mes, p, &mut v),
        // Simples Nacional (e o que sobrar).
        _ => do_simples(mes, p, &mut v),
    }
    v.sort_by_key(|o| (o.quando.is_none(), o.quando));
    v
}

fn do_mei(mes: Competencia, p: Competencia, v: &mut Vec<Obrigacao>) {
    let dia = |d: u32| Data::de_ymd(mes.ano(), mes.mes(), d).ok();
    v.push(ob(
        "DAS-MEI",
        dia(20).map(prorrogar),
        ref_mes(p),
        "O valor fixo mensal do MEI (INSS, e ICMS e/ou ISS conforme a atividade).",
        false,
        Esfera::Federal,
    ));
    if mes.mes() == 5 {
        v.push(ob(
            "DASN-SIMEI",
            dia(31),
            format!("ano de {}", mes.ano() - 1),
            "A declaração anual do MEI: o faturamento do ano e se teve empregado.",
            false,
            Esfera::Federal,
        ));
    }
}

fn do_lucro(regime: &str, mes: Competencia, p: Competencia, v: &mut Vec<Obrigacao>) {
    let dia = |d: u32| Data::de_ymd(mes.ano(), mes.mes(), d).ok();
    v.push(ob(
        "DCTFWeb e EFD-Reinf",
        dia(15).map(antecipar),
        ref_mes(p),
        "A EFD-Reinf (retenções e contribuições) e a DCTFWeb, que confessa os débitos \
         previdenciários e as retenções do mês anterior.",
        false,
        Esfera::Federal,
    ));
    v.push(ob(
        "PIS e COFINS",
        dia(25).map(antecipar),
        ref_mes(p),
        "O DARF do PIS e da COFINS sobre o faturamento do mês anterior.",
        false,
        Esfera::Federal,
    ));
    let pp = anterior(p);
    v.push(ob(
        "EFD-Contribuições",
        Some(dia_util(mes, 10)),
        ref_mes(pp),
        "A escrituração do PIS e da COFINS (10º dia útil do segundo mês seguinte).",
        false,
        Esfera::Federal,
    ));
    if matches!(p.mes(), 3 | 6 | 9 | 12) {
        v.push(ob(
            "IRPJ e CSLL do trimestre",
            Some(ultimo_dia_util(mes)),
            format!("{}º trimestre de {}", p.mes() / 3, p.ano()),
            if regime == "LucroReal" {
                "Na apuração trimestral; quem optou pela estimativa mensal paga todo mês, \
                 no último dia útil do mês seguinte."
            } else {
                "O IRPJ e a CSLL do trimestre (a quota única ou a primeira de três)."
            },
            false,
            Esfera::Federal,
        ));
    } else if regime == "LucroReal" {
        v.push(ob(
            "IRPJ e CSLL por estimativa",
            Some(ultimo_dia_util(mes)),
            ref_mes(p),
            "Só para quem optou pela estimativa mensal.",
            false,
            Esfera::Federal,
        ));
    }
    if mes.mes() == 6 {
        v.push(ob(
            "ECD",
            Some(ultimo_dia_util(mes)),
            format!("ano de {}", mes.ano() - 1),
            "A escrituração contábil digital do ano anterior (o contador entrega).",
            false,
            Esfera::Federal,
        ));
    }
    if mes.mes() == 7 {
        v.push(ob(
            "ECF",
            Some(ultimo_dia_util(mes)),
            format!("ano de {}", mes.ano() - 1),
            "A escrituração contábil fiscal do ano anterior (o contador entrega).",
            false,
            Esfera::Federal,
        ));
    }
    v.push(ob(
        "ICMS e EFD ICMS/IPI",
        None,
        ref_mes(p),
        "O ICMS próprio e a escrituração fiscal do mês anterior — o prazo é do seu estado \
         (e da atividade): confira com o contador.",
        false,
        Esfera::Estadual,
    ));
}

fn do_simples(mes: Competencia, p: Competencia, v: &mut Vec<Obrigacao>) {
    let dia = |d: u32| Data::de_ymd(mes.ano(), mes.mes(), d).ok();
    v.push(ob(
        "DAS e PGDAS-D",
        dia(20).map(prorrogar),
        ref_mes(p),
        "A apuração do Simples (PGDAS-D) sobre o faturamento do mês anterior e o \
         pagamento do DAS.",
        false,
        Esfera::Federal,
    ));
    if mes.mes() == 3 {
        v.push(ob(
            "DEFIS",
            dia(31),
            format!("ano de {}", mes.ano() - 1),
            "A declaração anual do Simples: as informações socioeconômicas e fiscais do \
             ano anterior.",
            false,
            Esfera::Federal,
        ));
    }
    v.push(ob(
        "DCTFWeb",
        dia(15).map(antecipar),
        ref_mes(p),
        "Com funcionários: a confissão das contribuições da folha (a do Simples sai no \
         DAS, menos no Anexo IV).",
        true,
        Esfera::Federal,
    ));
}

/// As obrigações que vencem num mês, com as do Cardeal (os números sem desfecho a inutilizar
/// até o dia 10, o pacote do contador).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ObrigacoesDoMes {
    /// O mês do vencimento.
    pub mes: Competencia,
}

impl Consulta for ObrigacoesDoMes {
    type Saida = Vec<Obrigacao>;
    const PERMISSAO: &'static str = "fiscal.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<Vec<Obrigacao>> {
        let regime = mod_empresa::sql::dados(c, ctx.empresa)?.regime;
        let mut v = obrigacoes_do_mes(&regime, self.mes);
        let p = anterior(self.mes);
        let fim_de_p = cardeal_kernel::Instante::de_data_hora(
            self.mes.primeiro_dia(),
            cardeal_kernel::Hora::MEIA_NOITE,
            ctx.fuso,
        );
        let sem_desfecho = crate::sql_documento::sem_desfecho_ate(c, ctx.empresa, fim_de_p)?.len();
        if sem_desfecho > 0 {
            v.push(Obrigacao {
                nome: "Inutilizar os números sem desfecho".into(),
                quando: Data::de_ymd(self.mes.ano(), self.mes.mes(), 10).ok(),
                referencia: ref_mes(p),
                descricao: format!(
                    "{sem_desfecho} nota(s) de {} sem autorização nem inutilização — o pedido de \
                     inutilização vai até o dia 10 (Notas fiscais › Inutilizar número).",
                    ref_mes(p)
                ),
                so_com_funcionarios: false,
                esfera: Esfera::Cardeal,
            });
        }
        v.push(Obrigacao {
            nome: "Pacote do contador".into(),
            quando: Data::de_ymd(self.mes.ano(), self.mes.mes(), 5).ok(),
            referencia: ref_mes(p),
            descricao: "Sugestão: o contador precisa dos relatórios e dos XMLs do mês antes do \
                        vencimento dos impostos (Financeiro › Pacote do contador)."
                .into(),
            so_com_funcionarios: false,
            esfera: Esfera::Cardeal,
        });
        v.sort_by_key(|o| (o.quando.is_none(), o.quando));
        Ok(v)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn d(a: i32, m: u32, dia: u32) -> Data {
        Data::de_ymd(a, m, dia).unwrap()
    }

    #[test]
    fn pascoa_e_feriados() {
        assert_eq!(pascoa(2026), d(2026, 4, 5));
        assert_eq!(pascoa(2027), d(2027, 3, 28));
        let f = feriados(2026);
        assert!(
            f.contains(&d(2026, 2, 16)) && f.contains(&d(2026, 2, 17)),
            "Carnaval"
        );
        assert!(f.contains(&d(2026, 4, 3)), "Sexta-feira Santa");
        assert!(f.contains(&d(2026, 11, 20)), "Consciência Negra");
    }

    #[test]
    fn prorroga_e_antecipa() {
        // 20/12/2026 é domingo: o DAS prorroga para segunda, 21; o FGTS antecipa para sexta, 18.
        assert_eq!(prorrogar(d(2026, 12, 20)), d(2026, 12, 21));
        assert_eq!(antecipar(d(2026, 12, 20)), d(2026, 12, 18));
        // 15/11/2026 é domingo e feriado; 14 é sábado: antecipa para sexta, 13.
        assert_eq!(antecipar(d(2026, 11, 15)), d(2026, 11, 13));
        assert_eq!(
            ultimo_dia_util(Competencia::nova(2026, 10)),
            d(2026, 10, 30)
        );
        // Outubro de 2026: 1 é quinta; o 12 é feriado — o 10º dia útil é 15.
        assert_eq!(dia_util(Competencia::nova(2026, 10), 10), d(2026, 10, 15));
    }

    #[test]
    fn simples_presumido_e_mei() {
        let s = obrigacoes_do_mes("SimplesNacional", Competencia::nova(2026, 12));
        let das = s.iter().find(|o| o.nome == "DAS e PGDAS-D").unwrap();
        assert_eq!(
            (das.quando, das.referencia.as_str()),
            (Some(d(2026, 12, 21)), "11/2026")
        );
        assert!(s.iter().all(|o| o.nome != "DEFIS"));
        let marco = obrigacoes_do_mes("SimplesNacional", Competencia::nova(2027, 3));
        assert!(marco
            .iter()
            .any(|o| o.nome == "DEFIS" && o.referencia == "ano de 2026"));

        let p = obrigacoes_do_mes("LucroPresumido", Competencia::nova(2026, 10));
        let nomes: Vec<&str> = p.iter().map(|o| o.nome.as_str()).collect();
        assert!(nomes.contains(&"PIS e COFINS"));
        assert!(nomes.contains(&"IRPJ e CSLL do trimestre"), "{nomes:?}");
        assert!(nomes.contains(&"EFD-Contribuições"));
        assert_eq!(
            nomes.last(),
            Some(&"ICMS e EFD ICMS/IPI"),
            "sem data vai por último"
        );
        let irpj = p
            .iter()
            .find(|o| o.nome == "IRPJ e CSLL do trimestre")
            .unwrap();
        assert_eq!(irpj.referencia, "3º trimestre de 2026");
        assert_eq!(irpj.quando, Some(d(2026, 10, 30)));

        let mei = obrigacoes_do_mes("MEI", Competencia::nova(2026, 5));
        assert!(mei.iter().any(|o| o.nome == "DASN-SIMEI"));
        assert!(mei.iter().any(|o| o.nome == "DAS-MEI"));
    }
}
