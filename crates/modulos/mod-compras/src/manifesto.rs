//! O manifesto do módulo de compras — identidade, submódulos, permissões, menu e contas.
//!
//! Ver `docs/modulos/compras.md` §2, §8 e §9. A consistência interna é provada por
//! [`Manifesto::validar`] no teste ao fim deste arquivo.

use cardeal_ledger::PapelConta;
use cardeal_modkit::{
    ContaPadrao, EntradaMenu, Icone, IdModulo, Manifesto, Permissao, Risco, Submodulo,
};

/// O id estável do módulo.
pub const ID: IdModulo = IdModulo::novo("compras");

/// Dependência dura: `resolver_fornecedor`, `confirmar_entrada_comum` e o rateio chamam
/// direto `pub fn` de clientes/estoque/financeiro na mesma transação (ver `src/lib.rs`) —
/// sem essas três ativas não há onde gravar fornecedor, saldo de estoque ou título a pagar.
const DEPENDE_DE: &[IdModulo] = &[
    IdModulo::novo("clientes"),
    IdModulo::novo("estoque"),
    IdModulo::novo("financeiro"),
];

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
        id: "pedido_compra",
        nome: "Pedido de Compra",
        essencial: true,
        depende_de: &[],
    },
    Submodulo {
        id: "cotacao",
        nome: "Cotação",
        essencial: false,
        depende_de: &["pedido_compra"],
    },
    Submodulo {
        id: "entrada_xml",
        nome: "Entrada por XML",
        essencial: false,
        depende_de: &["pedido_compra"],
    },
    Submodulo {
        id: "casamento_produto",
        nome: "Casamento Automático",
        essencial: false,
        depende_de: &["entrada_xml"],
    },
    Submodulo {
        id: "rateio",
        nome: "Rateio de Despesas",
        essencial: false,
        depende_de: &["entrada_xml"],
    },
    Submodulo {
        id: "devolucao",
        nome: "Devolução ao Fornecedor",
        essencial: false,
        depende_de: &["entrada_xml"],
    },
];

const PERMISSOES: &[Permissao] = &[
    perm(
        "compras.cotacao.ver",
        "Consultar cotações",
        Risco::Baixo,
        Some("cotacao"),
    ),
    perm(
        "compras.cotacao.criar",
        "Criar cotação",
        Risco::Baixo,
        Some("cotacao"),
    ),
    perm(
        "compras.cotacao.editar",
        "Registrar proposta de fornecedor",
        Risco::Baixo,
        Some("cotacao"),
    ),
    perm(
        "compras.cotacao.decidir",
        "Decidir cotação",
        Risco::Medio,
        Some("cotacao"),
    ),
    perm(
        "compras.pedido.ver",
        "Consultar pedidos de compra",
        Risco::Baixo,
        None,
    ),
    perm(
        "compras.pedido.criar",
        "Criar pedido de compra",
        Risco::Baixo,
        None,
    ),
    perm(
        "compras.entrada.ver",
        "Consultar notas de entrada",
        Risco::Baixo,
        None,
    ),
    perm(
        "compras.entrada.importar",
        "Importar nota de entrada",
        Risco::Baixo,
        None,
    ),
    perm(
        "compras.entrada.conferir",
        "Conferir nota (vincular produto, ratear despesas)",
        Risco::Baixo,
        None,
    ),
    perm(
        "compras.entrada.confirmar",
        "Confirmar entrada (gera estoque e título)",
        Risco::Alto,
        None,
    ),
    perm(
        "compras.entrada.preferencias",
        "Definir preferências de importação",
        Risco::Medio,
        None,
    ),
    perm(
        "compras.devolucao.criar",
        "Devolver ao fornecedor",
        Risco::Alto,
        Some("devolucao"),
    ),
];

const MENU: &[EntradaMenu] = &[
    menu(
        "compras.entradas",
        "Notas de Entrada",
        Icone::Carrinho,
        40,
        "compras.entrada.ver",
        None,
    ),
    menu(
        "compras.pedidos",
        "Pedidos de Compra",
        Icone::Carrinho,
        41,
        "compras.pedido.ver",
        None,
    ),
];

const CONTAS_REQUERIDAS: &[ContaPadrao] = &[
    ContaPadrao {
        papel: PapelConta::Fornecedores,
        obrigatoria: true,
    },
    ContaPadrao {
        papel: PapelConta::EstoqueMercadorias,
        obrigatoria: true,
    },
];

const EVENTOS_PUBLICADOS: &[&str] = &[
    "compras.nota_confirmada.v1",
    "compras.entrada_a_conferir.v1",
    "compras.devolucao_concluida.v1",
];

const EVENTOS_ASSINADOS: &[&str] = &["estoque.abaixo_ponto_pedido.v1"];

/// O manifesto do módulo de compras.
pub static MANIFESTO: Manifesto = Manifesto {
    id: ID,
    nome: "Compras",
    versao: (0, 1, 0),
    descricao: "Cotação, pedido de compra, entrada por XML e rateio de despesas.",
    icone: Icone::Carrinho,
    depende_de: DEPENDE_DE,
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
            .expect("o manifesto de compras deve ser válido");
    }

    #[test]
    fn so_o_pedido_de_compra_e_essencial() {
        assert!(MANIFESTO.submodulo_essencial("pedido_compra"));
        for sub in [
            "cotacao",
            "entrada_xml",
            "casamento_produto",
            "rateio",
            "devolucao",
        ] {
            assert!(!MANIFESTO.submodulo_essencial(sub));
        }
    }

    #[test]
    fn toda_permissao_do_menu_existe() {
        for e in MANIFESTO.menu {
            assert!(MANIFESTO.permissoes.iter().any(|p| p.chave == e.permissao));
        }
    }
}
