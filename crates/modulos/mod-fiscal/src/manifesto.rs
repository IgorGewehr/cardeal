//! O manifesto do módulo `fiscal`.
//!
//! `depende_de: &[EMPRESA]` — o CNPJ do certificado é conferido contra o da empresa, e a UF
//! do emitente sai dos dados fiscais dela (`mod-empresa`).

use cardeal_modkit::{Icone, IdModulo, Manifesto, Permissao, Risco, Submodulo};

/// O id estável do módulo.
pub const ID: IdModulo = IdModulo::novo("fiscal");

const EMPRESA: IdModulo = IdModulo::novo("empresa");
const PDV: IdModulo = IdModulo::novo("pdv");

const PERMISSOES: &[Permissao] = &[
    Permissao {
        chave: "fiscal.ver",
        descricao: "Ver a situação fiscal (certificado, ambiente) e testar a conexão com a SEFAZ",
        risco: Risco::Baixo,
        requer_submodulo: None,
    },
    Permissao {
        chave: "fiscal.emitir",
        descricao: "Emitir e reenviar a NFC-e das vendas",
        risco: Risco::Medio,
        requer_submodulo: None,
    },
    Permissao {
        chave: "fiscal.cancelar",
        descricao: "Cancelar NFC-e autorizada e inutilizar números que não vão ser usados",
        risco: Risco::Alto,
        requer_submodulo: None,
    },
    Permissao {
        chave: "fiscal.configurar",
        descricao:
            "Instalar o certificado digital e trocar o ambiente fiscal (homologação/produção)",
        risco: Risco::Alto,
        requer_submodulo: None,
    },
];

/// O manifesto.
pub static MANIFESTO: Manifesto = Manifesto {
    id: ID,
    nome: "Fiscal",
    versao: (0, 1, 0),
    descricao: "Certificado digital, ambiente e conexão com a SEFAZ pelo sefaz-api.",
    icone: Icone::Nota,
    depende_de: &[EMPRESA],
    melhora_com: &[PDV],
    conflita_com: &[],
    submodulos: &[Submodulo {
        id: "configuracao",
        nome: "Configuração fiscal",
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
