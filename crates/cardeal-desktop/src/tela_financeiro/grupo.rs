//! A aba **Grupo** do Financeiro (ADR-0017, onda 8): os CNPJs do grupo lado a lado e somados —
//! receita, resultado e margem do período, caixa, o que há a receber e a pagar, e o score de
//! saúde de cada um —, com o ranking pelo resultado.
//!
//! Cada número sai da consulta **na própria empresa**, com as permissões que a pessoa tem lá
//! (`Motor::consultar_em`): quem não vê o DRE de uma filial vê "sem acesso" naquela linha, não o
//! número dela. A venda de uma empresa do grupo para outra entra nas duas pontas (a eliminação
//! intercompany ainda não existe) — o painel avisa.

use cardeal_cliente::{EmpresaDoGrupo, Motor, Sessao};
use cardeal_kernel::{Data, Dinheiro, Fuso, Percentual, Periodo};
use cardeal_ui::atoms::{Etiqueta, Rotulo, Tom, ValorDinheiro};
use cardeal_ui::molecules::{Abas, CartaoKpi};
use cardeal_ui::organisms::{ColunaGrade, FaixaKpi, Grade};
use cardeal_ui::tokens::Espaco;
use eframe::egui;
use mod_financeiro::{
    AgingAPagar, AgingAReceber, ContasDisponiveis, DreDoPeriodo, SaudeFinanceira,
};

/// O período do painel.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PeriodoGrupo {
    /// O mês corrente.
    #[default]
    EsteMes,
    /// O mês anterior.
    MesPassado,
    /// De janeiro até hoje.
    EsteAno,
}

impl PeriodoGrupo {
    fn periodo(self, hoje: Data) -> Periodo {
        let mes = hoje.competencia();
        match self {
            Self::EsteMes => Periodo::novo(mes.primeiro_dia(), mes.ultimo_dia()),
            Self::MesPassado => {
                let m = mes.anterior();
                Periodo::novo(m.primeiro_dia(), m.ultimo_dia())
            }
            Self::EsteAno => Periodo::novo(hoje.inicio_do_ano(), hoje),
        }
    }
}

/// Os números de uma empresa. `None`: sem acesso (ou a consulta falhou).
#[derive(Debug, Clone)]
pub struct LinhaGrupo {
    /// A empresa.
    pub empresa: EmpresaDoGrupo,
    /// Receita líquida do período.
    pub receita: Option<Dinheiro>,
    /// Lucro líquido do período.
    pub resultado: Option<Dinheiro>,
    /// Margem líquida.
    pub margem: Option<Percentual>,
    /// Caixa e bancos hoje.
    pub caixa: Option<Dinheiro>,
    /// A receber em aberto.
    pub a_receber: Option<Dinheiro>,
    /// A pagar em aberto.
    pub a_pagar: Option<Dinheiro>,
    /// O score de saúde (0 a 100).
    pub score: Option<u8>,
}

/// A soma de um campo nas linhas em que ele existe; `parcial` se faltou em alguma.
fn somar(linhas: &[LinhaGrupo], f: impl Fn(&LinhaGrupo) -> Option<Dinheiro>) -> (Dinheiro, bool) {
    let mut total = Dinheiro::ZERO;
    let mut parcial = false;
    for l in linhas {
        match f(l) {
            Some(v) => total += v,
            None => parcial = true,
        }
    }
    (total, parcial)
}

/// O estado da aba.
#[derive(Default)]
pub struct EstadoGrupo {
    /// Os CNPJs em que a pessoa entra.
    pub empresas: Vec<EmpresaDoGrupo>,
    periodo: PeriodoGrupo,
    linhas: Vec<LinhaGrupo>,
}

impl EstadoGrupo {
    /// Lê os CNPJs do grupo (a aba só aparece com mais de um).
    pub fn conhecer(&mut self, motor: &Motor, sessao: &Sessao) {
        self.empresas = motor.empresas_do_grupo(sessao).unwrap_or_default();
    }

    /// Há grupo (mais de um CNPJ)?
    pub fn ha_grupo(&self) -> bool {
        self.empresas.len() > 1
    }

    /// Consulta cada empresa.
    pub fn carregar(&mut self, motor: &Motor, sessao: &Sessao) {
        self.conhecer(motor, sessao);
        let periodo = self.periodo.periodo(Data::hoje(Fuso::BRASILIA));
        self.linhas = self
            .empresas
            .iter()
            .map(|e| linha(motor, sessao, e, periodo))
            .collect();
        // O ranking: o melhor resultado primeiro; quem não deu para ler vai para o fim.
        self.linhas
            .sort_by_key(|l| std::cmp::Reverse(l.resultado.map(Dinheiro::em_centavos)));
    }
}

fn linha(motor: &Motor, sessao: &Sessao, e: &EmpresaDoGrupo, periodo: Periodo) -> LinhaGrupo {
    let dre = motor
        .consultar_em(
            sessao,
            e.id,
            "financeiro.dre_do_periodo.v1",
            &DreDoPeriodo { periodo },
        )
        .ok();
    LinhaGrupo {
        empresa: e.clone(),
        receita: dre.as_ref().map(|d| d.receita_liquida),
        resultado: dre.as_ref().map(|d| d.lucro_liquido),
        margem: dre
            .as_ref()
            .and_then(mod_financeiro::dre::Dre::margem_liquida),
        caixa: motor
            .consultar_em(
                sessao,
                e.id,
                "financeiro.contas_disponiveis.v1",
                &ContasDisponiveis,
            )
            .ok()
            .map(|v| v.iter().map(|c| c.saldo).sum()),
        a_receber: motor
            .consultar_em(
                sessao,
                e.id,
                "financeiro.aging_a_receber.v1",
                &AgingAReceber,
            )
            .ok()
            .map(|a| a.total_aberto),
        a_pagar: motor
            .consultar_em(sessao, e.id, "financeiro.aging_a_pagar.v1", &AgingAPagar)
            .ok()
            .map(|a| a.total_aberto),
        score: motor
            .consultar_em(
                sessao,
                e.id,
                "financeiro.saude.v1",
                &SaudeFinanceira { giro_mensal: None },
            )
            .ok()
            .map(|s| s.score),
    }
}

fn valor(ui: &mut egui::Ui, v: Option<Dinheiro>) {
    match v {
        Some(v) => {
            ui.add(ValorDinheiro::novo(v));
        }
        None => {
            ui.add(Rotulo::apoio("sem acesso"));
        }
    }
}

/// Desenha a aba.
pub fn painel(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, e: &mut EstadoGrupo) {
    if let Some(p) = Abas::nova(&[
        (PeriodoGrupo::EsteMes, "Este mês"),
        (PeriodoGrupo::MesPassado, "Mês passado"),
        (PeriodoGrupo::EsteAno, "Este ano"),
    ])
    .selecionada(e.periodo)
    .id_salt("grupo-periodo")
    .mostrar(ui)
    {
        e.periodo = p;
        e.carregar(motor, sessao);
    }
    ui.add_space(Espaco::E12);
    let (receita, rp) = somar(&e.linhas, |l| l.receita);
    let (resultado, sp) = somar(&e.linhas, |l| l.resultado);
    let (caixa, cp) = somar(&e.linhas, |l| l.caixa);
    let (a_receber, arp) = somar(&e.linhas, |l| l.a_receber);
    let (a_pagar, app) = somar(&e.linhas, |l| l.a_pagar);
    let marca = |p: bool| if p { " (parcial)" } else { "" };
    FaixaKpi::nova(vec![
        CartaoKpi::contagem("Receita do grupo", receita.formatar_com_simbolo())
            .variacao(format!("{} CNPJs{}", e.linhas.len(), marca(rp)))
            .compacto()
            .tom(Tom::Info),
        CartaoKpi::contagem("Resultado do grupo", resultado.formatar_com_simbolo())
            .variacao(format!("lucro líquido{}", marca(sp)))
            .compacto()
            .tom(if resultado.e_negativo() {
                Tom::Negativo
            } else {
                Tom::Positivo
            }),
        CartaoKpi::contagem("Caixa e bancos", caixa.formatar_com_simbolo())
            .variacao(format!("hoje{}", marca(cp)))
            .compacto()
            .tom(Tom::Neutro),
        CartaoKpi::contagem(
            "A receber − a pagar",
            (a_receber - a_pagar).formatar_com_simbolo(),
        )
        .variacao(format!(
            "{} − {}{}",
            a_receber.formatar_com_simbolo(),
            a_pagar.formatar_com_simbolo(),
            marca(arp || app)
        ))
        .compacto()
        .tom(Tom::Neutro),
    ])
    .mostrar(ui);
    ui.add_space(Espaco::E12);
    let colunas = vec![
        ColunaGrade::nova("#").largura(36.0).numero(),
        ColunaGrade::nova("Empresa"),
        ColunaGrade::nova("Receita").largura(120.0).numero(),
        ColunaGrade::nova("Resultado").largura(120.0).numero(),
        ColunaGrade::nova("Margem").largura(76.0).numero(),
        ColunaGrade::nova("Caixa").largura(120.0).numero(),
        ColunaGrade::nova("A receber").largura(110.0).numero(),
        ColunaGrade::nova("A pagar").largura(110.0).numero(),
        ColunaGrade::nova("Saúde").largura(80.0),
    ];
    let atual = motor.empresa_da_sessao(sessao);
    Grade::nova(colunas)
        .id_salt("grupo")
        .vazio("Nenhum CNPJ do grupo para mostrar.")
        .mostrar(ui, e.linhas.len(), |i, row| {
            let l = &e.linhas[i];
            let emp = &l.empresa;
            row.col(|ui| {
                ui.add(Rotulo::interface(if l.resultado.is_some() {
                    format!("{}º", i + 1)
                } else {
                    "—".to_owned()
                }));
            });
            row.col(|ui| {
                ui.horizontal(|ui| {
                    ui.add(Rotulo::interface(emp.nome.clone()));
                    if emp.matriz {
                        ui.add(Etiqueta::neutra("matriz"));
                    }
                    if Some(emp.id) == atual {
                        ui.add(Etiqueta::info("você está aqui"));
                    }
                });
            });
            row.col(|ui| valor(ui, l.receita));
            row.col(|ui| valor(ui, l.resultado));
            row.col(|ui| {
                ui.add(Rotulo::interface(l.margem.map_or_else(
                    || "—".to_owned(),
                    |m| format!("{}%", m.formatar(1)),
                )));
            });
            row.col(|ui| valor(ui, l.caixa));
            row.col(|ui| valor(ui, l.a_receber));
            row.col(|ui| valor(ui, l.a_pagar));
            row.col(|ui| match l.score {
                Some(s) => {
                    let tom = if s >= 70 {
                        Tom::Positivo
                    } else if s >= 40 {
                        Tom::Atencao
                    } else {
                        Tom::Negativo
                    };
                    ui.add(Etiqueta::nova(s.to_string(), tom));
                }
                None => {
                    ui.add(Rotulo::apoio("—"));
                }
            });
        });
    ui.add_space(Espaco::E8);
    ui.add(
        Rotulo::apoio(
            "Cada linha usa as permissões que você tem naquele CNPJ. As vendas entre empresas do \
             grupo entram nas duas pontas — a eliminação das operações entre elas ainda não é \
             feita.",
        )
        .quebravel(),
    );
}

#[cfg(test)]
mod testes {
    use super::*;

    fn com_receita(receita: Option<Dinheiro>) -> LinhaGrupo {
        LinhaGrupo {
            empresa: EmpresaDoGrupo {
                id: cardeal_kernel::Id::novo(),
                nome: "Loja".into(),
                cnpj: None,
                matriz: false,
            },
            receita,
            resultado: None,
            margem: None,
            caixa: None,
            a_receber: None,
            a_pagar: None,
            score: None,
        }
    }

    #[test]
    fn a_soma_avisa_quando_falta_uma_empresa() {
        let linhas = vec![com_receita(Some(Dinheiro::reais(100))), com_receita(None)];
        assert_eq!(somar(&linhas, |l| l.receita), (Dinheiro::reais(100), true));
        assert_eq!(
            somar(&linhas[..1], |l| l.receita),
            (Dinheiro::reais(100), false)
        );
    }

    #[test]
    fn o_grupo_ganha_um_cnpj_troca_de_empresa_e_soma_os_dois() {
        let mut t = crate::testes_comum::motor_de_teste();
        assert_eq!(t.motor.empresas_do_grupo(&t.sessao).unwrap().len(), 1);
        let matriz = t.motor.empresa_da_sessao(&t.sessao).unwrap();
        assert!(
            t.motor
                .adicionar_empresa(&t.sessao, "Filial Centro LTDA", "45543915000180")
                .is_err(),
            "CNPJ com dígito errado"
        );
        let filial = t
            .motor
            .adicionar_empresa(&t.sessao, "Filial Centro LTDA", "45.543.915/0001-81")
            .unwrap();
        let empresas = t.motor.empresas_do_grupo(&t.sessao).unwrap();
        assert_eq!(empresas.len(), 2);
        assert!(empresas[0].matriz && empresas[0].id == matriz);

        // O painel consulta cada empresa e soma.
        let mut e = EstadoGrupo::default();
        e.carregar(&t.motor, &t.sessao);
        assert!(e.ha_grupo());
        assert_eq!(e.linhas.len(), 2);
        assert!(e
            .linhas
            .iter()
            .all(|l| l.receita.is_some() && l.caixa.is_some()));
        let ctx = egui::Context::default();
        cardeal_ui::tokens::instalar_fontes(&ctx);
        cardeal_ui::tokens::instalar_estilo(&ctx, cardeal_ui::tokens::Tema::Claro);
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                painel(ui, &t.motor, &t.sessao, &mut e);
            });
        });

        // Trocar de empresa: a sessão e os dados cadastrais passam a ser os da filial.
        let na_filial = t.motor.trocar_empresa(&t.sessao, filial).unwrap();
        assert_eq!(t.motor.empresa_da_sessao(&na_filial), Some(filial));
        assert_eq!(
            t.motor.empresa_resumo().unwrap().razao_social,
            "Filial Centro LTDA"
        );
        let de_volta = t.motor.trocar_empresa(&na_filial, matriz).unwrap();
        assert_eq!(t.motor.empresa_da_sessao(&de_volta), Some(matriz));
        assert_ne!(
            t.motor.empresa_resumo().unwrap().razao_social,
            "Filial Centro LTDA"
        );
    }

    #[test]
    fn os_periodos() {
        let hoje = Data::de_ymd(2026, 3, 15).unwrap();
        let p = PeriodoGrupo::MesPassado.periodo(hoje);
        assert_eq!(
            p,
            Periodo::novo(
                Data::de_ymd(2026, 2, 1).unwrap(),
                Data::de_ymd(2026, 2, 28).unwrap()
            )
        );
        let a = PeriodoGrupo::EsteAno.periodo(hoje);
        assert_eq!(a, Periodo::novo(Data::de_ymd(2026, 1, 1).unwrap(), hoje));
    }
}
