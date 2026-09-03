//! O manifesto do módulo financeiro — identidade, submódulos, permissões, menu e contas.
//!
//! Dado estático (`&'static`), lido pelo registro do motor no boot. Ver
//! `docs/modulos/financeiro.md` §2, §8 e §9 e `docs/contratos-internos.md` §4. A consistência
//! interna é provada por [`Manifesto::validar`] no teste ao fim deste arquivo.

use cardeal_ledger::PapelConta;
use cardeal_modkit::{
    ContaPadrao, EntradaMenu, Icone, IdModulo, Manifesto, Permissao, Risco, Submodulo,
};

/// O id estável do módulo.
pub const ID: IdModulo = IdModulo::novo("financeiro");

const fn perm(
    chave: &'static str,
    descricao: &'static str,
    risco: Risco,
    requer_submodulo: Option<&'static str>,
) -> Permissao {
    Permissao {
        chave,
        descricao,
        risco,
        requer_submodulo,
    }
}

const fn menu(
    id: &'static str,
    rotulo: &'static str,
    icone: Icone,
    peso: u16,
    permissao: &'static str,
    requer_submodulo: Option<&'static str>,
) -> EntradaMenu {
    EntradaMenu {
        id,
        rotulo,
        icone,
        peso,
        permissao,
        requer_submodulo,
        pai: None,
    }
}

const SUBMODULOS: &[Submodulo] = &[
    Submodulo {
        id: "caixa",
        nome: "Caixa",
        essencial: true,
        depende_de: &[],
    },
    Submodulo {
        id: "receber",
        nome: "Contas a Receber",
        essencial: true,
        depende_de: &[],
    },
    Submodulo {
        id: "pagar",
        nome: "Contas a Pagar",
        essencial: false,
        depende_de: &[],
    },
    Submodulo {
        id: "bancos",
        nome: "Contas Bancárias",
        essencial: false,
        depende_de: &[],
    },
    Submodulo {
        id: "conciliacao",
        nome: "Conciliação Bancária",
        essencial: false,
        depende_de: &["bancos"],
    },
    Submodulo {
        id: "projecao",
        nome: "Projeção de Fluxo",
        essencial: false,
        depende_de: &["receber"],
    },
    Submodulo {
        id: "centro_custo",
        nome: "Centro de Custo",
        essencial: false,
        depende_de: &[],
    },
    Submodulo {
        id: "cobranca",
        nome: "Cobrança e Boletos",
        essencial: false,
        depende_de: &["receber", "bancos"],
    },
    Submodulo {
        id: "cheques",
        nome: "Cheques",
        essencial: false,
        depende_de: &["receber"],
    },
    Submodulo {
        id: "dre",
        nome: "DRE Gerencial",
        essencial: false,
        depende_de: &[],
    },
];

const PERMISSOES: &[Permissao] = &[
    perm("financeiro.pulso.ver", "Ver o Pulso", Risco::Baixo, None),
    // ── caixa ────────────────────────────────────────────────────────────────
    perm(
        "financeiro.caixa.ver",
        "Ver sessões e movimentos de caixa",
        Risco::Baixo,
        Some("caixa"),
    ),
    perm(
        "financeiro.caixa.abrir",
        "Abrir sessão de caixa",
        Risco::Baixo,
        Some("caixa"),
    ),
    perm(
        "financeiro.caixa.fechar",
        "Fechar sessão de caixa",
        Risco::Medio,
        Some("caixa"),
    ),
    perm(
        "financeiro.caixa.sangria",
        "Registrar sangria",
        Risco::Medio,
        Some("caixa"),
    ),
    perm(
        "financeiro.caixa.suprimento",
        "Registrar suprimento",
        Risco::Baixo,
        Some("caixa"),
    ),
    // ── receber ──────────────────────────────────────────────────────────────
    perm(
        "financeiro.receber.ver",
        "Consultar contas a receber",
        Risco::Baixo,
        Some("receber"),
    ),
    perm(
        "financeiro.receber.criar",
        "Lançar título a receber",
        Risco::Baixo,
        Some("receber"),
    ),
    perm(
        "financeiro.receber.baixar",
        "Dar baixa em recebimento",
        Risco::Medio,
        Some("receber"),
    ),
    perm(
        "financeiro.receber.estornar",
        "Estornar baixa de recebimento",
        Risco::Alto,
        Some("receber"),
    ),
    perm(
        "financeiro.receber.renegociar",
        "Renegociar título a receber",
        Risco::Alto,
        Some("receber"),
    ),
    // ── pagar ────────────────────────────────────────────────────────────────
    perm(
        "financeiro.pagar.ver",
        "Consultar contas a pagar",
        Risco::Baixo,
        Some("pagar"),
    ),
    perm(
        "financeiro.pagar.criar",
        "Lançar título a pagar",
        Risco::Baixo,
        Some("pagar"),
    ),
    perm(
        "financeiro.pagar.baixar",
        "Dar baixa em pagamento",
        Risco::Medio,
        Some("pagar"),
    ),
    perm(
        "financeiro.pagar.autorizar",
        "Autorizar pagamento acima do teto",
        Risco::Alto,
        Some("pagar"),
    ),
    // ── bancos e conciliação ─────────────────────────────────────────────────
    perm(
        "financeiro.banco.ver",
        "Consultar contas bancárias e extratos",
        Risco::Baixo,
        Some("bancos"),
    ),
    perm(
        "financeiro.banco.criar",
        "Cadastrar conta bancária",
        Risco::Medio,
        Some("bancos"),
    ),
    perm(
        "financeiro.conciliacao.ver",
        "Ver a conciliação bancária",
        Risco::Baixo,
        Some("conciliacao"),
    ),
    perm(
        "financeiro.conciliacao.importar",
        "Importar extrato bancário",
        Risco::Baixo,
        Some("conciliacao"),
    ),
    perm(
        "financeiro.conciliacao.confirmar",
        "Confirmar casamento de item do extrato",
        Risco::Medio,
        Some("conciliacao"),
    ),
    // ── projeção e recorrência ───────────────────────────────────────────────
    perm(
        "financeiro.projecao.ver",
        "Ver a projeção de fluxo de caixa",
        Risco::Baixo,
        Some("projecao"),
    ),
    perm(
        "financeiro.recorrencia.criar",
        "Criar/editar recorrência",
        Risco::Medio,
        Some("projecao"),
    ),
    // ── centro de custo ──────────────────────────────────────────────────────
    perm(
        "financeiro.centro_custo.ver",
        "Ver relatórios por centro de custo",
        Risco::Baixo,
        Some("centro_custo"),
    ),
    perm(
        "financeiro.centro_custo.criar",
        "Cadastrar centro de custo",
        Risco::Baixo,
        Some("centro_custo"),
    ),
    // ── cobrança ─────────────────────────────────────────────────────────────
    perm(
        "financeiro.cobranca.gerar",
        "Gerar boleto/Pix de cobrança",
        Risco::Baixo,
        Some("cobranca"),
    ),
    // ── cheques ──────────────────────────────────────────────────────────────
    perm(
        "financeiro.cheque.ver",
        "Ver a carteira de cheques",
        Risco::Baixo,
        Some("cheques"),
    ),
    perm(
        "financeiro.cheque.registrar",
        "Registrar cheque recebido",
        Risco::Baixo,
        Some("cheques"),
    ),
    perm(
        "financeiro.cheque.compensar",
        "Compensar cheque",
        Risco::Medio,
        Some("cheques"),
    ),
    perm(
        "financeiro.cheque.devolver",
        "Registrar devolução de cheque",
        Risco::Alto,
        Some("cheques"),
    ),
    // ── DRE ──────────────────────────────────────────────────────────────────
    perm(
        "financeiro.dre.ver",
        "Ver DRE gerencial",
        Risco::Medio,
        Some("dre"),
    ),
];

const MENU: &[EntradaMenu] = &[
    menu(
        "financeiro.pulso",
        "Pulso",
        Icone::Pulso,
        0,
        "financeiro.pulso.ver",
        None,
    ),
    menu(
        "financeiro.receber",
        "Contas a Receber",
        Icone::Dinheiro,
        10,
        "financeiro.receber.ver",
        Some("receber"),
    ),
    menu(
        "financeiro.pagar",
        "Contas a Pagar",
        Icone::Dinheiro,
        11,
        "financeiro.pagar.ver",
        Some("pagar"),
    ),
    menu(
        "financeiro.fluxo",
        "Fluxo de Caixa",
        Icone::Grafico,
        12,
        "financeiro.projecao.ver",
        Some("projecao"),
    ),
    menu(
        "financeiro.bancos",
        "Bancos",
        Icone::Banco,
        13,
        "financeiro.banco.ver",
        Some("bancos"),
    ),
    menu(
        "financeiro.conciliacao",
        "Conciliação",
        Icone::Conciliar,
        14,
        "financeiro.conciliacao.ver",
        Some("conciliacao"),
    ),
    menu(
        "financeiro.caixa",
        "Caixa",
        Icone::Caixa,
        15,
        "financeiro.caixa.ver",
        Some("caixa"),
    ),
    menu(
        "financeiro.centro_custo",
        "Centro de Custo",
        Icone::Grafico,
        16,
        "financeiro.centro_custo.ver",
        Some("centro_custo"),
    ),
    menu(
        "financeiro.cheques",
        "Cheques",
        Icone::Dinheiro,
        17,
        "financeiro.cheque.ver",
        Some("cheques"),
    ),
    menu(
        "financeiro.dre",
        "DRE",
        Icone::Grafico,
        18,
        "financeiro.dre.ver",
        Some("dre"),
    ),
];

const CONTAS_REQUERIDAS: &[ContaPadrao] = &[
    ContaPadrao {
        papel: PapelConta::Caixa,
        obrigatoria: true,
    },
    ContaPadrao {
        papel: PapelConta::Bancos,
        obrigatoria: true,
    },
    ContaPadrao {
        papel: PapelConta::ClientesAReceber,
        obrigatoria: true,
    },
    ContaPadrao {
        papel: PapelConta::Fornecedores,
        obrigatoria: false,
    },
    ContaPadrao {
        papel: PapelConta::ReceitaFinanceira,
        obrigatoria: false,
    },
    ContaPadrao {
        papel: PapelConta::DespesaFinanceira,
        obrigatoria: false,
    },
    ContaPadrao {
        papel: PapelConta::DescontosConcedidos,
        obrigatoria: false,
    },
    ContaPadrao {
        papel: PapelConta::QuebraCaixa,
        obrigatoria: false,
    },
    ContaPadrao {
        papel: PapelConta::OutrasReceitas,
        obrigatoria: false,
    },
    ContaPadrao {
        papel: PapelConta::ChequesAReceber,
        obrigatoria: false,
    },
    ContaPadrao {
        papel: PapelConta::ValoresEmTransito,
        obrigatoria: false,
    },
    ContaPadrao {
        papel: PapelConta::TaxasCartao,
        obrigatoria: false,
    },
];

const EVENTOS_PUBLICADOS: &[&str] = &[
    "financeiro.caixa_aberto.v1",
    "financeiro.caixa_fechado.v1",
    "financeiro.sangria_registrada.v1",
    "financeiro.titulo_lancado.v1",
    "financeiro.parcela_baixada.v1",
    "financeiro.baixa_estornada.v1",
    "financeiro.extrato_conciliado.v1",
    "financeiro.cheque_devolvido.v1",
    "financeiro.recorrencia_materializada.v1",
];

const EVENTOS_ASSINADOS: &[&str] = &[
    "vendas.pedido_faturado.v1",
    "pdv.venda_finalizada.v1",
    "compras.nota_confirmada.v1",
    "os.ordem_faturada.v1",
    "alugueis.fatura_gerada.v1",
    "hotelaria.folio_fechado.v1",
];

/// O manifesto do módulo financeiro.
pub static MANIFESTO: Manifesto = Manifesto {
    id: ID,
    nome: "Financeiro",
    versao: (0, 1, 0),
    descricao: "Caixa, contas a pagar e a receber, bancos, conciliação e projeção de fluxo.",
    icone: Icone::Dinheiro,
    depende_de: &[],
    melhora_com: &[],
    conflita_com: &[],
    submodulos: SUBMODULOS,
    permissoes: PERMISSOES,
    menu: MENU,
    contas_requeridas: CONTAS_REQUERIDAS,
    eventos_publicados: EVENTOS_PUBLICADOS,
    eventos_assinados: EVENTOS_ASSINADOS,
};

/// O manifesto do módulo, para o registro do motor.
#[must_use]
pub fn manifesto() -> &'static Manifesto {
    &MANIFESTO
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn manifesto_e_internamente_consistente() {
        MANIFESTO
            .validar()
            .expect("o manifesto do financeiro deve ser válido");
    }

    #[test]
    fn caixa_e_receber_sao_essenciais() {
        assert!(MANIFESTO.submodulo_essencial("caixa"));
        assert!(MANIFESTO.submodulo_essencial("receber"));
        assert!(!MANIFESTO.submodulo_essencial("dre"));
    }

    #[test]
    fn o_pulso_e_a_primeira_entrada_de_menu() {
        let pulso = MANIFESTO.menu.iter().min_by_key(|e| e.peso).unwrap();
        assert_eq!(pulso.id, "financeiro.pulso");
        assert_eq!(pulso.peso, 0);
    }

    #[test]
    fn toda_permissao_referida_no_menu_existe() {
        for entrada in MANIFESTO.menu {
            assert!(
                MANIFESTO
                    .permissoes
                    .iter()
                    .any(|p| p.chave == entrada.permissao),
                "menu {} referencia permissão inexistente {}",
                entrada.id,
                entrada.permissao
            );
        }
    }
}
