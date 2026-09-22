//! Camada 2 (molecules) — `AcoesRegistro`: o par "Editar · Excluir" que fecha a linha de uma
//! listagem de cadastro (produtos, clientes, ordens de serviço).
//!
//! Existe para que a coluna "Ações" seja **igual em toda tela**: mesmos rótulos, mesma cor,
//! mesma ordem. Antes cada tela repetia os dois botões e a divergência era só questão de
//! tempo. Use dentro de `row.col(|ui| …)` de uma [`Grade`](crate::organisms::Grade) marcada
//! com [`Grade::com_acoes`](crate::organisms::Grade::com_acoes), que dá a altura de linha
//! que estes botões pedem.

use egui::Ui;

use crate::atoms::Botao;
use crate::tokens::TemaUi;

/// O que o usuário pediu para o registro da linha.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcaoRegistro {
    /// Abrir o registro para edição.
    Editar,
    /// Excluir (a tela confirma antes — ver `dialogo_confirmacao`).
    Excluir,
}

/// Os botões de ação de uma linha.
#[must_use]
pub struct AcoesRegistro {
    pode_excluir: bool,
}

impl Default for AcoesRegistro {
    fn default() -> Self {
        Self::novo()
    }
}

impl AcoesRegistro {
    /// Editar e Excluir.
    pub const fn novo() -> Self {
        Self { pode_excluir: true }
    }

    /// Esconde "Excluir" quando o registro não aceita exclusão no estado atual (ex.: uma OS
    /// já faturada) — o botão some em vez de aparecer para falhar depois.
    pub const fn excluir(mut self, pode: bool) -> Self {
        self.pode_excluir = pode;
        self
    }

    /// Desenha os botões; devolve o que foi clicado neste quadro.
    pub fn mostrar(self, ui: &mut Ui) -> Option<AcaoRegistro> {
        let rubro = ui.cores().rubro;
        let mut acao = None;
        ui.horizontal(|ui| {
            if ui
                .add(Botao::fantasma("Editar").pequeno().cor(rubro))
                .clicked()
            {
                acao = Some(AcaoRegistro::Editar);
            }
            if self.pode_excluir && ui.add(Botao::destrutivo("Excluir").pequeno()).clicked() {
                acao = Some(AcaoRegistro::Excluir);
            }
        });
        acao
    }
}
