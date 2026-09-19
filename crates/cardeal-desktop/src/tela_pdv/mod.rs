//! Tela de PDV — a frente de caixa. `docs/modulos/pdv.md` §10.
//!
//! **Teclado e leitor de código de barras primeiro** (`docs/12-ui-ux.md` §1.3): o operador de
//! balcão nunca precisa do mouse. O campo de bipe fica sempre focado; `Enter` adiciona;
//! `F2` finaliza; `1..4` escolhe a forma de pagamento. Só há dica de tecla para o que
//! funciona (uma dica falsa é pior que nenhuma).
//!
//! Fluxo: escolher o caixa → abrir a sessão (se fechada) → bipar produtos no cupom →
//! finalizar. O carrinho é reconstruído no cliente a partir do retorno de cada comando (preço
//! resolvido pelo backend); não há consulta de leitura do cupom nesta fatia do `mod-pdv`.
//!
//! Organização: `entrada` (o que o operador digitou) e `pagamento` (a conta do troco) são
//! lógica pura e testada; este arquivo tem o estado, as visões e as ações. As visões só
//! empilham [`Acao`]s — quem fala com o motor é [`aplicar`], então a tela desenha sem backend.

mod acoes;
#[cfg(feature = "demo")]
pub mod demo;
mod dialogo_pagamento;
mod dialogos;
mod entrada;
mod estado;
mod pagamento;
#[cfg(test)]
mod testes;
mod visao;

use cardeal_cliente::{MotorLocal, SessaoLocal};
use cardeal_kernel::texto::casa_por_palavras;
use cardeal_kernel::{Arredondamento, Dinheiro, Id, Percentual, Preco, Quantidade};
use cardeal_modkit::Icone;
use cardeal_ui::atoms::{Botao, Divisor, Etiqueta, Rotulo, Tecla, Tom};
use cardeal_ui::molecules::{Campo, CampoBusca, EstadoVazio, ItemDeLista, SeletorOpcao};
use cardeal_ui::organisms::{
    notificar, ColunaGrade, Dialogo, Grade, LayoutTela, Notificacao, Painel,
};
use cardeal_ui::tokens::{Espaco, Papel, TemaUi};
use eframe::egui;
use mod_clientes::{ItemPessoa, Papel as PapelPessoa, PessoasPorPapel};
use mod_estoque::{
    ItemLocal, ItemProdutoComSaldo, Locais, ProdutoPorCodigoBarras, ProdutosComSaldo,
};
use mod_financeiro::{
    AbrirCaixa, CadastrarCaixa, CaixaCadastrado, CaixaFoiAberto, CaixaFoiFechado, Caixas,
    ContasDeCaixa, FecharCaixa, ItemCaixa, ItemContaResultado, RegistrarSangria, TOLERANCIA_QUEBRA,
};
use mod_pdv::{
    AbrirCupom, AdicionarItem, AplicarDescontoItem, CancelarCupom, CupomAberto, FinalizarVenda,
    IdentificarCliente, ItemFoiAdicionado, PrecoConsultado, PrecoDoProduto, VendaFoiFinalizada,
};
use mod_vendas::{TabelaPreco, TabelasDePreco};

use entrada::Entrada;
use pagamento::{Situacao, Valores, FORMAS};

use acoes::*;
use dialogo_pagamento::*;
use dialogos::*;
pub use estado::EstadoTelaPdv;
use estado::*;
use visao::*;

const SERIE_FISCAL: i64 = 1;
/// Quantos resultados de busca por nome a lista mostra.
const MAX_RESULTADOS: usize = 8;
/// As teclas `1..=4` escolhem a forma de pagamento, na ordem de [`FORMAS`].
const TECLAS_FORMA: [egui::Key; 4] = [
    egui::Key::Num1,
    egui::Key::Num2,
    egui::Key::Num3,
    egui::Key::Num4,
];

/// Consome uma tecla (sem modificador) — assim nenhum outro widget a vê. Devolve se estava
/// pressionada neste quadro.
fn consumir(ctx: &egui::Context, tecla: egui::Key) -> bool {
    ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, tecla))
}

/// Tira o foco de quem o tem — chamado ao abrir um diálogo, para que o leitor de código de
/// barras não continue "digitando" no campo por trás dele.
fn abrir(ctx: &egui::Context, estado: &mut EstadoTelaPdv, dlg: Dlg) {
    ctx.memory_mut(|m| {
        if let Some(id) = m.focused() {
            m.surrender_focus(id);
        }
    });
    estado.dlg = dlg;
}

// ── entrada principal ────────────────────────────────────────────────────────

/// Desenha a tela inteira e executa o que ela pediu.
pub fn mostrar(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaPdv,
) {
    // Execução adiada do fechamento fiscal: o clique só marcou a intenção; agora, um quadro
    // depois, o spinner/toast já pintaram e podemos rodar a chamada bloqueante.
    if std::mem::take(&mut estado.finalizar_pendente) {
        finalizar(ui.ctx(), motor, sessao, estado);
    }

    atalhos(ui.ctx(), estado);
    desenhar(ui, estado);
    dialogos(ui.ctx(), motor, sessao, estado);

    for acao in std::mem::take(&mut estado.pendentes) {
        aplicar(acao, ui.ctx(), motor, sessao, estado);
    }
}

/// As teclas da tela de venda. Com diálogo aberto o diálogo trata as próprias teclas.
fn atalhos(ctx: &egui::Context, estado: &mut EstadoTelaPdv) {
    if !matches!(estado.dlg, Dlg::Fechado) {
        return;
    }
    if estado.caixas.is_empty() {
        return;
    }
    if estado.sessao_aberta().is_none() {
        if consumir(ctx, egui::Key::F12) {
            abrir(
                ctx,
                estado,
                Dlg::AbrirCaixa {
                    valor: "0,00".to_owned(),
                },
            );
        }
        return;
    }

    if consumir(ctx, egui::Key::F2) {
        estado.pendentes.push(Acao::AbrirPagamento);
    }
    if consumir(ctx, egui::Key::F3) {
        estado.foco_entrada = true;
    }
    if consumir(ctx, egui::Key::F5) {
        estado.pendentes.push(Acao::AbrirDesconto);
    }
    if consumir(ctx, egui::Key::F7) {
        estado.pendentes.push(Acao::CancelarLinha);
    }
    if consumir(ctx, egui::Key::F8) {
        estado.pendentes.push(Acao::PedirCancelarCupom);
    }
    if consumir(ctx, egui::Key::F9) {
        estado.pendentes.push(Acao::AbrirSangria);
    }
    if consumir(ctx, egui::Key::F12) {
        estado.pendentes.push(Acao::AbrirFechamento);
    }
    if consumir(ctx, egui::Key::F6) {
        estado.pendentes.push(Acao::AbrirCliente);
    }
    if consumir(ctx, egui::Key::F10) {
        estado.pendentes.push(Acao::AbrirConsultaPreco);
    }
    if consumir(ctx, egui::Key::Escape) {
        estado.entrada.clear();
        estado.foco_entrada = true;
    }

    // ↑↓: com uma busca por nome na tela, escolhem o resultado; senão, o item do cupom.
    estado.atualizar_resultados();
    let n_resultados = estado.resultados.1.len();
    for (tecla, subir) in [(egui::Key::ArrowUp, true), (egui::Key::ArrowDown, false)] {
        if !consumir(ctx, tecla) {
            continue;
        }
        if n_resultados > 0 {
            estado.sel_resultado = if subir {
                estado.sel_resultado.saturating_sub(1)
            } else {
                (estado.sel_resultado + 1).min(n_resultados - 1)
            };
        } else {
            estado.mover_linha(subir);
        }
    }
}
