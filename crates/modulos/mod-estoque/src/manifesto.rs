//! O manifesto do módulo de estoque.
//!
//! Ver `docs/modulos/estoque.md` §2, §8, §9. Validado por [`Manifesto::validar`] no teste.

use cardeal_ledger::PapelConta;
use cardeal_modkit::{
    ContaPadrao, EntradaMenu, Icone, IdModulo, Manifesto, Permissao, Risco, Submodulo,
};

/// O id estável do módulo.
pub const ID: IdModulo = IdModulo::novo("estoque");

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
        id: "saldo",
        nome: "Saldo e Movimento",
        essencial: true,
        depende_de: &[],
    },
    Submodulo {
        id: "custo_medio",
        nome: "Custo Médio",
        essencial: true,
        depende_de: &["saldo"],
    },
    Submodulo {
        id: "lote",
        nome: "Lote",
        essencial: false,
        depende_de: &["saldo"],
    },
    Submodulo {
        id: "validade",
        nome: "Validade",
        essencial: false,
        depende_de: &["lote"],
    },
    Submodulo {
        id: "inventario",
        nome: "Inventário",
        essencial: false,
        depende_de: &["saldo"],
    },
    Submodulo {
        id: "multi_local",
        nome: "Múltiplos Locais",
        essencial: false,
        depende_de: &["saldo"],
    },
    Submodulo {
        id: "transferencia",
        nome: "Transferência entre Locais",
        essencial: false,
        depende_de: &["multi_local"],
    },
    Submodulo {
        id: "tanques",
        nome: "Tanques (posto)",
        essencial: false,
        depende_de: &["saldo"],
    },
    Submodulo {
        id: "grade",
        nome: "Grade (cor/tamanho)",
        essencial: false,
        depende_de: &["saldo"],
    },
];

const PERMISSOES: &[Permissao] = &[
    perm(
        "estoque.produto.ver",
        "Consultar produtos",
        Risco::Baixo,
        None,
    ),
    perm(
        "estoque.produto.criar",
        "Cadastrar produto/variação",
        Risco::Baixo,
        None,
    ),
    perm(
        "estoque.produto.editar",
        "Editar produto, código de barras, ponto de pedido",
        Risco::Baixo,
        None,
    ),
    perm(
        "estoque.local.criar",
        "Cadastrar local de estoque",
        Risco::Medio,
        None,
    ),
    perm(
        "estoque.saldo.ver",
        "Ver saldo por local",
        Risco::Baixo,
        None,
    ),
    perm(
        "estoque.movimento.ver",
        "Ver histórico de movimentos",
        Risco::Baixo,
        None,
    ),
    perm(
        "estoque.movimento.entrada",
        "Registrar entrada manual",
        Risco::Baixo,
        None,
    ),
    perm(
        "estoque.movimento.saida",
        "Registrar saída manual",
        Risco::Medio,
        None,
    ),
    perm(
        "estoque.movimento.ajustar",
        "Ajuste de saldo fora de inventário",
        Risco::Alto,
        None,
    ),
    perm(
        "estoque.movimento.perda",
        "Registrar perda",
        Risco::Medio,
        None,
    ),
    perm(
        "estoque.transferencia.ver",
        "Ver transferências",
        Risco::Baixo,
        Some("transferencia"),
    ),
    perm(
        "estoque.transferencia.criar",
        "Criar transferência entre locais",
        Risco::Medio,
        Some("transferencia"),
    ),
    perm(
        "estoque.transferencia.confirmar",
        "Confirmar recebimento de transferência",
        Risco::Baixo,
        Some("transferencia"),
    ),
    perm(
        "estoque.inventario.ver",
        "Ver inventários",
        Risco::Baixo,
        Some("inventario"),
    ),
    perm(
        "estoque.inventario.criar",
        "Criar inventário",
        Risco::Baixo,
        Some("inventario"),
    ),
    perm(
        "estoque.inventario.contar",
        "Registrar contagem",
        Risco::Baixo,
        Some("inventario"),
    ),
    perm(
        "estoque.inventario.encerrar",
        "Encerrar inventário e gerar ajustes",
        Risco::Alto,
        Some("inventario"),
    ),
    perm(
        "estoque.lote.ver",
        "Ver lotes e validade",
        Risco::Baixo,
        Some("lote"),
    ),
    perm("estoque.abc.ver", "Ver curva ABC", Risco::Baixo, None),
    perm(
        "estoque.compra_sugerida.ver",
        "Ver produtos abaixo do ponto de pedido",
        Risco::Baixo,
        None,
    ),
    perm(
        "estoque.tributario.editar",
        "Editar perfil tributário",
        Risco::Alto,
        None,
    ),
];

const MENU: &[EntradaMenu] = &[
    menu(
        "estoque.produtos",
        "Produtos",
        Icone::Estoque,
        30,
        "estoque.produto.ver",
        None,
    ),
    menu(
        "estoque.saldos",
        "Saldos",
        Icone::Estoque,
        31,
        "estoque.saldo.ver",
        None,
    ),
    menu(
        "estoque.transferencias",
        "Transferências",
        Icone::Estoque,
        32,
        "estoque.transferencia.ver",
        Some("transferencia"),
    ),
    menu(
        "estoque.inventarios",
        "Inventário",
        Icone::Estoque,
        33,
        "estoque.inventario.ver",
        Some("inventario"),
    ),
    menu(
        "estoque.abc",
        "Curva ABC",
        Icone::Grafico,
        34,
        "estoque.abc.ver",
        None,
    ),
];

const CONTAS_REQUERIDAS: &[ContaPadrao] = &[
    ContaPadrao {
        papel: PapelConta::EstoqueMercadorias,
        obrigatoria: true,
    },
    ContaPadrao {
        papel: PapelConta::Cmv,
        obrigatoria: true,
    },
    ContaPadrao {
        papel: PapelConta::Perdas,
        obrigatoria: false,
    },
    ContaPadrao {
        papel: PapelConta::OutrasReceitas,
        obrigatoria: false,
    },
    ContaPadrao {
        papel: PapelConta::EstoqueMateriaPrima,
        obrigatoria: false,
    },
    ContaPadrao {
        papel: PapelConta::EstoqueEmProcesso,
        obrigatoria: false,
    },
    ContaPadrao {
        papel: PapelConta::EstoqueAcabado,
        obrigatoria: false,
    },
];

const EVENTOS_PUBLICADOS: &[&str] = &[
    "estoque.produto_criado.v1",
    "estoque.saldo_alterado.v1",
    "estoque.abaixo_ponto_pedido.v1",
    "estoque.inventario_encerrado.v1",
    "estoque.lote_vencido.v1",
];

const EVENTOS_ASSINADOS: &[&str] = &["compras.nota_confirmada.v1", "pdv.venda_finalizada.v1"];

/// O manifesto do módulo de estoque.
pub static MANIFESTO: Manifesto = Manifesto {
    id: ID,
    nome: "Estoque",
    versao: (0, 1, 0),
    descricao: "Produtos, saldos por local, custo médio, lotes, validade e inventário.",
    icone: Icone::Estoque,
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
            .expect("o manifesto de estoque deve ser válido");
    }

    #[test]
    fn saldo_e_custo_medio_sao_essenciais() {
        assert!(MANIFESTO.submodulo_essencial("saldo"));
        assert!(MANIFESTO.submodulo_essencial("custo_medio"));
        assert!(!MANIFESTO.submodulo_essencial("lote"));
    }

    #[test]
    fn toda_permissao_do_menu_existe() {
        for e in MANIFESTO.menu {
            assert!(MANIFESTO.permissoes.iter().any(|p| p.chave == e.permissao));
        }
    }
}
