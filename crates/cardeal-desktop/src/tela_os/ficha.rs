//! A ficha de entrada da OS (previsão de entrega, nº de série, acessórios) — o mesmo
//! formulário na abertura e em "Editar dados", e o jeito de mostrar a previsão na lista.

use super::*;
use mod_os::FichaEntrada;

/// Os campos da ficha como o usuário digita (texto), até virar [`FichaEntrada`].
#[derive(Debug, Clone, Default)]
pub(super) struct FormFicha {
    pub(super) previsao: String,
    pub(super) numero_serie: String,
    pub(super) acessorios: String,
}

impl FormFicha {
    /// Preenche a partir da ficha gravada (para editar).
    pub(super) fn de(ficha: &FichaEntrada) -> Self {
        Self {
            previsao: ficha
                .previsao_entrega
                .map(|d| d.to_string())
                .unwrap_or_default(),
            numero_serie: ficha.numero_serie.clone(),
            acessorios: ficha.acessorios.clone(),
        }
    }

    /// A ficha pronta para o comando.
    ///
    /// # Errors
    /// Mensagem para o usuário quando a previsão foi digitada mas não é uma data.
    pub(super) fn validada(&self) -> Result<FichaEntrada, &'static str> {
        let previsao = match self.previsao.trim() {
            "" => None,
            t => Some(
                t.parse::<Data>()
                    .map_err(|_| "Previsão de entrega: use dd/mm/aaaa.")?,
            ),
        };
        Ok(FichaEntrada {
            previsao_entrega: previsao,
            numero_serie: self.numero_serie.clone(),
            acessorios: self.acessorios.clone(),
        })
    }

    /// Desenha os campos, com atalhos de previsão ("amanhã", "+3 dias", "+1 semana") — o
    /// prazo que o balcão promete de cabeça, sem abrir calendário.
    pub(super) fn mostrar(&mut self, ui: &mut egui::Ui) {
        ui.columns(2, |c| {
            c[0].add(
                Campo::novo("Previsão de entrega (opcional)", &mut self.previsao)
                    .mascara(Mascara::Data),
            );
            c[1].add(
                Campo::novo("Nº de série / IMEI (opcional)", &mut self.numero_serie)
                    .marcador("etiqueta do fabricante"),
            );
        });
        ui.horizontal(|ui| {
            let hoje = Data::hoje(Fuso::BRASILIA);
            for (rotulo, dias) in [("Amanhã", 1), ("+3 dias", 3), ("+1 semana", 7)] {
                if ui.add(Botao::fantasma(rotulo).pequeno()).clicked() {
                    self.previsao = hoje.mais_dias(dias).to_string();
                }
            }
        });
        ui.add_space(Espaco::E8);
        ui.add(
            Campo::novo(
                "Acessórios e condição na entrada (opcional)",
                &mut self.acessorios,
            )
            .marcador("ex.: com carregador, sem capinha, risco na tampa"),
        );
    }
}

/// A previsão como etiqueta de grade: vermelha se passou, âmbar se é hoje, neutra se está no
/// prazo; `None` quando a OS não tem previsão (ou já saiu da fila ativa).
pub(super) fn etiqueta_previsao(os: &OrdemServico, hoje: Data) -> Option<Etiqueta> {
    let previsao = os.ficha.previsao_entrega?;
    if !pode_faturar(os.estado) {
        return None;
    }
    let texto = previsao.formatar_curta();
    Some(if previsao < hoje {
        Etiqueta::negativa(format!("{texto} · atrasada"))
    } else if previsao == hoje {
        Etiqueta::atencao(format!("{texto} · hoje"))
    } else {
        Etiqueta::neutra(texto)
    })
}
