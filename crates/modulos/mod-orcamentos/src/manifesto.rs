//! O manifesto do módulo de orçamentos — identidade, submódulo, permissões.
//!
//! **Sem `EntradaMenu`**: os orçamentos aparecem como a 2ª aba da tela de Ordens de Serviço,
//! não como uma área própria da sidebar.

use cardeal_modkit::{
    ContaPadrao, EntradaMenu, Icone, IdModulo, Manifesto, Permissao, Risco, Submodulo,
};

/// O id estável do módulo.
pub const ID: IdModulo = IdModulo::novo("orcamentos");

const fn perm(chave: &'static str, descricao: &'static str, risco: Risco) -> Permissao {
    Permissao {
        chave,
        descricao,
        risco,
        requer_submodulo: None,
    }
}

const SUBMODULOS: &[Submodulo] = &[Submodulo {
    id: "orcamento",
    nome: "Orçamento comercial",
    essencial: true,
    depende_de: &[],
}];

const PERMISSOES: &[Permissao] = &[
    perm("orcamentos.orcamento.ver", "Consultar orçamentos", Risco::Baixo),
    perm("orcamentos.orcamento.criar", "Criar orçamento", Risco::Baixo),
    perm("orcamentos.orcamento.editar", "Editar orçamento (cabeçalho e itens)", Risco::Baixo),
    perm("orcamentos.orcamento.enviar", "Enviar orçamento ao cliente", Risco::Baixo),
    perm(
        "orcamentos.orcamento.decidir",
        "Registrar aprovação/recusa do cliente",
        Risco::Medio,
    ),
    perm("orcamentos.orcamento.cancelar", "Cancelar orçamento", Risco::Baixo),
    perm(
        "orcamentos.orcamento.converter",
        "Converter orçamento aprovado em ordem de serviço",
        Risco::Medio,
    ),
];

const MENU: &[EntradaMenu] = &[];
const CONTAS_REQUERIDAS: &[ContaPadrao] = &[];
const EVENTOS_PUBLICADOS: &[&str] = &[
    "orcamentos.orcamento_aprovado.v1",
    "orcamentos.orcamento_convertido.v1",
];
const EVENTOS_ASSINADOS: &[&str] = &[];
const MELHORA_COM: &[IdModulo] = &[IdModulo::novo("os")];

/// O manifesto do módulo de orçamentos.
pub static MANIFESTO: Manifesto = Manifesto {
    id: ID,
    nome: "Orçamentos",
    versao: (0, 1, 0),
    descricao: "Orçamentos comerciais: documento profissional, filtros, decisão e conversão em OS.",
    icone: Icone::Nota,
    depende_de: &[],
    melhora_com: MELHORA_COM,
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
        MANIFESTO.validar().expect("o manifesto de orcamentos deve ser válido");
    }
}
