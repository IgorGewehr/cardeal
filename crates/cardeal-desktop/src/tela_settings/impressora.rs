//! Configurações › Impressão › **Impressora deste computador**: para onde vai o cupom (o DANFCE e
//! o comprovante) — o PDF no visualizador (o padrão, que funciona sem configurar nada), uma
//! impressora instalada ou uma de rede, em ESC/POS direto, sem abrir o PDF. Fica nesta máquina
//! (`crate::maquina`), não na empresa: cada caixa tem a sua impressora.

use cardeal_ui::atoms::{Botao, Caixa, Rotulo};
use cardeal_ui::molecules::{Campo, SeletorOpcao};
use cardeal_ui::organisms::{notificar, Notificacao};
use cardeal_ui::tokens::Espaco;
use eframe::egui;

use crate::maquina::{Destino, ImpressoraDaMaquina, Maquina};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tipo {
    Pdf,
    Instalada,
    Rede,
}

/// O que está sendo escolhido.
#[derive(Default)]
pub(super) struct EstadoImpressora {
    carregado: bool,
    tipo: Option<Tipo>,
    /// As impressoras instaladas neste computador.
    instaladas: Vec<String>,
    escolhida: Option<usize>,
    rede: String,
    acentos: bool,
}

impl EstadoImpressora {
    /// Lê a preferência desta máquina e a lista de impressoras instaladas.
    pub(super) fn carregar(&mut self) {
        let i = crate::maquina::atual().impressora;
        self.instaladas = crate::impressora::instaladas();
        self.acentos = i.acentos;
        self.escolhida = None;
        self.tipo = Some(match i.destino {
            Destino::Pdf => Tipo::Pdf,
            Destino::Instalada(nome) => {
                // A gravada some da lista se foi desinstalada: continua aparecendo, escolhida.
                let pos = self.instaladas.iter().position(|n| *n == nome);
                self.escolhida = Some(pos.unwrap_or_else(|| {
                    self.instaladas.push(nome);
                    self.instaladas.len() - 1
                }));
                Tipo::Instalada
            }
            Destino::Rede(e) => {
                self.rede = e;
                Tipo::Rede
            }
        });
        self.carregado = true;
    }

    /// O destino escolhido, ou o que falta escolher.
    fn destino(&self) -> Result<Destino, &'static str> {
        match self.tipo.unwrap_or(Tipo::Pdf) {
            Tipo::Pdf => Ok(Destino::Pdf),
            Tipo::Instalada => self
                .escolhida
                .and_then(|i| self.instaladas.get(i))
                .cloned()
                .map(Destino::Instalada)
                .ok_or("Escolha a impressora"),
            Tipo::Rede => {
                let e = self.rede.trim();
                if e.is_empty() {
                    Err("Informe o endereço da impressora (ex.: 192.168.0.50)")
                } else {
                    Ok(Destino::Rede(e.to_owned()))
                }
            }
        }
    }

    fn impressora(&self) -> Result<ImpressoraDaMaquina, &'static str> {
        Ok(ImpressoraDaMaquina {
            destino: self.destino()?,
            acentos: self.acentos,
        })
    }
}

/// Desenha a seção. `bobina` é a largura configurada para a empresa (a da página de teste).
pub(super) fn secao(
    ui: &mut egui::Ui,
    pode: bool,
    bobina: cardeal_pdf::Bobina,
    estado: &mut EstadoImpressora,
) {
    if !estado.carregado {
        estado.carregar();
    }
    ui.add(Rotulo::titulo_secao("Impressora deste computador"));
    ui.add_space(Espaco::E8);
    ui.add(
        Rotulo::apoio(
            "Vale só nesta máquina — cada caixa tem a sua. Direto na térmica (ESC/POS), o cupom \
             sai sem abrir o PDF; a impressora instalada precisa do driver da marca, e a de rede, \
             do IP fixo.",
        )
        .quebravel(),
    );
    ui.add_space(Espaco::E12);
    ui.columns(2, |c| {
        SeletorOpcao::novo("Para onde vai o cupom", &mut estado.tipo)
            .opcao(Tipo::Pdf, "Abrir o PDF para imprimir")
            .opcao(Tipo::Instalada, "Impressora instalada (direto)")
            .opcao(Tipo::Rede, "Impressora de rede (direto)")
            .mostrar(&mut c[0]);
        match estado.tipo {
            Some(Tipo::Instalada) => {
                let mut s = SeletorOpcao::novo("Impressora", &mut estado.escolhida);
                for (i, nome) in estado.instaladas.iter().enumerate() {
                    s = s.opcao(i, nome.clone());
                }
                s.mostrar(&mut c[1]);
            }
            Some(Tipo::Rede) => {
                c[1].add(
                    Campo::novo("Endereço (IP ou IP:porta)", &mut estado.rede)
                        .marcador("192.168.0.50 — a porta padrão é a 9100"),
                );
            }
            Some(Tipo::Pdf) | None => {}
        }
    });
    let direto = matches!(estado.tipo, Some(Tipo::Instalada | Tipo::Rede));
    if estado.tipo == Some(Tipo::Instalada) && estado.instaladas.is_empty() {
        ui.add(Rotulo::apoio(
            "Nenhuma impressora instalada encontrada neste computador.",
        ));
    }
    if direto {
        ui.add_space(Espaco::E8);
        ui.add(Caixa::nova(
            &mut estado.acentos,
            "Imprimir com acento (se a página de teste sair com letras estranhas, desligue)",
        ));
    }
    ui.add_space(Espaco::E12);
    ui.horizontal(|ui| {
        if ui
            .add(Botao::primario("Salvar neste computador").habilitado(pode))
            .clicked()
        {
            let r = estado
                .impressora()
                .map_err(str::to_owned)
                .and_then(|impressora| crate::maquina::salvar(Maquina { impressora }));
            match r {
                Ok(()) => notificar(
                    ui.ctx(),
                    Notificacao::sucesso("Impressora deste computador salva"),
                ),
                Err(e) => notificar(ui.ctx(), Notificacao::erro(e)),
            }
        }
        if ui
            .add(Botao::secundario("Imprimir teste").habilitado(direto))
            .clicked()
        {
            let r = estado.impressora().map_err(str::to_owned).and_then(|i| {
                let bytes = cardeal_pdf::teste_escpos(bobina, crate::impressora::opcoes(&i));
                crate::impressora::enviar(&i.destino, &bytes)
            });
            match r {
                Ok(onde) => notificar(
                    ui.ctx(),
                    Notificacao::sucesso(format!("Página de teste enviada à impressora {onde}")),
                ),
                Err(e) => notificar(ui.ctx(), Notificacao::erro(e)),
            }
        }
        if estado.tipo == Some(Tipo::Instalada)
            && ui.add(Botao::fantasma("Procurar de novo")).clicked()
        {
            estado.instaladas = crate::impressora::instaladas();
            estado.escolhida = None;
        }
    });
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn o_destino_escolhido_e_o_que_falta() {
        let mut e = EstadoImpressora {
            tipo: Some(Tipo::Rede),
            ..Default::default()
        };
        assert!(e.destino().unwrap_err().contains("endereço"));
        e.rede = " 192.168.0.50 ".into();
        assert_eq!(e.destino(), Ok(Destino::Rede("192.168.0.50".into())));
        e.tipo = Some(Tipo::Instalada);
        assert_eq!(e.destino(), Err("Escolha a impressora"));
        e.instaladas = vec!["Elgin i9".into()];
        e.escolhida = Some(0);
        assert_eq!(e.destino(), Ok(Destino::Instalada("Elgin i9".into())));
        e.tipo = None;
        assert_eq!(e.destino(), Ok(Destino::Pdf));
    }

    #[test]
    fn desenha_e_le_a_maquina() {
        let mut e = EstadoImpressora::default();
        let ctx = egui::Context::default();
        cardeal_ui::tokens::instalar_fontes(&ctx);
        cardeal_ui::tokens::instalar_estilo(&ctx, cardeal_ui::tokens::Tema::Claro);
        for tipo in [None, Some(Tipo::Instalada), Some(Tipo::Rede)] {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    secao(ui, true, cardeal_pdf::Bobina::Mm80, &mut e);
                });
            });
            e.tipo = tipo;
        }
        assert!(e.carregado);
    }
}
