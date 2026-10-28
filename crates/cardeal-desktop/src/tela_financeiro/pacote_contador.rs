//! O pacote do contador: o mês escolhido na aba Resultado e caixa num PDF (DRE comparativa, DFC
//! direto e indireto, balancete, o resumo das notas fiscais e as pendências), numa planilha
//! `.xlsx` (as mesmas tabelas, o diário completo e as notas nota a nota, com números de verdade
//! para o contador trabalhar) e num ZIP com os XMLs do mês — emitidas, canceladas (a nota e o
//! evento), inutilizadas e as de entrada —, gravados numa pasta que o sistema abre em seguida,
//! pronta para anexar no e-mail ou no WhatsApp.

use std::path::{Path, PathBuf};

use super::*;
use cardeal_ledger::{LancamentoDoDiario, LinhaBalancete};
use cardeal_pdf::{
    gerar_relatorio, ColunaRelatorio, IdentidadeEmpresa, LinhaRelatorio, RelatorioPdf,
    SecaoRelatorio,
};
use cardeal_planilha::{Aba as AbaPlanilha, Celula, Planilha};
use mod_compras::{EstadoNotaEntrada, NotaDeEntradaDoMes, NotasDeEntradaDoMes};
use mod_financeiro::dfc::{BlocoDfc, Dfc, DfcIndireto};
use mod_financeiro::dre::TipoLinhaDre;
use mod_financeiro::{ComparativoDre, DiarioDoPeriodo};
use mod_fiscal::{BuscarXmlsQueFaltam, ModeloFiscal, NotasDoMes, NotasFiscaisDoMes, SituacaoNoMes};

/// As notas fiscais emitidas no mês — ou por que não entraram no pacote.
pub(super) type NotasEmitidas = Result<NotasDoMes, String>;
/// As notas de entrada do mês — ou por que não entraram no pacote.
pub(super) type NotasDeEntrada = Result<Vec<NotaDeEntradaDoMes>, String>;

/// O que entra no pacote de um mês.
pub(super) struct DadosDoPacote<'a> {
    pub(super) mes: Competencia,
    pub(super) dre: &'a ComparativoDre,
    pub(super) direto: &'a Dfc,
    pub(super) indireto: &'a DfcIndireto,
    pub(super) balancete: &'a [LinhaBalancete],
    pub(super) diario: &'a [LancamentoDoDiario],
    pub(super) emitidas: &'a NotasEmitidas,
    pub(super) entradas: &'a NotasDeEntrada,
}

const fn nome_do_modelo(m: ModeloFiscal) -> &'static str {
    match m {
        ModeloFiscal::Nfce => "NFC-e",
        ModeloFiscal::Nfe => "NF-e",
    }
}

const fn estado_da_entrada(e: EstadoNotaEntrada) -> &'static str {
    match e {
        EstadoNotaEntrada::AConferir => "A conferir",
        EstadoNotaEntrada::Conferida => "Conferida",
        EstadoNotaEntrada::Confirmada => "Confirmada",
        EstadoNotaEntrada::Devolvida => "Devolvida",
    }
}

/// O XML de uma nota de entrada: no pacote, nunca teve (lançada à mão) ou importada antes de o
/// sistema guardar o XML.
const fn xml_da_entrada(n: &NotaDeEntradaDoMes) -> &'static str {
    match (&n.xml, &n.chave) {
        (Some(_), _) => "no pacote",
        (None, None) => "lançada à mão, sem XML",
        (None, Some(_)) => "importe o arquivo XML de novo",
    }
}

/// Os XMLs do pacote, (caminho no ZIP, conteúdo): `Saidas/<modelo>/<situação>/` e `Entradas/`.
pub(super) fn arquivos_xml(
    emitidas: &NotasEmitidas,
    entradas: &NotasDeEntrada,
) -> Vec<(String, Vec<u8>)> {
    let mut v = Vec::new();
    if let Ok(e) = emitidas {
        for n in &e.notas {
            for (caminho, xml) in n.arquivos() {
                v.push((caminho, xml.as_bytes().to_vec()));
            }
        }
        for k in &e.cartas {
            let (caminho, xml) = k.arquivo();
            v.push((caminho, xml.as_bytes().to_vec()));
        }
        for n in &e.nfse {
            for (caminho, xml) in n.arquivos() {
                v.push((caminho, xml.as_bytes().to_vec()));
            }
        }
    }
    if let Ok(e) = entradas {
        for n in e {
            if let Some(xml) = &n.xml {
                let nome = n
                    .chave
                    .clone()
                    .unwrap_or_else(|| format!("serie{}-numero{}", n.serie, n.numero));
                v.push((format!("Entradas/{nome}-nfe.xml"), xml.as_bytes().to_vec()));
            }
        }
    }
    v
}

fn quantos_xmls(emitidas: &NotasEmitidas, entradas: &NotasDeEntrada) -> usize {
    let saidas = emitidas.as_ref().map_or(0, |e| {
        e.notas.iter().map(|n| n.arquivos().len()).sum::<usize>()
            + e.cartas.len()
            + e.nfse.iter().map(|n| n.arquivos().len()).sum::<usize>()
    });
    let de_entrada = entradas
        .as_ref()
        .map_or(0, |e| e.iter().filter(|n| n.xml.is_some()).count());
    saidas + de_entrada
}

/// O que precisa de atenção antes de mandar o mês: (o quê, o que fazer).
pub(super) fn pendencias(
    emitidas: &NotasEmitidas,
    entradas: &NotasDeEntrada,
) -> Vec<(String, String)> {
    let mut v = Vec::new();
    if let Ok(e) = emitidas {
        for d in &e.sem_desfecho {
            v.push((
                format!(
                    "{} série {} nº {} — {}",
                    nome_do_modelo(d.modelo),
                    d.serie,
                    d.numero,
                    d.estado.rotulo().to_lowercase()
                ),
                d.pedido.map_or_else(
                    || {
                        "Sem desfecho, o número é um buraco na série: autorize (corrija e \
                         reenvie) ou inutilize, em Notas fiscais."
                            .to_owned()
                    },
                    |p| {
                        let rotulo = p.rotulo();
                        let mut c = rotulo.chars();
                        let inicio: String = c
                            .next()
                            .map(|l| l.to_uppercase().collect())
                            .unwrap_or_default();
                        format!(
                            "{inicio}{} — transmita em Notas fiscais e gere o pacote de novo.",
                            c.as_str()
                        )
                    },
                ),
            ));
        }
        for (falta, n) in e.faltas() {
            v.push((
                format!("{n} × {}", falta.explicar()),
                falta.o_que_fazer().to_owned(),
            ));
        }
    }
    if let Ok(e) = entradas {
        let antigas = e
            .iter()
            .filter(|n| n.xml.is_none() && n.chave.is_some())
            .count();
        if antigas > 0 {
            v.push((
                format!(
                    "{antigas} nota(s) de entrada sem o XML guardado (importadas antes de o \
                     sistema guardar o XML)"
                ),
                "Importe o arquivo XML de novo em Compras: ele fica guardado, sem duplicar a nota."
                    .to_owned(),
            ));
        }
    }
    v
}

/// O resumo das notas do mês, para o PDF.
fn secao_das_notas(d: &DadosDoPacote<'_>, aaaamm: &str) -> SecaoRelatorio {
    let f = Dinheiro::formatar_com_simbolo;
    let mut linhas = Vec::new();
    let mut nota = format!(
        "Notas de produção, pela data de emissão (a inutilizada, pela da inutilização); as de \
         entrada, pela emissão — o dia em que a compra entra no Razão. Os XMLs estão em \
         «XMLs {aaaamm}.zip»; a lista nota a nota, na planilha."
    );
    match d.emitidas {
        Ok(e) => {
            if e.notas.is_empty() {
                linhas.push(linha(
                    vec!["Notas emitidas".into(), "0".into(), String::new()],
                    false,
                    0,
                ));
            }
            for modelo in [ModeloFiscal::Nfce, ModeloFiscal::Nfe] {
                let do_modelo: Vec<_> = e.notas.iter().filter(|n| n.modelo == modelo).collect();
                if do_modelo.is_empty() {
                    continue;
                }
                for situacao in [
                    SituacaoNoMes::Autorizada,
                    SituacaoNoMes::Cancelada,
                    SituacaoNoMes::Inutilizada,
                    SituacaoNoMes::Denegada,
                ] {
                    let estas: Vec<_> = do_modelo
                        .iter()
                        .filter(|n| n.situacao == situacao)
                        .collect();
                    let autorizada = situacao == SituacaoNoMes::Autorizada;
                    if estas.is_empty() && !autorizada {
                        continue;
                    }
                    let valor = if autorizada {
                        f(estas.iter().map(|n| n.total).sum())
                    } else {
                        String::new()
                    };
                    linhas.push(linha(
                        vec![
                            format!(
                                "{} {}s",
                                nome_do_modelo(modelo),
                                situacao.nome().to_lowercase()
                            ),
                            estas.len().to_string(),
                            valor,
                        ],
                        autorizada,
                        u8::from(!autorizada),
                    ));
                }
            }
            if !e.cartas.is_empty() {
                linhas.push(linha(
                    vec![
                        "Cartas de correção (NF-e)".into(),
                        e.cartas.len().to_string(),
                        String::new(),
                    ],
                    false,
                    0,
                ));
            }
            let (canceladas, autorizadas): (Vec<_>, Vec<_>) =
                e.nfse.iter().partition(|n| n.cancelada);
            if !autorizadas.is_empty() {
                linhas.push(linha(
                    vec![
                        "NFS-e autorizadas".into(),
                        autorizadas.len().to_string(),
                        f(autorizadas.iter().map(|n| n.total).sum()),
                    ],
                    true,
                    0,
                ));
            }
            if !canceladas.is_empty() {
                linhas.push(linha(
                    vec![
                        "NFS-e canceladas".into(),
                        canceladas.len().to_string(),
                        String::new(),
                    ],
                    false,
                    1,
                ));
            }
            if e.de_homologacao > 0 {
                nota.push_str(&format!(
                    " {} nota(s) de homologação (teste) ficaram de fora: não têm valor fiscal.",
                    e.de_homologacao
                ));
            }
        }
        Err(motivo) => {
            linhas.push(linha(
                vec![
                    "Notas emitidas — não incluídas".into(),
                    String::new(),
                    String::new(),
                ],
                false,
                0,
            ));
            nota.push_str(&format!(" Notas emitidas: {motivo}."));
        }
    }
    match d.entradas {
        Ok(e) => linhas.push(linha(
            vec![
                "Notas de entrada".into(),
                e.len().to_string(),
                f(e.iter().map(|n| n.valor_total).sum()),
            ],
            true,
            0,
        )),
        Err(motivo) => {
            linhas.push(linha(
                vec![
                    "Notas de entrada — não incluídas".into(),
                    String::new(),
                    String::new(),
                ],
                false,
                0,
            ));
            nota.push_str(&format!(" Notas de entrada: {motivo}."));
        }
    }
    linhas.push(linha(
        vec![
            "XMLs no pacote".into(),
            quantos_xmls(d.emitidas, d.entradas).to_string(),
            String::new(),
        ],
        true,
        0,
    ));
    SecaoRelatorio {
        titulo: "Notas fiscais do mês".to_owned(),
        nota: Some(nota),
        colunas: vec![
            coluna("Documento", 3.0, false),
            coluna("Quantidade", 1.0, true),
            coluna("Valor", 1.3, true),
        ],
        linhas,
    }
}

/// A aba das notas emitidas: as que fecharam no mês e as que ainda não tiveram desfecho.
fn aba_das_emitidas(e: &NotasDoMes) -> Option<AbaPlanilha> {
    if e.notas.is_empty() && e.sem_desfecho.is_empty() {
        return None;
    }
    let negrito = |t: &str| Celula::texto(t).negrito();
    let mut a = AbaPlanilha::nova("Notas emitidas")
        .colunas(&[12.0, 9.0, 7.0, 11.0, 22.0, 14.0, 48.0, 18.0, 18.0, 30.0])
        .congelar_cabecalho();
    a.linha(
        [
            "Data",
            "Modelo",
            "Série",
            "Número",
            "Situação",
            "Valor",
            "Chave de acesso",
            "Protocolo",
            "Protocolo do evento",
            "XML",
        ]
        .into_iter()
        .map(negrito)
        .collect(),
    );
    let numero = |n: u64| Celula::inteiro(i64::try_from(n).unwrap_or(i64::MAX));
    let texto = |t: &Option<String>| Celula::texto(t.clone().unwrap_or_default());
    for n in &e.notas {
        let faltas: Vec<&str> = n.faltas().iter().map(|f| f.curto()).collect();
        a.linha(vec![
            Celula::data(n.data),
            Celula::texto(nome_do_modelo(n.modelo)),
            Celula::inteiro(i64::from(n.serie)),
            numero(n.numero),
            Celula::texto(n.situacao.nome()),
            Celula::dinheiro(n.total),
            texto(&n.chave),
            texto(&n.protocolo),
            texto(&n.protocolo_evento),
            Celula::texto(if faltas.is_empty() {
                "no pacote".to_owned()
            } else {
                faltas.join("; ")
            }),
        ]);
    }
    for d in &e.sem_desfecho {
        a.linha(vec![
            Celula::data(d.criado_em.data(cardeal_kernel::Fuso::BRASILIA)),
            Celula::texto(nome_do_modelo(d.modelo)),
            Celula::inteiro(i64::from(d.serie)),
            numero(d.numero),
            Celula::texto(format!(
                "Sem desfecho ({})",
                d.estado.rotulo().to_lowercase()
            )),
            Celula::dinheiro(d.total),
            texto(&d.chave),
            Celula::vazia(),
            Celula::vazia(),
            Celula::texto("—"),
        ]);
    }
    Some(a)
}

/// A aba das notas de entrada do mês.
fn aba_das_entradas(e: &[NotaDeEntradaDoMes]) -> Option<AbaPlanilha> {
    if e.is_empty() {
        return None;
    }
    let negrito = |t: &str| Celula::texto(t).negrito();
    let mut a = AbaPlanilha::nova("Notas de entrada")
        .colunas(&[12.0, 11.0, 7.0, 40.0, 14.0, 13.0, 48.0, 30.0])
        .congelar_cabecalho();
    a.linha(
        [
            "Emissão",
            "Número",
            "Série",
            "Fornecedor",
            "Valor",
            "Situação",
            "Chave de acesso",
            "XML",
        ]
        .into_iter()
        .map(negrito)
        .collect(),
    );
    for n in e {
        a.linha(vec![
            Celula::data(n.data_emissao),
            Celula::texto(n.numero.clone()),
            Celula::texto(n.serie.clone()),
            Celula::texto(n.fornecedor.clone()),
            Celula::dinheiro(n.valor_total),
            Celula::texto(estado_da_entrada(n.estado)),
            Celula::texto(n.chave.clone().unwrap_or_default()),
            Celula::texto(xml_da_entrada(n)),
        ]);
    }
    Some(a)
}

fn coluna(titulo: &str, peso: f32, direita: bool) -> ColunaRelatorio {
    ColunaRelatorio {
        titulo: titulo.to_owned(),
        peso,
        direita,
    }
}

fn linha(celulas: Vec<String>, destaque: bool, recuo: u8) -> LinhaRelatorio {
    LinhaRelatorio {
        celulas,
        destaque,
        recuo,
    }
}

/// As linhas (rótulo, valor, destaque, recuo) de um fluxo de caixa.
fn linhas_do_fluxo(
    inicial: Dinheiro,
    blocos: &[BlocoDfc],
    variacao: Dinheiro,
    final_: Dinheiro,
) -> Vec<(String, Dinheiro, bool, u8)> {
    let mut v = vec![("Caixa no início do mês".to_owned(), inicial, true, 0)];
    for b in blocos {
        v.push((b.nome.clone(), b.total, true, 0));
        for l in &b.linhas {
            v.push((l.rotulo.clone(), l.valor, false, 1));
        }
    }
    v.push(("Variação do caixa".to_owned(), variacao, true, 0));
    v.push(("Caixa no fim do mês".to_owned(), final_, true, 0));
    v
}

/// O PDF do pacote.
pub(super) fn relatorio(d: &DadosDoPacote<'_>) -> RelatorioPdf {
    let f = Dinheiro::formatar_com_simbolo;
    let mut secoes = Vec::new();

    let dre = &d.dre.atual;
    secoes.push(SecaoRelatorio {
        titulo: "Demonstração do resultado (DRE)".to_owned(),
        nota: Some(
            "Pela competência de cada lançamento, com o mês anterior e o mesmo mês do ano \
             anterior. Receitas somam, custos e despesas subtraem."
                .to_owned(),
        ),
        colunas: vec![
            coluna("Linha", 3.0, false),
            coluna("Mês", 1.2, true),
            coluna("Mês anterior", 1.2, true),
            coluna("Ano anterior", 1.2, true),
        ],
        linhas: dre
            .linhas
            .iter()
            .map(|l| {
                let outro = |o: &mod_financeiro::dre::Dre| {
                    o.linhas
                        .iter()
                        .find(|x| x.rotulo == l.rotulo && x.codigo == l.codigo)
                        .map_or(Dinheiro::ZERO, |x| x.valor)
                };
                linha(
                    vec![
                        l.rotulo.clone(),
                        f(l.valor),
                        f(outro(&d.dre.mes_anterior)),
                        f(outro(&d.dre.mesmo_mes_ano_anterior)),
                    ],
                    l.tipo != TipoLinhaDre::Conta,
                    u8::from(l.tipo == TipoLinhaDre::Conta),
                )
            })
            .collect(),
    });
    for (titulo, nota, fluxo) in [
        (
            "Fluxo de caixa — método direto",
            "O que entrou e saiu do caixa e dos bancos, pela data do pagamento.",
            linhas_do_fluxo(
                d.direto.saldo_inicial,
                &d.direto.blocos,
                d.direto.variacao,
                d.direto.saldo_final,
            ),
        ),
        (
            "Fluxo de caixa — método indireto",
            "Do lucro líquido ao caixa, pelas variações das contas patrimoniais.",
            linhas_do_fluxo(
                d.indireto.saldo_inicial,
                &d.indireto.blocos,
                d.indireto.variacao,
                d.indireto.saldo_final,
            ),
        ),
    ] {
        secoes.push(SecaoRelatorio {
            titulo: titulo.to_owned(),
            nota: Some(nota.to_owned()),
            colunas: vec![coluna("Linha", 4.0, false), coluna("Valor", 1.4, true)],
            linhas: fluxo
                .into_iter()
                .map(|(r, v, destaque, recuo)| linha(vec![r, f(v)], destaque, recuo))
                .collect(),
        });
    }
    let dc = super::balancete::com_natureza;
    secoes.push(SecaoRelatorio {
        titulo: "Balancete de verificação".to_owned(),
        nota: Some(
            "Pela competência. Resultado acumulado no ano; o de anos anteriores, sem \
             encerramento no sistema, aparece no patrimônio líquido. D = devedor, C = credor. \
             O diário completo está na planilha."
                .to_owned(),
        ),
        colunas: vec![
            coluna("Conta", 3.2, false),
            coluna("Saldo anterior", 1.2, true),
            coluna("Débitos", 1.1, true),
            coluna("Créditos", 1.1, true),
            coluna("Saldo atual", 1.2, true),
        ],
        linhas: d
            .balancete
            .iter()
            .map(|l| {
                linha(
                    vec![
                        if l.codigo.is_empty() {
                            l.nome.clone()
                        } else {
                            format!("{}  {}", l.codigo, l.nome)
                        },
                        dc(l.saldo_anterior),
                        dc(l.debitos),
                        dc(-l.creditos),
                        dc(l.saldo_atual()),
                    ],
                    l.sintetica,
                    l.nivel.saturating_sub(1),
                )
            })
            .collect(),
    });
    let aaaamm = format!("{}-{:02}", d.mes.ano(), d.mes.mes());
    secoes.push(secao_das_notas(d, &aaaamm));
    let pendentes = pendencias(d.emitidas, d.entradas);
    if !pendentes.is_empty() {
        secoes.push(SecaoRelatorio {
            titulo: "Pendências do mês".to_owned(),
            nota: Some(
                "O que conferir antes de mandar o mês — o contador vai perguntar.".to_owned(),
            ),
            colunas: vec![
                coluna("O quê", 3.0, false),
                coluna("O que fazer", 3.0, false),
            ],
            linhas: pendentes
                .into_iter()
                .map(|(o_que, fazer)| linha(vec![o_que, fazer], false, 0))
                .collect(),
        });
    }
    RelatorioPdf {
        titulo: "Pacote do contador".to_owned(),
        subtitulo: titulo_do_mes(d.mes),
        secoes,
    }
}

fn titulo_do_mes(mes: Competencia) -> String {
    let t = mes.formatar_extenso();
    let mut c = t.chars();
    c.next().map_or_else(String::new, |p| {
        p.to_uppercase().collect::<String>() + c.as_str()
    })
}

/// A planilha do pacote: DRE, os dois fluxos, o balancete e o diário.
pub(super) fn planilha(d: &DadosDoPacote<'_>) -> Planilha {
    let negrito = |t: &str| Celula::texto(t).negrito();
    let mut dre = AbaPlanilha::nova("DRE")
        .colunas(&[46.0, 16.0, 16.0, 16.0])
        .congelar_cabecalho();
    dre.linha(vec![
        negrito("Linha"),
        negrito("Mês"),
        negrito("Mês anterior"),
        negrito("Ano anterior"),
    ]);
    for l in &d.dre.atual.linhas {
        let outro = |o: &mod_financeiro::dre::Dre| {
            o.linhas
                .iter()
                .find(|x| x.rotulo == l.rotulo && x.codigo == l.codigo)
                .map_or(Dinheiro::ZERO, |x| x.valor)
        };
        let destaque = l.tipo != TipoLinhaDre::Conta;
        let cel = |v: Dinheiro| {
            let c = Celula::dinheiro(v);
            if destaque {
                c.negrito()
            } else {
                c
            }
        };
        let rotulo = if destaque {
            negrito(&l.rotulo)
        } else {
            Celula::texto(format!("    {}", l.rotulo))
        };
        dre.linha(vec![
            rotulo,
            cel(l.valor),
            cel(outro(&d.dre.mes_anterior)),
            cel(outro(&d.dre.mesmo_mes_ano_anterior)),
        ]);
    }
    let fluxo = |nome: &str, linhas: Vec<(String, Dinheiro, bool, u8)>| {
        let mut a = AbaPlanilha::nova(nome)
            .colunas(&[56.0, 16.0])
            .congelar_cabecalho();
        a.linha(vec![negrito("Linha"), negrito("Valor")]);
        for (r, v, destaque, recuo) in linhas {
            let texto = format!("{}{r}", "    ".repeat(usize::from(recuo)));
            if destaque {
                a.linha(vec![negrito(&texto), Celula::dinheiro(v).negrito()]);
            } else {
                a.linha(vec![Celula::texto(texto), Celula::dinheiro(v)]);
            }
        }
        a
    };
    let direto = fluxo(
        "DFC direto",
        linhas_do_fluxo(
            d.direto.saldo_inicial,
            &d.direto.blocos,
            d.direto.variacao,
            d.direto.saldo_final,
        ),
    );
    let indireto = fluxo(
        "DFC indireto",
        linhas_do_fluxo(
            d.indireto.saldo_inicial,
            &d.indireto.blocos,
            d.indireto.variacao,
            d.indireto.saldo_final,
        ),
    );
    // Balancete com débito positivo e crédito negativo — o contador soma direto.
    let mut bal = AbaPlanilha::nova("Balancete")
        .colunas(&[12.0, 40.0, 16.0, 16.0, 16.0, 16.0])
        .congelar_cabecalho();
    bal.linha(vec![
        negrito("Código"),
        negrito("Conta"),
        negrito("Saldo anterior"),
        negrito("Débitos"),
        negrito("Créditos"),
        negrito("Saldo atual"),
    ]);
    for l in d.balancete {
        let cel = |c: Celula| if l.sintetica { c.negrito() } else { c };
        bal.linha(vec![
            cel(Celula::texto(l.codigo.clone())),
            cel(Celula::texto(l.nome.clone())),
            cel(Celula::dinheiro(l.saldo_anterior)),
            cel(Celula::dinheiro(l.debitos)),
            cel(Celula::dinheiro(l.creditos)),
            cel(Celula::dinheiro(l.saldo_atual())),
        ]);
    }
    let mut diario = AbaPlanilha::nova("Diário")
        .colunas(&[12.0, 8.0, 44.0, 12.0, 34.0, 15.0, 15.0])
        .congelar_cabecalho();
    diario.linha(vec![
        negrito("Data"),
        negrito("Nº"),
        negrito("Histórico"),
        negrito("Conta"),
        negrito("Nome da conta"),
        negrito("Débito"),
        negrito("Crédito"),
    ]);
    for l in d.diario {
        for p in &l.partidas {
            let (deb, cre) = if p.valor.e_positivo() {
                (Celula::dinheiro(p.valor), Celula::vazia())
            } else {
                (Celula::vazia(), Celula::dinheiro(-p.valor))
            };
            diario.linha(vec![
                Celula::data(l.data),
                Celula::inteiro(l.numero),
                Celula::texto(l.historico.clone()),
                Celula::texto(p.codigo.clone()),
                Celula::texto(p.nome.clone()),
                deb,
                cre,
            ]);
        }
    }
    let mut planilha = Planilha::nova()
        .aba(dre)
        .aba(direto)
        .aba(indireto)
        .aba(bal)
        .aba(diario);
    if let Some(a) = d.emitidas.as_ref().ok().and_then(aba_das_emitidas) {
        planilha = planilha.aba(a);
    }
    if let Some(a) = d.entradas.as_deref().ok().and_then(aba_das_entradas) {
        planilha = planilha.aba(a);
    }
    planilha
}

/// O que foi gravado.
pub(super) struct PacoteGravado {
    /// A pasta do mês.
    pub(super) pasta: PathBuf,
    /// Quantos XMLs foram no ZIP.
    pub(super) xmls: usize,
    /// Quantas notas estão na busca de XML na SEFAZ (a fila fiscal busca).
    pub(super) buscando: u32,
    /// Quantas pendências o PDF lista.
    pub(super) pendencias: usize,
    /// Se a EFD ICMS/IPI do mês foi junto.
    pub(super) efd: bool,
}

/// Grava o pacote do mês escolhido em `destino/Contador <AAAA-MM>/`.
///
/// # Errors
/// Mensagem para o usuário quando falta dado (o mês não carregou) ou a gravação falha.
pub(super) fn gravar(
    motor: &Motor,
    sessao: &Sessao,
    estado: &EstadoTelaFinanceiro,
    destino: &Path,
) -> Result<PacoteGravado, String> {
    let falta = || "Os relatórios do mês ainda não carregaram (ou falta permissão).".to_owned();
    let mes = estado.mes_dre();
    let diario: Vec<LancamentoDoDiario> = motor
        .consultar(
            sessao,
            "financeiro.diario_do_periodo.v1",
            &DiarioDoPeriodo {
                periodo: Periodo::novo(mes.primeiro_dia(), mes.ultimo_dia()),
            },
        )
        .map_err(|e| e.mensagem)?;
    // Antes de montar o mês, o que dá para recuperar na SEFAZ (o XML do cancelamento que não
    // veio, a nota confirmada pela consulta) entra na busca — a fila fiscal busca, e o pacote
    // gerado de novo já os leva. Sem a permissão de emitir, fica para quem tem.
    let buscando = if sessao.concede("fiscal.emitir") {
        motor
            .executar(
                sessao,
                "fiscal.buscar_xmls_que_faltam.v1",
                &BuscarXmlsQueFaltam,
            )
            .unwrap_or(0)
    } else {
        0
    };
    // As notas do mês: cada parte só com a permissão de vê-la; sem ela (ou com erro), o PDF diz
    // por que a parte ficou de fora, e o resto do pacote sai.
    let emitidas: NotasEmitidas = if sessao.concede("fiscal.ver") {
        motor
            .consultar(sessao, "fiscal.notas_do_mes.v1", &NotasFiscaisDoMes { mes })
            .map_err(|e| e.mensagem)
    } else {
        Err("o seu usuário não vê as notas fiscais".to_owned())
    };
    let entradas: NotasDeEntrada = if sessao.concede("compras.entrada.ver") {
        motor
            .consultar(
                sessao,
                "compras.notas_do_mes.v1",
                &NotasDeEntradaDoMes { mes },
            )
            .map_err(|e| e.mensagem)
    } else {
        Err("o seu usuário não vê as compras".to_owned())
    };
    let dados = DadosDoPacote {
        mes,
        dre: estado.dre.as_ref().ok_or_else(falta)?,
        direto: estado.dfc.as_ref().ok_or_else(falta)?,
        indireto: estado.dfc_indireto.as_ref().ok_or_else(falta)?,
        balancete: estado.balancete.as_deref().ok_or_else(falta)?,
        diario: &diario,
        emitidas: &emitidas,
        entradas: &entradas,
    };
    let ident = motor.identidade_visual().unwrap_or_default();
    let empresa = IdentidadeEmpresa {
        nome_fantasia: ident.nome_fantasia,
        razao_social: ident.razao_social,
        cnpj: ident.cnpj,
        endereco: ident.endereco,
        telefone: ident.telefone,
        email: ident.email,
        site: ident.site,
        logo_png: ident.logo_png,
    };
    let pdf = gerar_relatorio(
        &empresa,
        &relatorio(&dados),
        cardeal_kernel::Instante::agora(),
    )
    .map_err(|e| format!("Não consegui montar o PDF: {e}"))?;
    let xlsx = planilha(&dados).gerar();
    let aaaamm = format!("{}-{:02}", mes.ano(), mes.mes());
    let pasta = destino.join(format!("Contador {aaaamm}"));
    std::fs::create_dir_all(&pasta).map_err(|e| format!("Não consegui criar a pasta: {e}"))?;
    std::fs::write(pasta.join(format!("Relatórios {aaaamm}.pdf")), pdf)
        .map_err(|e| format!("Não consegui gravar o PDF: {e}"))?;
    std::fs::write(pasta.join(format!("Contabilidade {aaaamm}.xlsx")), xlsx)
        .map_err(|e| format!("Não consegui gravar a planilha: {e}"))?;
    let xmls = arquivos_xml(&emitidas, &entradas);
    if !xmls.is_empty() {
        let zip = cardeal_planilha::zip::empacotar_comprimido(&xmls)
            .map_err(|e| format!("Não consegui montar o ZIP dos XMLs: {e}"))?;
        std::fs::write(pasta.join(format!("XMLs {aaaamm}.zip")), zip)
            .map_err(|e| format!("Não consegui gravar os XMLs: {e}"))?;
    }
    let efd = gravar_efd(motor, sessao, mes, &pasta, &aaaamm)?;
    Ok(PacoteGravado {
        pasta,
        xmls: xmls.len(),
        buscando,
        pendencias: pendencias(&emitidas, &entradas).len(),
        efd,
    })
}

/// A EFD ICMS/IPI do mês na pasta do pacote, quando a empresa entrega (regime normal com o SPED
/// configurado), com os avisos ao lado; faltando algo, um arquivo diz o quê. No Simples, nada.
fn gravar_efd(
    motor: &Motor,
    sessao: &Sessao,
    mes: Competencia,
    pasta: &Path,
    aaaamm: &str,
) -> Result<bool, String> {
    if !sessao.concede("fiscal.ver") {
        return Ok(false);
    }
    let Ok(efd) = crate::sped::gerar(motor, sessao, mes, false, None) else {
        return Ok(false);
    };
    let escrever = |nome: String, conteudo: &[u8]| {
        std::fs::write(pasta.join(nome), conteudo)
            .map_err(|e| format!("Não consegui gravar a EFD: {e}"))
    };
    match &efd.arquivo {
        Some(a) => {
            escrever(a.nome.clone(), &a.em_latin1())?;
            if !a.avisos.is_empty() {
                escrever(
                    format!("EFD ICMS IPI {aaaamm} - avisos.txt"),
                    a.avisos.join("\r\n").as_bytes(),
                )?;
            }
            Ok(true)
        }
        None if crate::sped::dispensada(&efd) => Ok(false),
        None => {
            escrever(
                format!("EFD ICMS IPI {aaaamm} - não gerada.txt"),
                efd.impedimentos.join("\r\n").as_bytes(),
            )?;
            Ok(false)
        }
    }
}

/// O botão da aba Resultado: escolhe a pasta, grava e abre.
pub(super) fn botao(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &EstadoTelaFinanceiro,
) {
    if !ui
        .add(Botao::secundario("Pacote do contador…").pequeno())
        .clicked()
    {
        return;
    }
    let Some(destino) = rfd::FileDialog::new()
        .set_title("Onde gravar o pacote do contador")
        .pick_folder()
    else {
        return;
    };
    match gravar(motor, sessao, estado, &destino) {
        Ok(p) => {
            if p.buscando > 0 {
                crate::fiscal_fundo::disparar(ui.ctx(), motor, sessao);
            }
            let efd = if p.efd { " e a EFD ICMS/IPI" } else { "" };
            let aviso = match (p.xmls, p.pendencias) {
                (_, 0) if p.xmls > 0 => Notificacao::sucesso(format!(
                    "Pacote do contador gravado: PDF, planilha, {} XML(s) do mês{efd}",
                    p.xmls
                )),
                (0, 0) => Notificacao::sucesso(format!(
                    "Pacote do contador gravado: PDF e planilha do mês{efd}"
                )),
                (_, n) if p.buscando > 0 => Notificacao::aviso(format!(
                    "Pacote do contador gravado, com {n} pendência(s) — o Cardeal está buscando {} \
                     XML(s) na SEFAZ: gere o pacote de novo daqui a pouco",
                    p.buscando
                )),
                (_, n) => Notificacao::aviso(format!(
                    "Pacote do contador gravado, com {n} pendência(s) — veja o fim do PDF"
                )),
            };
            notificar(ui.ctx(), aviso);
            let _ = open::that(&p.pasta);
        }
        Err(e) => notificar(ui.ctx(), Notificacao::erro(e)),
    }
}
