//! Configurações › Usuários e acesso › Teto de desconto: quanto cada papel que vende pode dar
//! de desconto numa venda (itens e total somados), no PDV e nas vendas.
//!
//! O padrão vem de fábrica (Administrador e Gerente sem teto; Operador de Caixa e Vendedor até
//! 10%) e o dono ajusta aqui (`empresa.definir_teto_desconto.v1`). Vale a partir da próxima
//! vez que a pessoa entrar — a sessão leva os limites do papel desde o login.

use cardeal_cliente::{Motor, Sessao};
use cardeal_kernel::Percentual;
use cardeal_ui::atoms::{Botao, Caixa, CampoTexto, Rotulo};
use cardeal_ui::organisms::{notificar, Notificacao};
use cardeal_ui::tokens::{Espaco, TemaUi};
use eframe::egui;
use mod_empresa::{DefinirTetoDeDesconto, TetoDeDesconto, TetosDeDesconto};

/// Um papel na lista, com o que está sendo digitado.
pub(super) struct LinhaTeto {
    gravado: TetoDeDesconto,
    /// O percentual, sem o `%`.
    texto: String,
    sem_teto: bool,
}

impl LinhaTeto {
    fn de(gravado: TetoDeDesconto) -> Self {
        Self {
            texto: gravado.teto.map(sem_zeros).unwrap_or_default(),
            sem_teto: gravado.teto.is_none(),
            gravado,
        }
    }

    /// O teto digitado (`None` = sem teto), ou por que não vale.
    fn lido(&self) -> Result<Option<Percentual>, &'static str> {
        if self.sem_teto {
            return Ok(None);
        }
        let t = self.texto.trim();
        let t = t.strip_suffix('%').unwrap_or(t).trim();
        if t.is_empty() {
            return Err("informe o teto (0 = não dá desconto) ou marque \"Sem teto\"");
        }
        let p = t
            .parse::<Percentual>()
            .map_err(|_| "percentual inválido (ex.: 10 ou 7,5)")?;
        if p.e_fracao_valida() {
            Ok(Some(p))
        } else {
            Err("o teto vai de 0% a 100%")
        }
    }

    fn mudou(&self) -> bool {
        self.lido().is_ok_and(|t| t != self.gravado.teto)
    }
}

/// Os tetos gravados, prontos para editar (vazio sem a permissão de ver usuários).
pub(super) fn carregar(motor: &Motor, sessao: &Sessao) -> Vec<LinhaTeto> {
    let tetos: Result<Vec<TetoDeDesconto>, _> =
        motor.consultar(sessao, "empresa.tetos_desconto.v1", &TetosDeDesconto);
    tetos
        .map(|l| l.into_iter().map(LinhaTeto::de).collect())
        .unwrap_or_default()
}

/// Grava os tetos que mudaram e devolve quantos; para no primeiro erro (os anteriores ficam).
pub(super) fn salvar(
    motor: &Motor,
    sessao: &Sessao,
    linhas: &[LinhaTeto],
) -> Result<usize, String> {
    let mut gravados = 0;
    for l in linhas.iter().filter(|l| l.mudou()) {
        let teto = l.lido().map_err(str::to_owned)?;
        let r: Result<(), _> = motor.executar(
            sessao,
            "empresa.definir_teto_desconto.v1",
            &DefinirTetoDeDesconto {
                papel: l.gravado.papel,
                teto,
            },
        );
        r.map_err(|e| format!("{}: {}", l.gravado.papel.nome(), e.mensagem))?;
        gravados += 1;
    }
    Ok(gravados)
}

/// `10` em vez de `10,00`; `7,5` em vez de `7,50`.
fn sem_zeros(p: Percentual) -> String {
    let t = p.formatar(2);
    match t.split_once(',') {
        Some((inteiro, casas)) => match casas.trim_end_matches('0') {
            "" => inteiro.to_owned(),
            casas => format!("{inteiro},{casas}"),
        },
        None => t,
    }
}

fn quem_tem(usuarios: usize) -> String {
    match usuarios {
        0 => "ninguém ainda".to_owned(),
        1 => "1 usuário".to_owned(),
        n => format!("{n} usuários"),
    }
}

/// O cartão. Devolve `true` quando gravou (para a tela recarregar).
pub(super) fn secao(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    linhas: &mut [LinhaTeto],
) -> bool {
    ui.add(Rotulo::titulo_secao("Teto de desconto"));
    ui.add_space(Espaco::E8);
    ui.add(
        Rotulo::apoio(
            "Quanto cada papel pode dar de desconto numa venda — itens e total somados, no PDV \
             e nas vendas. Vale a partir da próxima vez que a pessoa entrar.",
        )
        .quebravel(),
    );
    ui.add_space(Espaco::E12);
    if linhas.is_empty() {
        ui.add(Rotulo::campo("Sem acesso aos papéis da empresa."));
        return false;
    }
    let pode = sessao.concede("empresa.usuarios.gerenciar");
    let mut erro = None;
    for l in linhas.iter_mut() {
        ui.horizontal(|ui| {
            ui.scope(|ui| {
                ui.set_width(170.0);
                ui.add(Rotulo::interface(l.gravado.papel.nome()));
            });
            ui.scope(|ui| {
                ui.set_width(120.0);
                ui.add(Rotulo::campo(quem_tem(l.gravado.usuarios)));
            });
            if !pode {
                let t = l
                    .gravado
                    .teto
                    .map_or_else(|| "sem teto".to_owned(), Percentual::formatar_com_simbolo);
                ui.add(Rotulo::interface(t));
                return;
            }
            ui.scope(|ui| {
                ui.set_width(90.0);
                if l.sem_teto {
                    ui.add(Rotulo::campo("—"));
                } else {
                    ui.add(CampoTexto::novo(&mut l.texto).marcador("10"));
                }
            });
            ui.add(Rotulo::interface("%"));
            ui.add_space(Espaco::E12);
            ui.add(Caixa::nova(&mut l.sem_teto, "Sem teto"));
        });
        if let Err(e) = l.lido() {
            erro.get_or_insert(format!("{}: {e}", l.gravado.papel.nome()));
        }
        ui.add_space(Espaco::E4);
    }
    if !pode {
        ui.add_space(Espaco::E8);
        ui.add(Rotulo::apoio("Só quem gerencia os usuários ajusta o teto."));
        return false;
    }
    if let Some(e) = &erro {
        ui.add_space(Espaco::E4);
        ui.add(
            Rotulo::interface(e.clone())
                .quebravel()
                .cor(ui.cores().negativo),
        );
    }
    ui.add_space(Espaco::E12);
    let mudou = linhas.iter().any(LinhaTeto::mudou);
    let salvar_ok = mudou && erro.is_none();
    if ui
        .add(Botao::primario("Salvar tetos").habilitado(salvar_ok))
        .clicked()
    {
        return match salvar(motor, sessao, linhas) {
            Ok(n) => {
                let msg = if n == 1 {
                    "Teto de desconto salvo".to_owned()
                } else {
                    format!("{n} tetos de desconto salvos")
                };
                notificar(ui.ctx(), Notificacao::sucesso(msg));
                true
            }
            Err(e) => {
                notificar(ui.ctx(), Notificacao::erro(e));
                true
            }
        };
    }
    false
}

#[cfg(test)]
mod testes {
    use super::*;
    use mod_empresa::PapelDeFabrica;

    fn linha(papel: PapelDeFabrica, teto: Option<Percentual>) -> LinhaTeto {
        LinhaTeto::de(TetoDeDesconto {
            papel,
            teto,
            usuarios: 0,
        })
    }

    #[test]
    fn le_o_teto_digitado() {
        let mut l = linha(PapelDeFabrica::Vendedor, Some(Percentual::pontos(10)));
        assert_eq!(l.texto, "10");
        assert_eq!(sem_zeros(Percentual::milesimos(7_500)), "7,5");
        assert!(!l.mudou());
        for (texto, esperado) in [
            ("15", Ok(Some(Percentual::pontos(15)))),
            ("7,5%", Ok(Some(Percentual::milesimos(7_500)))),
            ("0", Ok(Some(Percentual::ZERO))),
        ] {
            texto.clone_into(&mut l.texto);
            assert_eq!(l.lido(), esperado, "{texto}");
        }
        for texto in ["", "dez", "101"] {
            texto.clone_into(&mut l.texto);
            assert!(l.lido().is_err(), "{texto}");
            assert!(!l.mudou(), "o que não vale não conta como mudança");
        }
        l.sem_teto = true;
        assert_eq!(l.lido(), Ok(None));
        assert!(l.mudou());
        let sem = linha(PapelDeFabrica::Gerente, None);
        assert!(sem.sem_teto && sem.texto.is_empty() && !sem.mudou());
    }

    #[test]
    fn salva_so_o_que_mudou_e_a_tela_volta_com_o_gravado() {
        let t = crate::testes_comum::motor_de_teste();
        let (m, s) = (&t.motor, &t.sessao);
        let mut linhas = carregar(m, s);
        let papeis: Vec<_> = linhas.iter().map(|l| l.gravado.papel).collect();
        assert_eq!(
            papeis,
            vec![
                PapelDeFabrica::Administrador,
                PapelDeFabrica::Gerente,
                PapelDeFabrica::OperadorCaixa,
                PapelDeFabrica::Vendedor,
            ]
        );
        assert_eq!(salvar(m, s, &linhas), Ok(0), "nada mudou, nada grava");
        "5".clone_into(&mut linhas[2].texto);
        linhas[1].sem_teto = false;
        "25".clone_into(&mut linhas[1].texto);
        assert_eq!(salvar(m, s, &linhas), Ok(2));
        let de_novo = carregar(m, s);
        assert_eq!(de_novo[1].gravado.teto, Some(Percentual::pontos(25)));
        assert_eq!(de_novo[2].gravado.teto, Some(Percentual::pontos(5)));
        assert_eq!(de_novo[3].gravado.teto, Some(Percentual::pontos(10)));
        assert!(de_novo.iter().all(|l| !l.mudou()));
    }

    #[test]
    fn o_cartao_desenha_com_e_sem_permissao() {
        let t = crate::testes_comum::motor_de_teste();
        let (m, s) = (&t.motor, &t.sessao);
        let mut linhas = carregar(m, s);
        let ctx = egui::Context::default();
        cardeal_ui::tokens::instalar_fontes(&ctx);
        cardeal_ui::tokens::instalar_estilo(&ctx, cardeal_ui::tokens::Tema::Claro);
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                assert!(!secao(ui, m, s, &mut linhas));
                assert!(!secao(ui, m, s, &mut []));
            });
        });
    }
}
