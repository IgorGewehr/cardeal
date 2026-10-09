//! A aba "Cobrança": quem deve, quanto e há quanto tempo — o aging dos recebíveis (e, na outra
//! chave, o do que a empresa deve). Cada devedor tem o WhatsApp a um clique com a mensagem de
//! cobrança pronta.

use super::*;
use cardeal_ui::organisms::ItemBarraHorizontal;
use mod_financeiro::aging::{Aging, Devedor};
use mod_financeiro::{AgingAPagar, AgingAReceber};

impl EstadoTelaFinanceiro {
    pub(super) fn carregar_cobranca(&mut self, motor: &Motor, sessao: &Sessao) {
        let r = if self.cobranca_pagar {
            motor.consultar(sessao, "financeiro.aging_a_pagar.v1", &AgingAPagar)
        } else {
            motor.consultar(sessao, "financeiro.aging_a_receber.v1", &AgingAReceber)
        };
        self.aging = self.anotar(r);
    }

    pub(super) fn nome_devedor(&self, d: &Devedor) -> String {
        d.contraparte.map_or_else(
            || "Sem pessoa informada".to_owned(),
            |c| {
                self.nomes
                    .get(&c.id())
                    .cloned()
                    .unwrap_or_else(|| "Pessoa".to_owned())
            },
        )
    }

    fn telefone_do_devedor(&self, d: &Devedor) -> Option<String> {
        let id = d.contraparte?.id();
        self.clientes
            .iter()
            .chain(self.fornecedores.iter())
            .find(|p| p.pessoa == id)
            .and_then(|p| p.telefone.clone())
    }
}

pub(super) fn mensagem(nome: &str, d: &Devedor) -> String {
    let primeiro = nome.split_whitespace().next().unwrap_or(nome);
    format!(
        "Olá, {primeiro}! Tudo bem? Passando para lembrar que consta em aberto conosco o valor de \
         {}, vencido há {} dia(s). Podemos combinar o pagamento? Obrigado!",
        d.vencido.formatar_com_simbolo(),
        d.dias_do_mais_antigo
    )
}

pub(super) fn painel_cobranca(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
) {
    if let Some(p) = Abas::nova(&[(false, "Quem me deve"), (true, "A quem eu devo")])
        .selecionada(estado.cobranca_pagar)
        .id_salt("cobranca-especie")
        .mostrar(ui)
    {
        estado.cobranca_pagar = p;
        estado.carregar_cobranca(motor, sessao);
    }
    ui.add_space(Espaco::E12);

    let Some(aging) = estado.aging.clone() else {
        EstadoVazio::novo(Icone::Dinheiro, "Sem dados para apurar o atraso.").mostrar(ui);
        return;
    };
    let pagar = estado.cobranca_pagar;
    kpis(ui, &aging, pagar);
    ui.add_space(Espaco::E16);
    if aging.total_aberto.e_zero() {
        EstadoVazio::novo(
            Icone::Dinheiro,
            if pagar {
                "Nenhuma conta a pagar em aberto."
            } else {
                "Nada a receber em aberto."
            },
        )
        .mostrar(ui);
        return;
    }

    ui.add(Rotulo::titulo_secao("Por tempo de atraso"));
    ui.add_space(Espaco::E8);
    let barras: Vec<ItemBarraHorizontal> = aging
        .faixas
        .iter()
        .map(|f| ItemBarraHorizontal {
            rotulo: f.rotulo.clone(),
            valor: reais_para_grafico(f.valor),
        })
        .collect();
    Painel::novo().plano().mostrar(ui, |ui| {
        GraficoBarrasHorizontais::novo(&barras).mostrar(ui);
    });
    ui.add_space(Espaco::E16);
    devedores(ui, estado, &aging);
}

fn kpis(ui: &mut egui::Ui, a: &Aging, pagar: bool) {
    let taxa = a
        .taxa_de_inadimplencia()
        .map_or_else(|| "—".to_owned(), |p| format!("{}%", p.formatar(1)));
    let tom = if a.vencido_mais_de_30.e_positivo() {
        Tom::Negativo
    } else {
        Tom::Positivo
    };
    FaixaKpi::nova(vec![
        CartaoKpi::novo(
            if pagar {
                "Total a pagar"
            } else {
                "Total a receber"
            },
            a.total_aberto,
        )
        .tom(Tom::Neutro),
        CartaoKpi::novo("Já vencido", a.total_vencido).tom(if a.total_vencido.e_positivo() {
            Tom::Atencao
        } else {
            Tom::Positivo
        }),
        CartaoKpi::novo("Vencido há mais de 30 dias", a.vencido_mais_de_30).tom(tom),
        CartaoKpi::contagem("Inadimplência", taxa)
            .tom(tom)
            .variacao("vencido há +30 dias ÷ total em aberto"),
    ])
    .mostrar(ui);
}

fn devedores(ui: &mut egui::Ui, estado: &EstadoTelaFinanceiro, a: &Aging) {
    ui.add(Rotulo::titulo_secao(if estado.cobranca_pagar {
        "Quem tem contas vencidas conosco"
    } else {
        "Quem está devendo"
    }));
    ui.add_space(Espaco::E8);
    if a.devedores.is_empty() {
        EstadoVazio::novo(Icone::Dinheiro, "Nada vencido. Tudo em dia.").mostrar(ui);
        return;
    }
    let mut abrir: Option<String> = None;
    Grade::nova(vec![
        ColunaGrade::nova("Nome").largura(240.0),
        ColunaGrade::nova("Vencido").largura(130.0).numero(),
        ColunaGrade::nova("A vencer").largura(130.0).numero(),
        ColunaGrade::nova("Atraso").largura(110.0).numero(),
        ColunaGrade::nova("Parcelas").largura(90.0).numero(),
        ColunaGrade::nova("Cobrar").largura(140.0),
    ])
    .com_acoes()
    .id_salt("aging-devedores")
    .mostrar(ui, a.devedores.len(), |i, row| {
        let d = &a.devedores[i];
        let nome = estado.nome_devedor(d);
        row.col(|ui| {
            ui.add(Rotulo::interface(nome.clone()));
        });
        row.col(|ui| {
            ui.add(ValorDinheiro::novo(d.vencido).neutro());
        });
        row.col(|ui| {
            if d.a_vencer.e_zero() {
                ui.add(Rotulo::interface("—").cor(ui.cores().texto_fraco));
            } else {
                ui.add(ValorDinheiro::novo(d.a_vencer).neutro());
            }
        });
        row.col(|ui| {
            let etiqueta = if d.dias_do_mais_antigo > 30 {
                Etiqueta::negativa(format!("{} dias", d.dias_do_mais_antigo))
            } else {
                Etiqueta::atencao(format!("{} dias", d.dias_do_mais_antigo))
            };
            ui.add(etiqueta);
        });
        row.col(|ui| {
            ui.add(Rotulo::interface(d.parcelas_vencidas.to_string()));
        });
        row.col(|ui| {
            if estado.cobranca_pagar {
                return;
            }
            match estado.telefone_do_devedor(d) {
                Some(tel) => {
                    if let Some(link) = crate::telefone::link_whatsapp(&tel, &mensagem(&nome, d)) {
                        if ui.add(Botao::secundario("WhatsApp").pequeno()).clicked() {
                            abrir = Some(link);
                        }
                    } else {
                        ui.add(Rotulo::apoio("telefone inválido"));
                    }
                }
                None => {
                    ui.add(Rotulo::apoio("sem telefone"));
                }
            }
        });
    });
    if let Some(link) = abrir {
        if let Err(e) = open::that_detached(&link) {
            notificar(
                ui.ctx(),
                Notificacao::erro("Não foi possível abrir o WhatsApp").detalhe(e.to_string()),
            );
        }
    }
}
