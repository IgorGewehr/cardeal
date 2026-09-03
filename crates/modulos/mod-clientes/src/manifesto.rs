//! O manifesto do módulo de cadastro de pessoas.
//!
//! Ver `docs/modulos/clientes.md` §2, §8, §9. Validado por [`Manifesto::validar`] no teste.

use cardeal_modkit::{
    ContaPadrao, EntradaMenu, Icone, IdModulo, Manifesto, Permissao, Risco, Submodulo,
};

/// O id estável do módulo.
pub const ID: IdModulo = IdModulo::novo("clientes");

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
        id: "cadastro",
        nome: "Cadastro de Pessoas",
        essencial: true,
        depende_de: &[],
    },
    Submodulo {
        id: "credito",
        nome: "Limite de Crédito",
        essencial: false,
        depende_de: &["cadastro"],
    },
    Submodulo {
        id: "dedup",
        nome: "Deduplicação Assistida",
        essencial: false,
        depende_de: &["cadastro"],
    },
    Submodulo {
        id: "carteira",
        nome: "Carteira de Vendedor",
        essencial: false,
        depende_de: &["cadastro"],
    },
    Submodulo {
        id: "score",
        nome: "Score de Pagamento",
        essencial: false,
        depende_de: &["credito"],
    },
];

const PERMISSOES: &[Permissao] = &[
    perm(
        "clientes.pessoa.ver",
        "Consultar cadastro de pessoas",
        Risco::Baixo,
        None,
    ),
    perm("clientes.pessoa.criar", "Criar pessoa", Risco::Baixo, None),
    perm(
        "clientes.pessoa.editar",
        "Editar dados cadastrais",
        Risco::Baixo,
        None,
    ),
    perm(
        "clientes.pessoa.consultar_sefaz",
        "Consultar cadastro na SEFAZ",
        Risco::Baixo,
        None,
    ),
    perm(
        "clientes.papel.gerenciar",
        "Adicionar/remover papel",
        Risco::Medio,
        None,
    ),
    perm(
        "clientes.duplicidade.mesclar",
        "Mesclar cadastros duplicados",
        Risco::Alto,
        Some("dedup"),
    ),
    perm(
        "clientes.credito.ver",
        "Ver limite e score de crédito",
        Risco::Baixo,
        Some("credito"),
    ),
    perm(
        "clientes.credito.definir_limite",
        "Definir limite de crédito",
        Risco::Medio,
        Some("credito"),
    ),
    perm(
        "clientes.credito.liberar",
        "Liberar cliente bloqueado por crédito",
        Risco::Alto,
        Some("credito"),
    ),
    perm(
        "clientes.carteira.ver",
        "Ver carteira de vendedor",
        Risco::Baixo,
        Some("carteira"),
    ),
    perm(
        "clientes.carteira.gerenciar",
        "Definir vendedor responsável",
        Risco::Medio,
        Some("carteira"),
    ),
    perm(
        "clientes.tabela_preco.vincular",
        "Vincular tabela de preço ao cliente",
        Risco::Baixo,
        None,
    ),
    perm(
        "clientes.lgpd.anonimizar",
        "Anonimizar dados pessoais",
        Risco::Critico,
        None,
    ),
    perm(
        "clientes.lgpd.exportar",
        "Exportar dados do titular",
        Risco::Alto,
        None,
    ),
];

const MENU: &[EntradaMenu] = &[
    menu(
        "clientes.pessoas",
        "Pessoas",
        Icone::Pessoas,
        20,
        "clientes.pessoa.ver",
        None,
    ),
    menu(
        "clientes.duplicidades",
        "Duplicidades",
        Icone::Alerta,
        21,
        "clientes.duplicidade.mesclar",
        Some("dedup"),
    ),
    menu(
        "clientes.credito",
        "Crédito",
        Icone::Dinheiro,
        22,
        "clientes.credito.ver",
        Some("credito"),
    ),
    menu(
        "clientes.carteiras",
        "Carteiras",
        Icone::Pessoas,
        23,
        "clientes.carteira.ver",
        Some("carteira"),
    ),
];

const EVENTOS_PUBLICADOS: &[&str] = &[
    "clientes.pessoa_criada.v1",
    "clientes.papel_adicionado.v1",
    "clientes.credito_bloqueado.v1",
    "clientes.credito_liberado.v1",
    "clientes.duplicidade_sugerida.v1",
    "clientes.cadastro_mesclado.v1",
    "clientes.pessoa_anonimizada.v1",
];

const EVENTOS_ASSINADOS: &[&str] = &[
    "financeiro.parcela_baixada.v1",
    "financeiro.baixa_estornada.v1",
    "financeiro.titulo_lancado.v1",
];

/// O manifesto do módulo de clientes.
pub static MANIFESTO: Manifesto = Manifesto {
    id: ID,
    nome: "Clientes",
    versao: (0, 1, 0),
    descricao:
        "Cadastro único de pessoas: cliente, fornecedor, transportadora, funcionário, sócio.",
    icone: Icone::Pessoas,
    depende_de: &[],
    melhora_com: &[],
    conflita_com: &[],
    submodulos: SUBMODULOS,
    permissoes: PERMISSOES,
    menu: MENU,
    contas_requeridas: &[] as &[ContaPadrao],
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
            .expect("o manifesto de clientes deve ser válido");
    }

    #[test]
    fn so_o_cadastro_e_essencial() {
        assert!(MANIFESTO.submodulo_essencial("cadastro"));
        for sub in ["credito", "dedup", "carteira", "score"] {
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
