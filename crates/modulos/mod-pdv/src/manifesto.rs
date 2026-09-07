//! O manifesto do módulo de PDV.
//!
//! Ver `docs/modulos/pdv.md` §2, §7, §8, §9. Validado por [`Manifesto::validar`] no teste.

use cardeal_ledger::PapelConta;
use cardeal_modkit::{
    ContaPadrao, EntradaMenu, Icone, IdModulo, Manifesto, Permissao, Risco, Submodulo,
};

/// O id estável do módulo.
pub const ID: IdModulo = IdModulo::novo("pdv");

const FINANCEIRO: IdModulo = IdModulo::novo("financeiro");
const CLIENTES: IdModulo = IdModulo::novo("clientes");
const ESTOQUE: IdModulo = IdModulo::novo("estoque");
const VENDAS: IdModulo = IdModulo::novo("vendas");

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
        id: "frente_caixa",
        nome: "Frente de Caixa",
        essencial: true,
        depende_de: &[],
    },
    Submodulo {
        id: "sangria",
        nome: "Sangria e Suprimento",
        essencial: true,
        depende_de: &[],
    },
    Submodulo {
        id: "turno",
        nome: "Turno de Caixa",
        essencial: true,
        depende_de: &[],
    },
    Submodulo {
        id: "tef",
        nome: "TEF",
        essencial: false,
        depende_de: &[],
    },
    Submodulo {
        id: "balanca",
        nome: "Balança",
        essencial: false,
        depende_de: &[],
    },
    Submodulo {
        id: "pista",
        nome: "Pista (posto)",
        essencial: false,
        depende_de: &[],
    },
    Submodulo {
        id: "delivery",
        nome: "Delivery",
        essencial: false,
        depende_de: &["frente_caixa"],
    },
];

const PERMISSOES: &[Permissao] = &[
    perm(
        "pdv.venda.editar",
        "Adicionar/alterar item, identificar cliente",
        Risco::Baixo,
        None,
    ),
    perm(
        "pdv.venda.finalizar",
        "Finalizar venda (F2)",
        Risco::Medio,
        None,
    ),
    perm(
        "pdv.venda.ver",
        "Consultar cupons do turno",
        Risco::Baixo,
        None,
    ),
    perm(
        "pdv.item.cancelar",
        "Cancelar item antes de finalizar (F7)",
        Risco::Baixo,
        None,
    ),
    perm(
        "pdv.cupom.cancelar",
        "Cancelar cupom inteiro (F8)",
        Risco::Alto,
        None,
    ),
    perm(
        "pdv.desconto.aplicar",
        "Aplicar desconto (F5)",
        Risco::Medio,
        None,
    ),
    perm(
        "pdv.preco.consultar",
        "Consulta de preço sem abrir venda (F10)",
        Risco::Baixo,
        None,
    ),
    perm(
        "pdv.tef.operar",
        "Iniciar pagamento por TEF",
        Risco::Medio,
        Some("tef"),
    ),
];

const MENU: &[EntradaMenu] = &[menu(
    "pdv.venda",
    "PDV",
    Icone::Caixa,
    5,
    "pdv.venda.editar",
    None,
)];

const CONTAS_REQUERIDAS: &[ContaPadrao] = &[
    ContaPadrao {
        papel: PapelConta::Caixa,
        obrigatoria: true,
    },
    ContaPadrao {
        papel: PapelConta::ReceitaVendas,
        obrigatoria: true,
    },
    ContaPadrao {
        papel: PapelConta::Cmv,
        obrigatoria: true,
    },
    ContaPadrao {
        papel: PapelConta::EstoqueMercadorias,
        obrigatoria: true,
    },
    ContaPadrao {
        papel: PapelConta::CartoesAReceber,
        obrigatoria: true,
    },
    ContaPadrao {
        papel: PapelConta::ValoresEmTransito,
        obrigatoria: true,
    },
    ContaPadrao {
        papel: PapelConta::DescontosConcedidos,
        obrigatoria: false,
    },
];

const EVENTOS_PUBLICADOS: &[&str] = &[
    "pdv.venda_finalizada.v1",
    "pdv.cupom_cancelado.v1",
    "pdv.item_cancelado.v1",
];

/// O manifesto do módulo de PDV.
pub static MANIFESTO: Manifesto = Manifesto {
    id: ID,
    nome: "PDV",
    versao: (0, 1, 0),
    descricao: "Frente de caixa: venda de balcão, sangria/suprimento e turno de caixa.",
    icone: Icone::Caixa,
    depende_de: &[FINANCEIRO, CLIENTES, ESTOQUE, VENDAS],
    melhora_com: &[],
    conflita_com: &[],
    submodulos: SUBMODULOS,
    permissoes: PERMISSOES,
    menu: MENU,
    contas_requeridas: CONTAS_REQUERIDAS,
    eventos_publicados: EVENTOS_PUBLICADOS,
    eventos_assinados: &[],
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
            .expect("o manifesto de pdv deve ser válido");
    }

    #[test]
    fn frente_caixa_sangria_e_turno_sao_essenciais() {
        assert!(MANIFESTO.submodulo_essencial("frente_caixa"));
        assert!(MANIFESTO.submodulo_essencial("sangria"));
        assert!(MANIFESTO.submodulo_essencial("turno"));
        assert!(!MANIFESTO.submodulo_essencial("tef"));
    }

    #[test]
    fn depende_do_nucleo_comercial() {
        assert!(MANIFESTO.depende_de.contains(&FINANCEIRO));
        assert!(MANIFESTO.depende_de.contains(&CLIENTES));
        assert!(MANIFESTO.depende_de.contains(&ESTOQUE));
        assert!(MANIFESTO.depende_de.contains(&VENDAS));
    }

    #[test]
    fn toda_permissao_do_menu_existe() {
        for e in MANIFESTO.menu {
            assert!(MANIFESTO.permissoes.iter().any(|p| p.chave == e.permissao));
        }
    }
}
