//! O manifesto do módulo de ordens de serviço — identidade, submódulos, permissões, menu e
//! contas.
//!
//! Ver `docs/modulos/os.md` §2, §8 e §9. A consistência interna é provada por
//! [`Manifesto::validar`] no teste ao fim deste arquivo.

use cardeal_ledger::PapelConta;
use cardeal_modkit::{
    ContaPadrao, EntradaMenu, Icone, IdModulo, Manifesto, Permissao, Risco, Submodulo,
};

/// O id estável do módulo.
pub const ID: IdModulo = IdModulo::novo("os");

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
        id: "ordem",
        nome: "Ordem de Serviço",
        essencial: true,
        depende_de: &[],
    },
    Submodulo {
        id: "laudo",
        nome: "Laudo Técnico",
        essencial: false,
        depende_de: &["ordem"],
    },
    Submodulo {
        id: "garantia",
        nome: "Garantia e Reincidência",
        essencial: false,
        depende_de: &["ordem"],
    },
];

const PERMISSOES: &[Permissao] = &[
    perm(
        "os.ordem.ver",
        "Consultar ordens de serviço",
        Risco::Baixo,
        None,
    ),
    perm(
        "os.ordem.criar",
        "Abrir ordem de serviço",
        Risco::Baixo,
        None,
    ),
    perm(
        "os.laudo.registrar",
        "Registrar laudo técnico",
        Risco::Baixo,
        Some("laudo"),
    ),
    perm(
        "os.orcamento.montar",
        "Montar orçamento (peça/mão de obra)",
        Risco::Baixo,
        None,
    ),
    perm(
        "os.orcamento.enviar",
        "Enviar orçamento para aprovação",
        Risco::Baixo,
        None,
    ),
    perm(
        "os.orcamento.aprovar",
        "Registrar aprovação/reprovação do cliente",
        Risco::Medio,
        None,
    ),
    perm(
        "os.execucao.iniciar",
        "Iniciar execução",
        Risco::Baixo,
        None,
    ),
    perm(
        "os.peca.aplicar",
        "Aplicar peça (consome estoque)",
        Risco::Medio,
        None,
    ),
    perm(
        "os.execucao.registrar_mao_de_obra",
        "Registrar mão de obra",
        Risco::Baixo,
        None,
    ),
    perm(
        "os.execucao.concluir",
        "Concluir execução",
        Risco::Baixo,
        None,
    ),
    perm("os.faturar", "Faturar ordem de serviço", Risco::Alto, None),
    perm(
        "os.ordem.cancelar",
        "Cancelar ordem de serviço",
        Risco::Medio,
        None,
    ),
    perm(
        "os.garantia.acionar",
        "Acionar garantia",
        Risco::Medio,
        Some("garantia"),
    ),
    perm(
        "os.garantia.ver",
        "Ver relatório de reincidência",
        Risco::Baixo,
        Some("garantia"),
    ),
];

const MENU: &[EntradaMenu] = &[menu(
    "os.ordens",
    "Ordens de Serviço",
    Icone::Ferramenta,
    30,
    "os.ordem.ver",
    None,
)];

const CONTAS_REQUERIDAS: &[ContaPadrao] = &[
    ContaPadrao {
        papel: PapelConta::ClientesAReceber,
        obrigatoria: true,
    },
    ContaPadrao {
        papel: PapelConta::ReceitaServicos,
        obrigatoria: true,
    },
    ContaPadrao {
        papel: PapelConta::CustoServico,
        obrigatoria: false,
    },
    ContaPadrao {
        papel: PapelConta::EstoqueMercadorias,
        obrigatoria: false,
    },
];

const EVENTOS_PUBLICADOS: &[&str] = &[
    "os.ordem_aberta.v1",
    "os.orcamento_aprovado.v1",
    "os.ordem_faturada.v1",
    "os.ordem_cancelada.v1",
    "os.garantia_acionada.v1",
];

const EVENTOS_ASSINADOS: &[&str] = &["agenda.conflito_detectado.v1"];

/// O manifesto do módulo de ordens de serviço.
pub static MANIFESTO: Manifesto = Manifesto {
    id: ID,
    nome: "Ordens de Serviço",
    versao: (0, 1, 0),
    descricao: "Abertura, laudo, orçamento, execução, garantia e faturamento de OS.",
    icone: Icone::Ferramenta,
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
            .expect("o manifesto de os deve ser válido");
    }

    #[test]
    fn so_a_ordem_e_essencial() {
        assert!(MANIFESTO.submodulo_essencial("ordem"));
        for sub in ["laudo", "garantia"] {
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
