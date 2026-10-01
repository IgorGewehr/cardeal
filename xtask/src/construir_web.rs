//! `cargo xtask construir-web` — o cliente do navegador (ADR-0016), pronto para o servidor:
//!
//! 1. `cargo build --profile web --target wasm32-unknown-unknown -p cardeal-web`
//! 2. `wasm-bindgen --target web` (a ponte JS)
//! 3. `wasm-opt -Oz` (binaryen) — tamanho é tempo de abertura em 4G
//! 4. tudo numa pasta **versionada pelo hash do conteúdo** (`app/<hash>/`): o servidor manda
//!    cache imutável de um ano para ela, e só o `index.html` (sem cache) muda a cada versão
//! 5. pré-compressão `.br` (brotli 11) e `.gz` (gzip 9): o servidor entrega o arquivo pronto,
//!    sem comprimir nada por requisição
//!
//! Saída: `dist/web/` (`index.html` + `app/<hash>/…`). Ferramentas: `wasm-bindgen` na mesma
//! versão do crate (`cargo binstall wasm-bindgen-cli@<versão>`) e `wasm-opt`.

use std::fs;
use std::io::Write as _;
use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};

const ALVO: &str = "wasm32-unknown-unknown";
/// Recursos que o rustc 1.98 já emite para `wasm32-unknown-unknown`; o `wasm-opt` precisa
/// saber deles para não recusar o módulo.
const RECURSOS_WASM: &[&str] = &[
    "--enable-bulk-memory",
    "--enable-nontrapping-float-to-int",
    "--enable-sign-ext",
    "--enable-mutable-globals",
    "--enable-reference-types",
    "--enable-multivalue",
];

fn rodar(cmd: &mut Command, o_que: &str) -> Result<()> {
    let status = cmd
        .status()
        .with_context(|| format!("{o_que}: não foi possível executar"))?;
    if !status.success() {
        bail!("{o_que} falhou");
    }
    Ok(())
}

pub fn executar(raiz: &Path, sem_wasm_opt: bool) -> Result<()> {
    rodar(
        Command::new(env!("CARGO")).current_dir(raiz).args([
            "build",
            "--profile",
            "web",
            "--target",
            ALVO,
            "-p",
            "cardeal-web",
        ]),
        "cargo build (wasm)",
    )?;
    let wasm = raiz.join(format!("target/{ALVO}/web/cardeal_web.wasm"));

    let tmp = raiz.join("target/web-bindgen");
    let _ = fs::remove_dir_all(&tmp);
    rodar(
        Command::new("wasm-bindgen")
            .args(["--target", "web", "--no-typescript", "--out-dir"])
            .arg(&tmp)
            .arg(&wasm),
        "wasm-bindgen (cargo binstall wasm-bindgen-cli)",
    )?;
    let bg = tmp.join("cardeal_web_bg.wasm");
    if !sem_wasm_opt {
        rodar(
            Command::new("wasm-opt")
                .arg("-Oz")
                .args(RECURSOS_WASM)
                .arg(&bg)
                .arg("-o")
                .arg(&bg),
            "wasm-opt (cargo binstall wasm-opt)",
        )?;
    }

    let fonte = raiz.join("crates/cardeal-web/web");
    let arquivos: Vec<(String, Vec<u8>)> = vec![
        ("cardeal_web_bg.wasm".into(), fs::read(&bg)?),
        (
            "cardeal_web.js".into(),
            fs::read(tmp.join("cardeal_web.js"))?,
        ),
        ("iniciar.js".into(), fs::read(fonte.join("iniciar.js"))?),
        ("estilo.css".into(), fs::read(fonte.join("estilo.css"))?),
        ("icone.png".into(), fs::read(fonte.join("icone.png"))?),
    ];
    let mut hasher = blake3::Hasher::new();
    for (_, bytes) in &arquivos {
        hasher.update(bytes);
    }
    let versao = &hasher.finalize().to_hex()[..12];

    let saida = raiz.join("dist/web");
    let _ = fs::remove_dir_all(&saida);
    let pasta = saida.join("app").join(versao);
    fs::create_dir_all(&pasta)?;

    println!("cardeal-web {versao}:");
    for (nome, bytes) in &arquivos {
        gravar_com_compressao(&pasta.join(nome), bytes, nome)?;
    }
    let index = fs::read_to_string(fonte.join("index.html"))?.replace("__VERSAO__", versao);
    gravar_com_compressao(&saida.join("index.html"), index.as_bytes(), "index.html")?;
    println!("→ {}", saida.display());
    Ok(())
}

fn gravar_com_compressao(destino: &Path, bytes: &[u8], nome: &str) -> Result<()> {
    fs::write(destino, bytes)?;
    let mut br = Vec::new();
    {
        let mut c = brotli::CompressorWriter::new(&mut br, 64 * 1024, 11, 24);
        c.write_all(bytes)?;
    }
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    gz.write_all(bytes)?;
    let gz = gz.finish()?;
    // Comprimir só vale quando ganha (PNG já vem comprimido).
    if br.len() < bytes.len() {
        fs::write(destino.with_file_name(format!("{nome}.br")), &br)?;
        fs::write(destino.with_file_name(format!("{nome}.gz")), &gz)?;
    }
    println!(
        "  {nome:<22} {:>9}  br {:>9}  gz {:>9}",
        tamanho(bytes.len()),
        tamanho(br.len()),
        tamanho(gz.len())
    );
    Ok(())
}

#[allow(clippy::cast_precision_loss)]
fn tamanho(b: usize) -> String {
    if b >= 1024 * 1024 {
        format!("{:.2} MB", b as f64 / 1024.0 / 1024.0)
    } else {
        format!("{:.1} KB", b as f64 / 1024.0)
    }
}
