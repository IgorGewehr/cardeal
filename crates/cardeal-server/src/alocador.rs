//! O alocador do processo: mimalloc (v3), ajustado para **devolver memória ao sistema** assim
//! que uma empresa ociosa é fechada — o perfil de um servidor multi-tenant, onde a memória
//! livre de uma empresa fechada deve voltar para o SO, não ficar retida "para depois".
//!
//! Medido com `examples/carga.rs` (200 empresas abertas e despejadas, 2026-10-01):
//!
//! | mimalloc                                   | 200 abertas | após despejo | p99 quente |
//! |--------------------------------------------|-------------|--------------|------------|
//! | padrão                                     | 317 MB      | 258 MB       | 410 µs     |
//! | `purge_delay=0`, `arena_eager_commit=0`    | **164 MB**  | **130 MB**   | **193 µs** |
//!
//! Duas opções, dois caminhos:
//!
//! - `purge_delay` é lido a cada liberação: [`configurar`] o ajusta daqui.
//! - `arena_eager_commit` vale para a primeira arena, criada **antes** do `main` — só a variável
//!   de ambiente `MIMALLOC_ARENA_EAGER_COMMIT=0` alcança. O `Dockerfile` a define; sem ela, o
//!   servidor avisa no log ao subir.
//!
//! O único `unsafe` do servidor mora aqui: `mi_option_set`, que só grava um inteiro na tabela
//! de opções do mimalloc.

#![allow(unsafe_code)]

use libmimalloc_sys::{mi_option_set, mi_option_t};

/// Índice no `enum mi_option_e` do mimalloc v3 (`c_src/mimalloc/v3/include/mimalloc.h`). O crate
/// não exporta a constante; o teste abaixo quebra se uma atualização reordenar o enum.
const PURGE_DELAY: mi_option_t = 15;

/// Aplica o perfil de servidor. Chame no início de `main`, antes de subir threads.
pub(crate) fn configurar() {
    // SAFETY: `mi_option_set` só grava na tabela global de opções; o índice é conferido pelo
    // teste `indice_confere_com_o_padrao_do_mimalloc`.
    unsafe { mi_option_set(PURGE_DELAY, 0) };
    if std::env::var_os("MIMALLOC_ARENA_EAGER_COMMIT").is_none() {
        tracing::warn!(
            "MIMALLOC_ARENA_EAGER_COMMIT não definido — defina =0 (o Dockerfile já define) para \
             não reservar memória da arena antes do uso"
        );
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn indice_confere_com_o_padrao_do_mimalloc() {
        // Padrão documentado no `options.c` do v3: purge_delay = 1000 ms.
        // SAFETY: leitura da tabela de opções.
        let purge = unsafe { libmimalloc_sys::mi_option_get(PURGE_DELAY) };
        assert_eq!(purge, 1000, "o enum mi_option_e mudou — reveja o índice");
    }
}
