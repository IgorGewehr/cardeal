//! `cargo xtask verificar-wasm` — a catraca da ADR-0016: o domínio que o cliente web (egui
//! compilado para `wasm32`) usa tem que compilar **sem** SQLite e **sem um aviso sequer**.
//!
//! Cada crate é conferido com `--no-default-features` (a feature `sqlite` desligada): sobra o
//! domínio e os tipos de comando/consulta. Um `use rusqlite` fora do gate, um tipo de domínio
//! que passou a depender do repositório, um auxiliar usado só pela persistência sem o gate —
//! tudo isso quebra aqui, não meses depois na hora de montar o cliente web.

use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};

/// O que o cliente web compila — da base aos módulos que as telas usam.
const CRATES: &[&str] = &[
    "cardeal-kernel",
    "cardeal-protocol",
    "cardeal-ledger",
    "cardeal-auth",
    "cardeal-modkit",
    "cardeal-ui",
    "mod-empresa",
    "mod-financeiro",
    "mod-clientes",
    "mod-estoque",
    "mod-agenda",
    "mod-os",
    "mod-vendas",
    "mod-pdv",
    "mod-compras",
    "mod-orcamentos",
    "cardeal-cliente",
    "cardeal-web",
];

const ALVO: &str = "wasm32-unknown-unknown";

pub fn executar(raiz: &Path) -> Result<()> {
    let mut args = vec![
        "check".to_owned(),
        "--target".to_owned(),
        ALVO.to_owned(),
        "--no-default-features".to_owned(),
    ];
    for c in CRATES {
        args.push("-p".to_owned());
        args.push((*c).to_owned());
    }
    let status = Command::new(env!("CARGO"))
        .current_dir(raiz)
        .args(&args)
        // Aviso também reprova: um `use` sem gate é o primeiro sinal de vazamento.
        .env("RUSTFLAGS", "-D warnings")
        .status()
        .context("rodando cargo check para wasm32 (instale o alvo: rustup target add wasm32-unknown-unknown)")?;
    if !status.success() {
        bail!("verificar-wasm: o domínio deixou de compilar para o navegador sem SQLite");
    }
    println!(
        "verificar-wasm: ok — {} crates compilam para {ALVO} sem SQLite.",
        CRATES.len()
    );
    Ok(())
}
