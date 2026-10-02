//! # cardeal-protocol
//!
//! O que atravessa a rede entre um cliente (desktop nativo ou egui/WASM) e o `cardeal-server`
//! — `docs/09-protocolo-api.md` e ADR-0016. Servidor e cliente compilam este mesmo crate: uma
//! incompatibilidade é erro de compilação, não erro em produção.
//!
//! Sem `tokio`, sem `rusqlite`, sem `axum`: este crate compila para `wasm32-unknown-unknown`.
//!
//! ## Forma do protocolo
//!
//! - **Três verbos.** Comando (`POST …/cmd/{nome}`), consulta (`POST …/qry/{nome}`) e sessão.
//!   O nome é o mesmo que o `Despachante` registra (`"os.abrir_ordem.v1"`); a carga é o próprio
//!   struct do comando em `postcard`. Não existe enum gigante de comandos: o servidor não conhece
//!   módulos, só nomes, e um módulo novo não muda o protocolo.
//! - **Erro é `cardeal_kernel::Erro`** em `postcard`, com o status HTTP de [`status_http`]. A
//!   tela mostra exatamente o mesmo erro que mostraria no modo monoposto.
//! - **Idempotência obrigatória** em comando: [`CABECALHO_IDEMPOTENCIA`] com um `UUIDv7` por
//!   intenção do usuário (não por tentativa).

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::module_name_repetitions, clippy::must_use_candidate)]

mod rotas;
mod sessao;

pub use rotas::{
    rota_comando, rota_consulta, rota_membro, rota_membros, rota_sessao_empresa, ROTA_SAUDE,
    ROTA_SENHA, ROTA_SESSAO,
};
pub use sessao::{
    EmpresaAcessivel, InfoSessao, MembroAdicionado, PedidoLogin, PedidoNovoMembro,
    PedidoTrocaSenha, RespostaLogin, TipoCliente,
};

use cardeal_kernel::CodigoErro;

/// A versão do protocolo. O servidor aceita `[VERSAO - 2, VERSAO]` (`docs/09` §7).
pub const VERSAO: u16 = 1;

/// O tipo de conteúdo binário entre os componentes do Cardeal.
pub const TIPO_POSTCARD: &str = "application/x-cardeal-postcard";

/// Cabeçalho com a versão do protocolo do cliente. Obrigatório em toda chamada `/v1`: além de
/// versionar, um cabeçalho próprio força o *preflight* de CORS, então um site de terceiros não
/// consegue disparar um comando com o cookie do usuário (defesa de CSRF somada ao
/// `SameSite=Strict`).
pub const CABECALHO_PROTOCOLO: &str = "x-cardeal-protocolo";

/// Cabeçalho com a chave de idempotência do comando (UUID em texto).
pub const CABECALHO_IDEMPOTENCIA: &str = "idempotency-key";

/// Nome do cookie de sessão do navegador (`HttpOnly; Secure; SameSite=Strict`).
pub const COOKIE_SESSAO: &str = "cardeal_sessao";

/// Teto do corpo de uma requisição. Nenhum comando legítimo chega perto: a maior carga real
/// é uma logo PNG (512 KB).
pub const TETO_CORPO_BYTES: usize = 1024 * 1024;

/// Verdadeiro se o servidor desta versão aceita um cliente que fala `versao`.
#[must_use]
pub const fn versao_aceita(versao: u16) -> bool {
    versao <= VERSAO && versao + 2 >= VERSAO
}

/// O status HTTP de um erro do Cardeal, pela faixa do código (`docs/09` §3). Cliente e
/// servidor usam a mesma tabela; o cliente nunca decide nada pelo status — decide pelo
/// `Erro` do corpo —, mas proxies, logs e o Cloudflare enxergam a classe certa.
#[must_use]
pub const fn status_http(codigo: CodigoErro) -> u16 {
    match codigo.0 {
        2002 => 404,
        4003 => 429,
        3001 | 3003 | 3004 => 403,
        3002 | 3005 | 3006 => 401,
        1000..=1999 => 422,
        2000..=2999 | 4000..=4999 => 409,
        5003 => 504,
        5000..=6999 => 503,
        _ => 500,
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn aceita_as_duas_versoes_anteriores_e_nenhuma_futura() {
        assert!(versao_aceita(VERSAO));
        assert!(!versao_aceita(VERSAO + 1));
    }

    #[test]
    fn status_por_faixa() {
        assert_eq!(status_http(CodigoErro::CAMPO_OBRIGATORIO), 422);
        assert_eq!(status_http(CodigoErro::NAO_ENCONTRADO), 404);
        assert_eq!(status_http(CodigoErro::ESTOQUE_INSUFICIENTE), 409);
        assert_eq!(status_http(CodigoErro::SEM_PERMISSAO), 403);
        assert_eq!(status_http(CodigoErro::SESSAO_INVALIDA), 401);
        assert_eq!(status_http(CodigoErro::CREDENCIAL_INVALIDA), 401);
        assert_eq!(status_http(CodigoErro::VERSAO_DESATUALIZADA), 409);
        assert_eq!(status_http(CodigoErro::MUITAS_TENTATIVAS), 429);
        assert_eq!(status_http(CodigoErro::BANCO_INDISPONIVEL), 503);
        assert_eq!(status_http(CodigoErro::TEMPO_ESGOTADO), 504);
        assert_eq!(status_http(CodigoErro::FALHA_INTERNA), 503);
    }
}
