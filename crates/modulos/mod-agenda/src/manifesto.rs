//! O manifesto do módulo de agenda — identidade, submódulos, permissões, menu e eventos.
//!
//! Dado estático (`&'static`), lido pelo registro do motor no boot. Ver
//! `docs/modulos/agenda.md` §2, §8 e §9. A consistência interna é provada por
//! [`Manifesto::validar`] no teste ao fim deste arquivo.
//!
//! `depende_de: &[CLIENTES]` — `agenda_compromisso.cliente` referencia `clientes_pessoa(id)`
//! de verdade (ver `src/migracoes.rs`); a agenda precisa do cadastro de clientes já ativo.

use cardeal_modkit::{EntradaMenu, Icone, IdModulo, Manifesto, Permissao, Risco, Submodulo};

/// O id estável do módulo.
pub const ID: IdModulo = IdModulo::novo("agenda");

const CLIENTES: IdModulo = IdModulo::novo("clientes");

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
        id: "compromisso",
        nome: "Compromissos",
        essencial: true,
        depende_de: &[],
    },
    Submodulo {
        id: "recurso",
        nome: "Recursos e Disponibilidade",
        essencial: false,
        depende_de: &["compromisso"],
    },
    Submodulo {
        id: "lembrete",
        nome: "Lembretes",
        essencial: false,
        depende_de: &["compromisso"],
    },
];

const PERMISSOES: &[Permissao] = &[
    perm(
        "agenda.recurso.gerenciar",
        "Cadastrar recurso e disponibilidade",
        Risco::Medio,
        Some("recurso"),
    ),
    perm(
        "agenda.compromisso.ver",
        "Consultar agenda",
        Risco::Baixo,
        None,
    ),
    perm(
        "agenda.compromisso.criar",
        "Criar compromisso",
        Risco::Baixo,
        None,
    ),
    perm(
        "agenda.compromisso.confirmar",
        "Confirmar/iniciar/concluir compromisso",
        Risco::Baixo,
        None,
    ),
    perm(
        "agenda.compromisso.cancelar",
        "Cancelar compromisso",
        Risco::Medio,
        None,
    ),
];

const MENU: &[EntradaMenu] = &[menu(
    "agenda.compromissos",
    "Agenda",
    Icone::Agenda,
    20,
    "agenda.compromisso.ver",
    None,
)];

const EVENTOS_PUBLICADOS: &[&str] = &[
    "agenda.compromisso_criado.v1",
    "agenda.compromisso_confirmado.v1",
    "agenda.compromisso_cancelado.v1",
    "agenda.conflito_detectado.v1",
    "agenda.lembrete_disparado.v1",
];

/// O manifesto do módulo de agenda.
pub static MANIFESTO: Manifesto = Manifesto {
    id: ID,
    nome: "Agenda",
    versao: (0, 1, 0),
    descricao: "Compromissos e recursos — sala, técnico, equipamento — com conflito \
                detectado na hora de marcar.",
    icone: Icone::Agenda,
    depende_de: &[CLIENTES],
    melhora_com: &[],
    conflita_com: &[],
    submodulos: SUBMODULOS,
    permissoes: PERMISSOES,
    menu: MENU,
    contas_requeridas: &[],
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
            .expect("o manifesto da agenda deve ser válido");
    }

    #[test]
    fn compromisso_e_essencial() {
        assert!(MANIFESTO.submodulo_essencial("compromisso"));
        assert!(!MANIFESTO.submodulo_essencial("recurso"));
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
