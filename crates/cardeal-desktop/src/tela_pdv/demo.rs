//! Cenários fictícios do PDV, para conferir o desenho sem motor nem banco.
//!
//! Compilado só com `--features demo`; usado por `examples/pdv_demo.rs`, que renderiza um
//! cenário e salva um PNG (`docs/12-ui-ux.md` §7: "conferência visual"). Não faz parte do
//! binário de produção.

use cardeal_kernel::{Dinheiro, Id, Percentual, Preco, Quantidade};
use mod_estoque::{ItemLocal, ItemProdutoComSaldo};
use mod_financeiro::ItemCaixa;
use mod_vendas::{TabelaPreco, TipoTabela};

use super::{desenhar, dialogo_pagamento, Dlg, EstadoPagamento, EstadoTelaPdv, Linha, UltimaVenda};

/// Os cenários que o exemplo sabe montar.
pub const CENARIOS: [&str; 6] = [
    "venda",
    "busca",
    "pagamento-troco",
    "pagamento-falta",
    "concluida",
    "fechado",
];

fn produto(nome: &str, saldo: i64, codigo: &str) -> ItemProdutoComSaldo {
    ItemProdutoComSaldo {
        produto: Id::novo(),
        nome: nome.to_owned(),
        ncm: "22021000".to_owned(),
        disponivel: Quantidade::UM.vezes(saldo),
        reservado: Quantidade::ZERO,
        custo_medio: "1,00".parse().unwrap_or_else(|_| unreachable!()),
        codigo_barras: Some(codigo.to_owned()),
    }
}

fn linha(nome: &str, codigo: &str, qtd: &str, preco: &str, pct: i64) -> Linha {
    let quantidade: Quantidade = qtd.parse().unwrap_or_else(|_| unreachable!());
    let preco: Preco = preco.parse().unwrap_or_else(|_| unreachable!());
    let mut l = Linha {
        item: Id::novo(),
        nome: nome.to_owned(),
        codigo: Some(codigo.to_owned()),
        quantidade,
        preco,
        desconto: Percentual::ZERO,
        total: Dinheiro::ZERO,
        cancelado: false,
    };
    l.total = l.bruto();
    if pct > 0 {
        l.com_desconto(Percentual::pontos(pct));
    }
    l
}

/// Monta o estado de um cenário. `None` se o nome não existe.
#[must_use]
pub fn estado(cenario: &str) -> Option<EstadoTelaPdv> {
    let mut e = EstadoTelaPdv::default();
    let caixa = Id::novo();
    let tabela = Id::novo();
    let local = Id::novo();
    e.caixas = vec![ItemCaixa {
        caixa,
        nome: "Caixa 1".to_owned(),
        conta_razao: Id::novo(),
        sessao_aberta: (cenario != "fechado").then(Id::novo),
    }];
    e.tabelas = vec![TabelaPreco {
        id: tabela,
        empresa: Id::novo(),
        nome: "Varejo".to_owned(),
        tipo: TipoTabela::Venda,
        vigente_de: "2026-01-01".parse().unwrap_or_else(|_| unreachable!()),
        vigente_ate: None,
        ativa: true,
        versao: cardeal_kernel::Versao::INICIAL,
    }];
    e.locais = vec![ItemLocal {
        id: local,
        nome: "Loja".to_owned(),
        tipo: "Loja".to_owned(),
    }];
    e.caixa_sel = Some(caixa);
    e.tabela_sel = Some(tabela);
    e.local_sel = Some(local);
    e.produtos = vec![
        produto("Refrigerante Cola 2L", 48, "7894900011517"),
        produto("Refrigerante Guaraná 2L", 0, "7894900027013"),
        produto("Refrigerante Cola Lata 350ml", 120, "7894900010015"),
        produto("Pão Francês (kg)", 12, "2000000000015"),
        produto("Achocolatado 400g", 30, "7891234567895"),
        produto("Açúcar Cristal 1kg", 64, "7896065700018"),
    ];

    let com_itens = |e: &mut EstadoTelaPdv| {
        e.carrinho = vec![
            linha("Refrigerante Cola 2L", "7894900011517", "2", "6,90", 0),
            linha("Pão Francês (kg)", "2000000000015", "0,450", "18,90", 0),
            linha("Achocolatado 400g", "7891234567895", "1", "9,50", 5),
        ];
        e.linha_sel = Some(1);
        e.cupom = Some(Id::novo());
        e.cupom_numero = Some(4087);
        e.recalcular_total();
    };

    match cenario {
        "venda" => com_itens(&mut e),
        "busca" => {
            com_itens(&mut e);
            e.entrada = "refri".to_owned();
        }
        "pagamento-troco" | "pagamento-falta" => {
            com_itens(&mut e);
            let mut p = EstadoPagamento::nova();
            p.valores[0] = if cenario == "pagamento-troco" {
                Dinheiro::reais(50)
            } else {
                Dinheiro::reais(20)
            };
            p.ativa = 1;
            e.dlg = Dlg::Pagamento(p);
        }
        "concluida" => {
            e.ultima_venda = Some(UltimaVenda {
                numero: 4087,
                total: Dinheiro::reais(31),
                troco: Dinheiro::reais(19),
            });
        }
        "fechado" => {}
        _ => return None,
    }
    Some(e)
}

/// Desenha um quadro do cenário (tela + diálogo de pagamento, se aberto), sem motor.
pub fn desenhar_quadro(ui: &mut eframe::egui::Ui, estado: &mut EstadoTelaPdv) {
    desenhar(ui, estado);
    if matches!(estado.dlg, Dlg::Pagamento(_)) {
        dialogo_pagamento(ui.ctx(), estado);
    }
}
