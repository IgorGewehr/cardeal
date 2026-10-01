//! Os dados que as consultas devolvem.

use cardeal_kernel::Id;
use serde::{Deserialize, Serialize};

/// Teto da logo da empresa: 512 KB. Ela viaja em todo PDF e, no cliente web, pela rede.
pub const TETO_LOGO_BYTES: usize = 512 * 1024;

/// Dados cadastrais da empresa, para a tela de Configurações.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmpresaResumo {
    /// Razão social.
    pub razao_social: String,
    /// Nome fantasia.
    pub nome_fantasia: String,
    /// CNPJ (não editável pela tela — muda por processo formal).
    pub cnpj: String,
    /// Regime tributário (`MEI`/`SimplesNacional`/`LucroPresumido`/`LucroReal`).
    pub regime: String,
    /// Perfil de operação.
    pub perfil: String,
}

/// A identidade visual da empresa para documentos (orçamento em PDF etc.).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentidadeVisual {
    /// Razão social.
    pub razao_social: String,
    /// Nome fantasia.
    pub nome_fantasia: String,
    /// CNPJ.
    pub cnpj: String,
    /// Endereço de exibição (uma linha).
    pub endereco: String,
    /// Telefone.
    pub telefone: String,
    /// E-mail.
    pub email: String,
    /// Site.
    pub site: String,
    /// Bytes do PNG da logo, se cadastrada.
    pub logo_png: Option<Vec<u8>>,
}

/// Um usuário, resumido para a lista de Configurações.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsuarioResumo {
    /// Identidade.
    pub id: Id,
    /// Login.
    pub login: String,
    /// Nome.
    pub nome: String,
    /// Se está ativo.
    pub ativo: bool,
}

/// Um papel, resumido para a lista de Configurações.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PapelResumo {
    /// Nome.
    pub nome: String,
    /// Descrição.
    pub descricao: String,
    /// Papel de fábrica (imutável).
    pub sistema: bool,
    /// Quantas permissões concede.
    pub permissoes: usize,
}
