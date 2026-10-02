//! As rotas — montadas aqui para cliente e servidor nunca divergirem num caractere.

use cardeal_kernel::Id;

/// Login (`POST`) e logout (`DELETE`).
pub const ROTA_SESSAO: &str = "/v1/sessao";

/// Troca de senha da conta (`POST`).
pub const ROTA_SENHA: &str = "/v1/sessao/senha";

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

/// Os membros de uma empresa (`POST` adiciona): `/v1/e/{empresa}/membros`.
#[must_use]
pub fn rota_membros(empresa: Id) -> String {
    format!("/v1/e/{empresa}/membros")
}

/// Um membro (`DELETE` desativa e desvincula): `/v1/e/{empresa}/membros/{usuario}`.
#[must_use]
pub fn rota_membro(empresa: Id, usuario: Id) -> String {
    format!("/v1/e/{empresa}/membros/{usuario}")
}

/// Os CNPJs da organização (`POST` cadastra outro): `/v1/e/{empresa}/empresas`.
#[must_use]
pub fn rota_empresas(empresa: Id) -> String {
    format!("/v1/e/{empresa}/empresas")
}

/// O fluxo de alterações da empresa (`GET`, `text/event-stream`): `/v1/e/{empresa}/eventos`.
///
/// O `EventSource` do navegador não manda cabeçalhos próprios, então a versão do protocolo
/// vai na consulta (`?v=`). Cada evento [`EVENTO_MUDOU`] traz no `data` os módulos que
/// mudaram, separados por vírgula, e no `id` o ponto para continuar (o navegador o devolve
/// sozinho em `Last-Event-ID` ao reconectar). [`EVENTO_RECARREGAR`] diz que o ponto se
/// perdeu: recarregue tudo o que está na tela.
#[must_use]
pub fn rota_eventos(empresa: Id) -> String {
    format!("/v1/e/{empresa}/eventos?v={}", crate::VERSAO)
}

/// Alguns módulos mudaram (`data` = `os,financeiro`).
pub const EVENTO_MUDOU: &str = "mudou";

/// O cliente perdeu o fio (podado ou de outra base): recarregar tudo.
pub const EVENTO_RECARREGAR: &str = "recarregar";

/// A sessão caiu (logout, acesso removido): o fluxo termina.
pub const EVENTO_SESSAO_ENCERRADA: &str = "sessao_encerrada";

/// Os módulos de um evento [`EVENTO_MUDOU`].
pub fn modulos_alterados(data: &str) -> impl Iterator<Item = &str> {
    data.split(',').map(str::trim).filter(|m| !m.is_empty())
}
