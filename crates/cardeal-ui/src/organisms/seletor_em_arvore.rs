//! Camada 3 (organisms) — `SeletorEmArvore`: escolhe um registro de uma hierarquia (grupo de
//! produto, conta do plano…). O campo mostra o **caminho** escolhido ("Bebidas › Sucos"); ao
//! clicar abre um painel com a [`ArvoreHierarquia`] expansível e uma busca incremental (digitar
//! mostra só o que casa, com os pais, tudo aberto). Clicar num nó fecha o painel e escolhe-o.
//! `docs/12-ui-ux.md` §7.
//!
//! Componente controlado: a tela guarda o `Option<Id>` escolhido e o [`EstadoSeletorArvore`]
//! (busca e nós recolhidos); os nós chegam em pré-ordem, como na árvore.

use std::collections::HashSet;

use cardeal_kernel::texto::casa_por_palavras;
use cardeal_kernel::Id;
use egui::Ui;

use crate::atoms::{CampoTexto, Rotulo};
use crate::molecules::{CampoSelecao, ItemDeLista};
use crate::organisms::{ArvoreHierarquia, NoArvore};
use crate::tokens::Espaco;

/// O estado de tela do seletor: a busca digitada e o que está recolhido na árvore.
#[derive(Debug, Default, Clone)]
pub struct EstadoSeletorArvore {
    /// O texto da busca incremental.
    pub busca: String,
    /// Os nós recolhidos (fora da busca).
    pub recolhidos: HashSet<Id>,
}

/// O caminho do nó `id` ("Grupo › Sub › Cat"), a partir dos nós em pré-ordem.
#[must_use]
pub fn caminho_do_no(nos: &[NoArvore], id: Id) -> Option<String> {
    let mut pilha: Vec<&str> = Vec::new();
    for no in nos {
        pilha.truncate(usize::from(no.nivel.saturating_sub(1)));
        pilha.push(&no.rotulo);
        if no.id == id {
            return Some(pilha.join(" › "));
        }
    }
    None
}

/// Os nós cujo rótulo casa com `termo` (sem caixa/acento, palavras em qualquer ordem), mais os
/// ancestrais deles — a árvore reduzida que a busca mostra. `tem_filhos` reflete o que sobrou.
#[must_use]
pub fn filtrar_nos(nos: &[NoArvore], termo: &str) -> Vec<NoArvore> {
    let mut manter = vec![false; nos.len()];
    let mut pilha: Vec<usize> = Vec::new();
    for (i, no) in nos.iter().enumerate() {
        pilha.truncate(usize::from(no.nivel.saturating_sub(1)));
        if casa_por_palavras(&no.rotulo, termo) {
            manter[i] = true;
            for a in &pilha {
                manter[*a] = true;
            }
        }
        pilha.push(i);
    }
    let mut saida: Vec<NoArvore> = nos
        .iter()
        .zip(&manter)
        .filter(|(_, m)| **m)
        .map(|(n, _)| n.clone())
        .collect();
    for i in 0..saida.len() {
        let nivel = saida[i].nivel;
        saida[i].tem_filhos = saida.get(i + 1).is_some_and(|p| p.nivel > nivel);
    }
    saida
}

/// O seletor em árvore.
#[must_use]
pub struct SeletorEmArvore<'a> {
    rotulo: String,
    selecionado: &'a mut Option<Id>,
    nos: &'a [NoArvore],
    estado: &'a mut EstadoSeletorArvore,
    marcador: String,
    opcao_vazia: Option<String>,
    abrir: bool,
    com_rotulo: bool,
}

impl<'a> SeletorEmArvore<'a> {
    /// Um seletor com `rotulo` (também a identidade do painel — único na tela).
    pub fn novo(
        rotulo: impl Into<String>,
        selecionado: &'a mut Option<Id>,
        nos: &'a [NoArvore],
        estado: &'a mut EstadoSeletorArvore,
    ) -> Self {
        Self {
            rotulo: rotulo.into(),
            selecionado,
            nos,
            estado,
            marcador: "Selecione…".to_owned(),
            opcao_vazia: None,
            abrir: false,
            com_rotulo: true,
        }
    }

    /// O texto do campo quando nada está escolhido.
    pub fn marcador(mut self, marcador: impl Into<String>) -> Self {
        self.marcador = marcador.into();
        self
    }

    /// Acrescenta no topo do painel uma linha que **limpa** a escolha ("Todos os grupos").
    pub fn opcao_vazia(mut self, texto: impl Into<String>) -> Self {
        self.opcao_vazia = Some(texto.into());
        self
    }

    /// Não desenha o rótulo acima do campo (barras de filtro, onde o marcador já diz do que se
    /// trata). O rótulo continua sendo a identidade do painel.
    pub const fn sem_rotulo(mut self) -> Self {
        self.com_rotulo = false;
        self
    }

    /// Abre o painel neste quadro (conferência visual da galeria; telas não precisam).
    pub const fn abrir(mut self, abrir: bool) -> Self {
        self.abrir = abrir;
        self
    }

    /// Desenha. Devolve `true` no quadro em que a escolha mudou.
    pub fn mostrar(self, ui: &mut Ui) -> bool {
        let Self {
            rotulo,
            selecionado,
            nos,
            estado,
            marcador,
            opcao_vazia,
            abrir,
            com_rotulo,
        } = self;
        let id_popup = ui.make_persistent_id(("seletor-em-arvore", rotulo.as_str()));
        let valor = selecionado.and_then(|id| caminho_do_no(nos, id));
        let mut campo = CampoSelecao::nova(valor).marcador(marcador);
        if com_rotulo {
            campo = campo.rotulo(rotulo.clone());
        }
        let campo = campo.mostrar(ui);
        if abrir {
            ui.memory_mut(|m| m.open_popup(id_popup));
        }
        if campo.clicked() {
            ui.memory_mut(|m| m.toggle_popup(id_popup));
            estado.busca.clear();
        }

        let mut mudou = false;
        let largura = campo.rect.width().clamp(280.0_f32, 520.0_f32);
        egui::popup::popup_below_widget(
            ui,
            id_popup,
            &campo,
            egui::popup::PopupCloseBehavior::CloseOnClickOutside,
            |ui| {
                ui.set_min_width(largura);
                ui.set_max_width(largura);
                ui.add_space(Espaco::E4);
                let busca = ui.add(
                    CampoTexto::novo(&mut estado.busca)
                        .marcador("Buscar…")
                        .preenche_largura(true),
                );
                if !busca.has_focus() && estado.busca.is_empty() {
                    busca.request_focus();
                }
                ui.add_space(Espaco::E4);
                if let Some(texto) = &opcao_vazia {
                    if ItemDeLista::novo(texto.clone())
                        .selecionado(selecionado.is_none())
                        .mostrar(ui, |_| {})
                        .clicked()
                    {
                        *selecionado = None;
                        mudou = true;
                    }
                }
                let termo = estado.busca.trim().to_owned();
                let reduzidos;
                let (lista, mut abertos): (&[NoArvore], HashSet<Id>) = if termo.is_empty() {
                    (nos, std::mem::take(&mut estado.recolhidos))
                } else {
                    reduzidos = filtrar_nos(nos, &termo);
                    (&reduzidos, HashSet::new())
                };
                egui::ScrollArea::vertical()
                    .max_height(280.0_f32)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        let r = ArvoreHierarquia::nova(lista, &mut abertos)
                            .selecionado(*selecionado)
                            .mostrar(ui, |_, _| {});
                        if let Some(id) = r.clicado {
                            *selecionado = Some(id);
                            mudou = true;
                        }
                        if !termo.is_empty() && lista.is_empty() {
                            ui.add(Rotulo::apoio("Nenhum resultado."));
                        }
                    });
                if termo.is_empty() {
                    estado.recolhidos = abertos;
                }
            },
        );
        if mudou {
            ui.memory_mut(egui::Memory::close_popup);
        }
        mudou
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn no(nivel: u8, rotulo: &str) -> NoArvore {
        NoArvore {
            id: Id::novo(),
            nivel,
            rotulo: rotulo.to_owned(),
            apoio: None,
            tem_filhos: false,
        }
    }

    fn amostra() -> Vec<NoArvore> {
        vec![
            no(1, "Bebidas"),
            no(2, "Refrigerantes"),
            no(3, "Cola"),
            no(2, "Sucos"),
            no(1, "Peças"),
            no(2, "Telas"),
        ]
    }

    #[test]
    fn caminho_junta_os_ancestrais() {
        let nos = amostra();
        assert_eq!(
            caminho_do_no(&nos, nos[2].id).as_deref(),
            Some("Bebidas › Refrigerantes › Cola")
        );
        assert_eq!(
            caminho_do_no(&nos, nos[5].id).as_deref(),
            Some("Peças › Telas")
        );
        assert_eq!(caminho_do_no(&nos, nos[0].id).as_deref(), Some("Bebidas"));
        assert_eq!(caminho_do_no(&nos, Id::novo()), None);
    }

    #[test]
    fn busca_traz_os_pais_e_ignora_acento() {
        let nos = amostra();
        let r = filtrar_nos(&nos, "pecas");
        let nomes: Vec<_> = r.iter().map(|n| n.rotulo.as_str()).collect();
        assert_eq!(nomes, ["Peças"]);
        let r = filtrar_nos(&nos, "cola");
        let nomes: Vec<_> = r.iter().map(|n| n.rotulo.as_str()).collect();
        assert_eq!(nomes, ["Bebidas", "Refrigerantes", "Cola"]);
        assert!(r[0].tem_filhos && r[1].tem_filhos && !r[2].tem_filhos);
        assert!(filtrar_nos(&nos, "zzz").is_empty());
    }
}
