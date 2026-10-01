//! As rotas — montadas aqui para cliente e servidor nunca divergirem num caractere.

use cardeal_kernel::Id;

/// Login (`POST`) e logout (`DELETE`).
pub const ROTA_SESSAO: &str = "/v1/sessao";

/// Prontidão do servidor (`GET`), para o balanceador e o Cloudflare.
pub const ROTA_SAUDE: &str = "/saude";

/// A rota de um comando numa empresa: `/v1/e/{empresa}/cmd/{nome}`.
#[must_use]
pub fn rota_comando(empresa: Id, nome: &str) -> String {
    format!("/v1/e/{empresa}/cmd/{nome}")
}

/// A rota de uma consulta numa empresa: `/v1/e/{empresa}/qry/{nome}`.
#[must_use]
pub fn rota_consulta(empresa: Id, nome: &str) -> String {
    format!("/v1/e/{empresa}/qry/{nome}")
}

/// A sessão da conta numa empresa (`GET`): `/v1/e/{empresa}/sessao`.
#[must_use]
pub fn rota_sessao_empresa(empresa: Id) -> String {
    format!("/v1/e/{empresa}/sessao")
}
