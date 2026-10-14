//! O pacote do contador: o mês escolhido na aba Resultado e caixa num PDF (DRE comparativa, DFC
//! direto e indireto, balancete) e numa planilha `.xlsx` (as mesmas tabelas e o diário completo,
//! com números de verdade para o contador trabalhar), gravados numa pasta que o sistema abre em
//! seguida — pronta para anexar no e-mail ou no WhatsApp.

use std::path::{Path, PathBuf};

use super::*;
use cardeal_ledger::{LancamentoDoDiario, LinhaBalancete};
use cardeal_pdf::{
    gerar_relatorio, ColunaRelatorio, IdentidadeEmpresa, LinhaRelatorio, RelatorioPdf,
    SecaoRelatorio,
};
use cardeal_planilha::{Aba as AbaPlanilha, Celula, Planilha};
use mod_financeiro::dfc::{BlocoDfc, Dfc, DfcIndireto};
use mod_financeiro::dre::TipoLinhaDre;
use mod_financeiro::{ComparativoDre, DiarioDoPeriodo};

/// O que entra no pacote de um mês.
pub(super) struct DadosDoPacote<'a> {
    pub(super) mes: Competencia,
    pub(super) dre: &'a ComparativoDre,
    pub(super) direto: &'a Dfc,
    pub(super) indireto: &'a DfcIndireto,
    pub(super) balancete: &'a [LinhaBalancete],
    pub(super) diario: &'a [LancamentoDoDiario],
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
    Planilha::nova()
        .aba(dre)
        .aba(direto)
        .aba(indireto)
        .aba(bal)
        .aba(diario)
}

/// Grava o pacote do mês escolhido em `destino/Contador <AAAA-MM>/` e devolve a pasta.
///
/// # Errors
/// Mensagem para o usuário quando falta dado (o mês não carregou) ou a gravação falha.
pub(super) fn gravar(
    motor: &Motor,
    sessao: &Sessao,
    estado: &EstadoTelaFinanceiro,
    destino: &Path,
) -> Result<PathBuf, String> {
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
    let dados = DadosDoPacote {
        mes,
        dre: estado.dre.as_ref().ok_or_else(falta)?,
        direto: estado.dfc.as_ref().ok_or_else(falta)?,
        indireto: estado.dfc_indireto.as_ref().ok_or_else(falta)?,
        balancete: estado.balancete.as_deref().ok_or_else(falta)?,
        diario: &diario,
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
    Ok(pasta)
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
        Ok(pasta) => {
            notificar(
                ui.ctx(),
                Notificacao::sucesso("Pacote do contador gravado: PDF e planilha do mês"),
            );
            let _ = open::that(&pasta);
        }
        Err(e) => notificar(ui.ctx(), Notificacao::erro(e)),
    }
}
