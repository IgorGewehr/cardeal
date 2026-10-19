//! "Custo-hora da equipe…": quanto custa uma hora de cada técnico (os usuários) e o padrão da
//! oficina para quem não tem o seu (`mod_os::custo_hora`). É o que põe a mão de obra nas
//! margens da OS — a prevista (horas orçadas) e a real (horas apontadas).

use std::collections::HashMap;

use super::*;
use mod_os::custo_hora::TabelaCustoHora;
use mod_os::{CustosHora, DefinirCustoHora};

/// O estado do diálogo: o que está gravado e o que está digitado.
#[derive(Default)]
pub(super) struct EstadoCustoHora {
    gravado: TabelaCustoHora,
    pub(super) padrao: String,
    pub(super) tecnicos: HashMap<Id, String>,
}

fn texto(v: Option<Dinheiro>) -> String {
    v.map(|d| d.formatar()).unwrap_or_default()
}

impl EstadoCustoHora {
    pub(super) fn carregar(&mut self, motor: &Motor, sessao: &Sessao, usuarios: &[UsuarioResumo]) {
        self.gravado = motor
            .consultar(sessao, "os.custos_hora.v1", &CustosHora)
            .unwrap_or_default();
        self.padrao = texto(self.gravado.padrao);
        self.tecnicos = usuarios
            .iter()
            .map(|u| {
                let proprio = self
                    .gravado
                    .tecnicos
                    .iter()
                    .find(|(t, _)| *t == u.id)
                    .map(|(_, c)| *c);
                (u.id, texto(proprio))
            })
            .collect();
    }
}

/// O valor digitado: vazio = sem (volta a herdar); senão um valor maior que zero.
fn ler(rotulo: &str, texto: &str) -> Result<Option<Dinheiro>, String> {
    match texto.trim() {
        "" => Ok(None),
        t => Dinheiro::de_str(t)
            .ok()
            .filter(|d| d.e_positivo())
            .map(Some)
            .ok_or_else(|| format!("{rotulo}: digite o valor da hora, como 35,00")),
    }
}

/// Grava o que mudou; devolve quantos valores foram alterados.
pub(super) fn salvar(
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaOs,
) -> Result<u32, String> {
    let c = &estado.custo_hora;
    let mut mudancas: Vec<DefinirCustoHora> = Vec::new();
    let padrao = ler("Padrão da oficina", &c.padrao)?;
    if padrao != c.gravado.padrao {
        mudancas.push(DefinirCustoHora {
            tecnico: None,
            custo_hora: padrao,
        });
    }
    for u in &estado.usuarios {
        let digitado = ler(&u.nome, c.tecnicos.get(&u.id).map_or("", String::as_str))?;
        let gravado = c
            .gravado
            .tecnicos
            .iter()
            .find(|(t, _)| *t == u.id)
            .map(|(_, v)| *v);
        if digitado != gravado {
            mudancas.push(DefinirCustoHora {
                tecnico: Some(u.id),
                custo_hora: digitado,
            });
        }
    }
    for m in &mudancas {
        motor
            .executar(sessao, "os.definir_custo_hora.v1", m)
            .map_err(|e| e.mensagem)?;
    }
    let usuarios = estado.usuarios.clone();
    estado.custo_hora.carregar(motor, sessao, &usuarios);
    Ok(u32::try_from(mudancas.len()).unwrap_or(u32::MAX))
}

pub(super) fn dialogo(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaOs,
) {
    let fechar = Dialogo::nova("Custo-hora da equipe")
        .largura(560.0)
        .mostrar(
            ctx,
            estado,
            |ui, estado| {
                ui.add(
                    Rotulo::apoio(
                        "Quanto custa uma hora de cada técnico (salário, encargos e benefícios \
                         divididos pelas horas produtivas do mês). Com ele, a mão de obra entra no \
                         custo da OS: a margem prevista usa as horas orçadas, a real usa as horas \
                         apontadas. Em branco, o técnico usa o padrão da oficina.",
                    )
                    .quebravel(),
                );
                ui.add_space(Espaco::E12);
                ui.add(
                    Campo::novo("Padrão da oficina (R$/hora)", &mut estado.custo_hora.padrao)
                        .marcador("ex.: 30,00"),
                );
                ui.add_space(Espaco::E12);
                ui.add(Rotulo::titulo_secao("Por técnico"));
                ui.add_space(Espaco::E4);
                for u in estado.usuarios.clone() {
                    let valor = estado.custo_hora.tecnicos.entry(u.id).or_default();
                    ui.add(Campo::novo(format!("{} (R$/hora)", u.nome), valor).marcador("padrão"));
                    ui.add_space(Espaco::E4);
                }
            },
            |ui, estado| {
                if ui.add(Botao::primario("Salvar")).clicked() {
                    match salvar(motor, sessao, estado) {
                        Ok(n) => {
                            notificar(
                                ui.ctx(),
                                Notificacao::sucesso(if n == 0 {
                                    "Nada mudou".to_owned()
                                } else {
                                    format!("Custo-hora salvo ({n} alteração(ões))")
                                }),
                            );
                            estado.dlg = Dlg::Fechado;
                        }
                        Err(e) => notificar(ui.ctx(), Notificacao::erro(e)),
                    }
                }
                if ui.add(Botao::secundario("Cancelar")).clicked() {
                    estado.dlg = Dlg::Fechado;
                }
            },
        );
    if fechar {
        estado.dlg = Dlg::Fechado;
    }
}
