//! O manifesto do módulo `empresa`.

use cardeal_modkit::{Icone, IdModulo, Manifesto, Permissao, Risco, Submodulo};

/// O id estável do módulo.
pub const ID: IdModulo = IdModulo::novo("empresa");

const PERMISSOES: &[Permissao] = &[
    Permissao {
        chave: "empresa.dados.ver",
        descricao: "Ver dados cadastrais e identidade visual da empresa",
        risco: Risco::Baixo,
        requer_submodulo: None,
    },
    Permissao {
        chave: "empresa.dados.editar",
        descricao: "Alterar dados cadastrais, contato e logo da empresa",
        risco: Risco::Alto,
        requer_submodulo: None,
    },
    Permissao {
        chave: "empresa.usuarios.gerenciar",
        descricao: "Adicionar e desativar usuários da empresa",
        risco: Risco::Alto,
        requer_submodulo: None,
    },
    Permissao {
        chave: "empresa.usuarios.ver",
        descricao: "Ver usuários e papéis",
        risco: Risco::Baixo,
        requer_submodulo: None,
    },
];

/// O manifesto.
pub static MANIFESTO: Manifesto = Manifesto {
    id: ID,
    nome: "Empresa",
    versao: (0, 1, 0),
    descricao: "Dados da própria empresa, identidade visual, usuários e papéis.",
    icone: Icone::Config,
    depende_de: &[],
    melhora_com: &[],
    conflita_com: &[],
    submodulos: &[Submodulo {
        id: "cadastro",
        nome: "Cadastro da empresa",
        essencial: true,
        depende_de: &[],
    }],
    permissoes: PERMISSOES,
    menu: &[],
    contas_requeridas: &[],
    eventos_publicados: &[],
    eventos_assinados: &[],
};

#[cfg(test)]
mod testes {
    #[test]
    fn manifesto_valido() {
        super::MANIFESTO.validar().unwrap();
    }
}
