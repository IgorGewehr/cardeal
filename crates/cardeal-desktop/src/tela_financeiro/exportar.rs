//! Exportar o relatório que está na tela: em **PDF** (com o cabeçalho e o logo da empresa,
//! `cardeal_pdf::gerar_relatorio`) e em **Excel** (`.xlsx` de verdade, `cardeal_planilha`: dinheiro
//! como número, data como data, cabeçalho congelado — não um CSV com outro nome). A mesma tabela
//! da tela, montada só quando o botão é clicado.

use std::path::Path;

use super::*;
use cardeal_pdf::{
    gerar_relatorio, ColunaRelatorio, IdentidadeEmpresa, LinhaRelatorio, RelatorioPdf,
    SecaoRelatorio,
};
use cardeal_planilha::{Aba as AbaPlanilha, Celula, Planilha};

/// O conteúdo de uma célula.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Valor {
    Texto(String),
    Dinheiro(Dinheiro),
    Data(Data),
    Vazio,
}

impl Valor {
    pub(super) fn texto(t: impl Into<String>) -> Self {
        Self::Texto(t.into())
    }

    fn em_texto(&self) -> String {
        match self {
            Self::Texto(t) => t.clone(),
            Self::Dinheiro(d) => d.formatar_com_simbolo(),
            Self::Data(d) => d.formatar(),
            Self::Vazio => String::new(),
        }
    }

    fn em_celula(&self, recuo: u8) -> Celula {
        match self {
            Self::Texto(t) if recuo > 0 => {
                Celula::texto(format!("{}{t}", "    ".repeat(usize::from(recuo))))
            }
            Self::Texto(t) => Celula::texto(t.clone()),
            Self::Dinheiro(d) => Celula::dinheiro(*d),
            Self::Data(d) => Celula::data(*d),
            Self::Vazio => Celula::vazia(),
        }
    }
}

/// Uma coluna: o título, o peso na largura e se é número (à direita).
#[derive(Debug, Clone)]
pub(super) struct Coluna {
    pub(super) titulo: String,
    pub(super) peso: f32,
    pub(super) numero: bool,
}

impl Coluna {
    pub(super) fn texto(titulo: &str, peso: f32) -> Self {
        Self {
            titulo: titulo.to_owned(),
            peso,
            numero: false,
        }
    }

    pub(super) fn numero(titulo: &str, peso: f32) -> Self {
        Self {
            titulo: titulo.to_owned(),
            peso,
            numero: true,
        }
    }
}

/// Uma linha: as células, em negrito (cabeçalho de grupo, total) e o recuo da primeira coluna.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Linha {
    pub(super) celulas: Vec<Valor>,
    pub(super) destaque: bool,
    pub(super) recuo: u8,
}

impl Linha {
    pub(super) const fn nova(celulas: Vec<Valor>) -> Self {
        Self {
            celulas,
            destaque: false,
            recuo: 0,
        }
    }

    pub(super) const fn destaque(mut self) -> Self {
        self.destaque = true;
        self
    }

    pub(super) const fn recuo(mut self, n: u8) -> Self {
        self.recuo = n;
        self
    }
}

/// Uma tabela (uma seção do PDF, uma aba da planilha).
#[derive(Debug, Clone)]
pub(super) struct Tabela {
    pub(super) titulo: String,
    pub(super) nota: Option<String>,
    pub(super) colunas: Vec<Coluna>,
    pub(super) linhas: Vec<Linha>,
}

/// O relatório: título, subtítulo (o período, os filtros) e as tabelas.
#[derive(Debug, Clone)]
pub(super) struct Relatorio {
    pub(super) titulo: String,
    pub(super) subtitulo: String,
    pub(super) tabelas: Vec<Tabela>,
}

impl Relatorio {
    /// Os bytes do PDF, com o cabeçalho da empresa.
    pub(super) fn pdf(&self, motor: &Motor) -> Result<Vec<u8>, String> {
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
        gerar_relatorio(&empresa, &self.em_pdf(), cardeal_kernel::Instante::agora())
            .map_err(|e| format!("Não consegui montar o PDF: {e}"))
    }

    fn em_pdf(&self) -> RelatorioPdf {
        RelatorioPdf {
            titulo: self.titulo.clone(),
            subtitulo: self.subtitulo.clone(),
            secoes: self
                .tabelas
                .iter()
                .map(|t| SecaoRelatorio {
                    titulo: t.titulo.clone(),
                    nota: t.nota.clone(),
                    colunas: t
                        .colunas
                        .iter()
                        .map(|c| ColunaRelatorio {
                            titulo: c.titulo.clone(),
                            peso: c.peso,
                            direita: c.numero,
                        })
                        .collect(),
                    linhas: t
                        .linhas
                        .iter()
                        .map(|l| LinhaRelatorio {
                            celulas: l.celulas.iter().map(Valor::em_texto).collect(),
                            destaque: l.destaque,
                            recuo: l.recuo,
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    /// Os bytes do `.xlsx`: uma aba por tabela, o cabeçalho em negrito e congelado.
    pub(super) fn xlsx(&self) -> Vec<u8> {
        let mut planilha = Planilha::nova();
        for t in &self.tabelas {
            let nome: String = t.titulo.chars().take(31).collect();
            let larguras: Vec<f32> = t.colunas.iter().map(|c| (c.peso * 9.0).max(10.0)).collect();
            let mut aba = AbaPlanilha::nova(&nome)
                .colunas(&larguras)
                .congelar_cabecalho();
            aba.linha(
                t.colunas
                    .iter()
                    .map(|c| Celula::texto(c.titulo.clone()).negrito())
                    .collect(),
            );
            for l in &t.linhas {
                aba.linha(
                    l.celulas
                        .iter()
                        .enumerate()
                        .map(|(i, v)| {
                            let c = v.em_celula(if i == 0 { l.recuo } else { 0 });
                            if l.destaque {
                                c.negrito()
                            } else {
                                c
                            }
                        })
                        .collect(),
                );
            }
            planilha = planilha.aba(aba);
        }
        planilha.gerar()
    }

    /// "Contas a pagar - 05-11-2026.pdf": o título e o subtítulo, sem o que o Windows recusa.
    pub(super) fn nome_de_arquivo(&self, extensao: &str) -> String {
        let bruto = format!("{} - {}", self.titulo, self.subtitulo);
        let limpo: String = bruto
            .chars()
            .map(|c| match c {
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
                c => c,
            })
            .collect();
        format!("{}.{extensao}", limpo.trim())
    }
}

/// Grava os bytes no arquivo escolhido e abre.
fn gravar_e_abrir(ui: &egui::Ui, destino: &Path, bytes: &[u8]) {
    match std::fs::write(destino, bytes) {
        Ok(()) => {
            notificar(
                ui.ctx(),
                Notificacao::sucesso(format!(
                    "Gravado: {}",
                    destino
                        .file_name()
                        .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
                )),
            );
            let _ = open::that(destino);
        }
        Err(e) => notificar(
            ui.ctx(),
            Notificacao::erro(format!("Não consegui gravar o arquivo: {e}")),
        ),
    }
}

/// Os botões "Exportar PDF" e "Exportar Excel". O relatório só é montado no clique; a pessoa
/// escolhe onde gravar e o arquivo abre em seguida.
pub(super) fn botoes(ui: &mut egui::Ui, motor: &Motor, montar: impl FnOnce() -> Relatorio) {
    let pdf = ui
        .add(Botao::secundario("Exportar PDF").pequeno())
        .clicked();
    let excel = ui
        .add(Botao::secundario("Exportar Excel").pequeno())
        .clicked();
    if !pdf && !excel {
        return;
    }
    let r = montar();
    let (extensao, filtro) = if pdf {
        ("pdf", "PDF")
    } else {
        ("xlsx", "Planilha do Excel")
    };
    let Some(destino) = rfd::FileDialog::new()
        .set_title("Onde gravar o relatório")
        .set_file_name(r.nome_de_arquivo(extensao))
        .add_filter(filtro, &[extensao])
        .save_file()
    else {
        return;
    };
    if pdf {
        match r.pdf(motor) {
            Ok(bytes) => gravar_e_abrir(ui, &destino, &bytes),
            Err(e) => notificar(ui.ctx(), Notificacao::erro(e)),
        }
    } else {
        gravar_e_abrir(ui, &destino, &r.xlsx());
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn exemplo() -> Relatorio {
        Relatorio {
            titulo: "Contas a pagar".into(),
            subtitulo: "05/11/2026".into(),
            tabelas: vec![Tabela {
                titulo: "Por vencimento".into(),
                nota: Some("Todo o em aberto.".into()),
                colunas: vec![
                    Coluna::texto("Fornecedor", 3.0),
                    Coluna::texto("Vencimento", 1.5),
                    Coluna::numero("Valor", 1.5),
                ],
                linhas: vec![
                    Linha::nova(vec![
                        Valor::texto("Vencidos"),
                        Valor::Vazio,
                        Valor::Dinheiro(Dinheiro::reais(300)),
                    ])
                    .destaque(),
                    Linha::nova(vec![
                        Valor::texto("Imobiliária"),
                        Valor::Data(Data::de_ymd(2026, 9, 10).unwrap()),
                        Valor::Dinheiro(Dinheiro::reais(300)),
                    ])
                    .recuo(1),
                ],
            }],
        }
    }

    #[test]
    fn o_relatorio_vira_pdf_e_planilha_e_o_nome_nao_tem_barra() {
        let r = exemplo();
        assert_eq!(r.nome_de_arquivo("pdf"), "Contas a pagar - 05-11-2026.pdf");
        let pdf = r.em_pdf();
        assert_eq!(pdf.secoes.len(), 1);
        assert_eq!(pdf.secoes[0].linhas[1].celulas[1], "10/09/2026");
        assert_eq!(pdf.secoes[0].linhas[0].celulas[2], "R$ 300,00");
        assert!(pdf.secoes[0].colunas[2].direita);
        let empresa = IdentidadeEmpresa::default();
        assert!(
            gerar_relatorio(&empresa, &pdf, cardeal_kernel::Instante::EPOCA)
                .unwrap()
                .starts_with(b"%PDF")
        );
        // O .xlsx é um ZIP (PK…) com a aba.
        let xlsx = r.xlsx();
        assert!(xlsx.starts_with(b"PK"));
    }
}
