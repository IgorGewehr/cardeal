//! O manifesto do módulo de vendas.
//!
//! Ver `docs/modulos/vendas.md` §2, §7, §8, §9. Validado por [`Manifesto::validar`] no teste.

use cardeal_ledger::PapelConta;
use cardeal_modkit::{
    ContaPadrao, EntradaMenu, Icone, IdModulo, Manifesto, Permissao, Risco, Submodulo,
};

/// O id estável do módulo.
pub const ID: IdModulo = IdModulo::novo("vendas");

const FINANCEIRO: IdModulo = IdModulo::novo("financeiro");
const CLIENTES: IdModulo = IdModulo::novo("clientes");
const ESTOQUE: IdModulo = IdModulo::novo("estoque");

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
        id: "pedido",
        nome: "Pedido",
        essencial: true,
        depende_de: &[],
    },
    Submodulo {
        id: "tabela_preco",
        nome: "Tabela de Preço",
        essencial: true,
        depende_de: &[],
    },
    Submodulo {
        id: "orcamento",
        nome: "Orçamento",
        essencial: false,
        depende_de: &["pedido"],
    },
    Submodulo {
        id: "comissao",
        nome: "Comissão",
        essencial: false,
        depende_de: &["pedido"],
    },
    Submodulo {
        id: "entrega",
        nome: "Entrega",
        essencial: false,
        depende_de: &["pedido"],
    },
    Submodulo {
        id: "devolucao",
        nome: "Devolução",
        essencial: false,
        depende_de: &["pedido"],
    },
    Submodulo {
        id: "contrato_recorrente",
        nome: "Contrato Recorrente",
        essencial: false,
        depende_de: &["pedido"],
    },
];

const PERMISSOES: &[Permissao] = &[
    perm(
        "vendas.orcamento.ver",
        "Consultar orçamentos",
        Risco::Baixo,
        Some("orcamento"),
    ),
    perm(
        "vendas.orcamento.criar",
        "Criar orçamento",
        Risco::Baixo,
        Some("orcamento"),
    ),
    perm(
        "vendas.orcamento.aprovar",
        "Aprovar orçamento",
        Risco::Baixo,
        Some("orcamento"),
    ),
    perm("vendas.pedido.ver", "Consultar pedidos", Risco::Baixo, None),
    perm("vendas.pedido.criar", "Criar pedido", Risco::Baixo, None),
    perm(
        "vendas.pedido.editar",
        "Editar itens do pedido",
        Risco::Baixo,
        None,
    ),
    perm(
        "vendas.pedido.descontar",
        "Aplicar desconto",
        Risco::Medio,
        None,
    ),
    perm(
        "vendas.pedido.confirmar",
        "Confirmar pedido (reserva estoque)",
        Risco::Medio,
        None,
    ),
    perm("vendas.pedido.faturar", "Faturar pedido", Risco::Alto, None),
    perm(
        "vendas.pedido.cancelar",
        "Cancelar pedido não faturado",
        Risco::Medio,
        None,
    ),
    perm(
        "vendas.entrega.registrar",
        "Registrar entrega",
        Risco::Baixo,
        Some("entrega"),
    ),
    perm(
        "vendas.devolucao.ver",
        "Ver devoluções",
        Risco::Baixo,
        Some("devolucao"),
    ),
    perm(
        "vendas.devolucao.solicitar",
        "Solicitar devolução",
        Risco::Baixo,
        Some("devolucao"),
    ),
    perm(
        "vendas.devolucao.aprovar",
        "Aprovar devolução",
        Risco::Alto,
        Some("devolucao"),
    ),
    perm(
        "vendas.devolucao.concluir",
        "Concluir devolução (estorno)",
        Risco::Alto,
        Some("devolucao"),
    ),
    perm(
        "vendas.tabela_preco.ver",
        "Consultar tabelas de preço",
        Risco::Baixo,
        None,
    ),
    perm(
        "vendas.tabela_preco.criar",
        "Criar tabela de preço",
        Risco::Medio,
        None,
    ),
    perm(
        "vendas.tabela_preco.editar",
        "Editar regra de preço",
        Risco::Medio,
        None,
    ),
    perm(
        "vendas.comissao.ver",
        "Ver apuração de comissão",
        Risco::Baixo,
        Some("comissao"),
    ),
    perm(
        "vendas.comissao.pagar",
        "Pagar comissão",
        Risco::Medio,
        Some("comissao"),
    ),
    perm(
        "vendas.contrato_recorrente.criar",
        "Criar contrato recorrente",
        Risco::Medio,
        Some("contrato_recorrente"),
    ),
];

const MENU: &[EntradaMenu] = &[
    menu(
        "vendas.orcamentos",
        "Orçamentos",
        Icone::Carrinho,
        40,
        "vendas.orcamento.ver",
        Some("orcamento"),
    ),
    menu(
        "vendas.pedidos",
        "Pedidos",
        Icone::Carrinho,
        41,
        "vendas.pedido.ver",
        None,
    ),
    menu(
        "vendas.devolucoes",
        "Devoluções",
        Icone::Carrinho,
        42,
        "vendas.devolucao.ver",
        Some("devolucao"),
    ),
    menu(
        "vendas.tabelas_preco",
        "Tabelas de Preço",
        Icone::Dinheiro,
        43,
        "vendas.tabela_preco.ver",
        None,
    ),
    menu(
        "vendas.comissoes",
        "Comissões",
        Icone::Dinheiro,
        44,
        "vendas.comissao.ver",
        Some("comissao"),
    ),
];

const CONTAS_REQUERIDAS: &[ContaPadrao] = &[
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
        papel: PapelConta::ClientesAReceber,
        obrigatoria: true,
    },
    ContaPadrao {
        papel: PapelConta::DescontosConcedidos,
        obrigatoria: false,
    },
    ContaPadrao {
        papel: PapelConta::DevolucoesVenda,
        obrigatoria: false,
    },
    ContaPadrao {
        papel: PapelConta::DespesaComercial,
        obrigatoria: false,
    },
    ContaPadrao {
        papel: PapelConta::Caixa,
        obrigatoria: false,
    },
];

const EVENTOS_PUBLICADOS: &[&str] = &[
    "vendas.orcamento_aprovado.v1",
    "vendas.pedido_confirmado.v1",
    "vendas.pedido_faturado.v1",
    "vendas.pedido_cancelado.v1",
    "vendas.devolucao_concluida.v1",
    "vendas.comissao_apurada.v1",
];

const EVENTOS_ASSINADOS: &[&str] = &["estoque.saldo_alterado.v1", "clientes.credito_bloqueado.v1"];

/// O manifesto do módulo de vendas.
pub static MANIFESTO: Manifesto = Manifesto {
    id: ID,
    nome: "Vendas",
    versao: (0, 1, 0),
    descricao: "Orçamento, pedido, preço por tabela, comissão, entrega e devolução.",
    icone: Icone::Carrinho,
    depende_de: &[FINANCEIRO, CLIENTES, ESTOQUE],
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
            .expect("o manifesto de vendas deve ser válido");
    }

    #[test]
    fn pedido_e_tabela_preco_sao_essenciais() {
        assert!(MANIFESTO.submodulo_essencial("pedido"));
        assert!(MANIFESTO.submodulo_essencial("tabela_preco"));
        assert!(!MANIFESTO.submodulo_essencial("orcamento"));
    }

    #[test]
    fn depende_do_nucleo_comercial() {
        assert!(MANIFESTO.depende_de.contains(&FINANCEIRO));
        assert!(MANIFESTO.depende_de.contains(&CLIENTES));
        assert!(MANIFESTO.depende_de.contains(&ESTOQUE));
    }

    #[test]
    fn toda_permissao_do_menu_existe() {
        for e in MANIFESTO.menu {
            assert!(MANIFESTO.permissoes.iter().any(|p| p.chave == e.permissao));
        }
    }
}
