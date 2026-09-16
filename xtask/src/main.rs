//! Tarefas de build e empacotamento do Cardeal.
//!
//! Hoje: `cargo xtask empacotar-linux` — gera um instalador Linux (`.rpm` por padrão, ou
//! `--formato appimage`) a partir do código-fonte, sem exigir que quem for instalar rode
//! `cargo build` na mão. Ver `docs/build/empacotamento.md` para a escolha de formato e o que
//! cada um cobre.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(name = "xtask", about = "Tarefas de build e empacotamento do Cardeal")]
struct Cli {
    #[command(subcommand)]
    comando: Comando,
}

#[derive(Subcommand)]
enum Comando {
    /// Gera um instalador Linux a partir do código-fonte.
    EmpacotarLinux {
        /// Formato do pacote. `rpm` é o padrão — ver docs/build/empacotamento.md para o
        /// porquê (nativo do Fedora, `rpmbuild` já vem instalado, sem baixar ferramenta
        /// nenhuma). `appimage` é portátil entre distros, mas precisa baixar `appimagetool`
        /// na primeira vez e depende de FUSE em tempo de execução.
        #[arg(long, value_enum, default_value_t = FormatoLinux::Rpm)]
        formato: FormatoLinux,
        /// Pula `cargo build --release` — usa o binário que já está em `target/release`.
        #[arg(long)]
        pular_build: bool,
    },
}

#[derive(ValueEnum, Clone, Copy, PartialEq, Eq)]
enum FormatoLinux {
    Rpm,
    Appimage,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let raiz = raiz_do_repositorio()?;
    match cli.comando {
        Comando::EmpacotarLinux {
            formato,
            pular_build,
        } => match formato {
            FormatoLinux::Rpm => empacotar_linux_rpm(&raiz, pular_build),
            FormatoLinux::Appimage => empacotar_linux_appimage(&raiz, pular_build),
        },
    }
}

/// A raiz do workspace — `xtask` é sempre `<raiz>/xtask`, então basta subir um nível a
/// partir do `CARGO_MANIFEST_DIR` deste crate (funciona independente de onde `cargo xtask`
/// foi chamado).
fn raiz_do_repositorio() -> Result<PathBuf> {
    let aqui = Path::new(env!("CARGO_MANIFEST_DIR"));
    aqui.parent()
        .map(Path::to_path_buf)
        .context("xtask precisa estar em <raiz>/xtask")
}

/// Lê `[workspace.package].version` do `Cargo.toml` raiz — a mesma versão que todo crate do
/// workspace herda via `version.workspace = true`, então é a versão certa para o pacote.
fn versao_do_workspace(raiz: &Path) -> Result<String> {
    let conteudo = fs::read_to_string(raiz.join("Cargo.toml")).context("lendo Cargo.toml raiz")?;
    let doc: toml::Value = conteudo.parse().context("parseando Cargo.toml raiz")?;
    doc.get("workspace")
        .and_then(|w| w.get("package"))
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .context("workspace.package.version não encontrado no Cargo.toml raiz")
}

fn rodar(nome: &str, args: &[&str], dir: &Path) -> Result<()> {
    println!("$ {nome} {}", args.join(" "));
    let status = Command::new(nome)
        .args(args)
        .current_dir(dir)
        .status()
        .with_context(|| format!("executando `{nome}` — está instalado e no PATH?"))?;
    if !status.success() {
        bail!("`{nome} {}` saiu com {status}", args.join(" "));
    }
    Ok(())
}

fn cargo_build_release(raiz: &Path, pular: bool) -> Result<()> {
    if pular {
        println!("(--pular-build) usando o binário que já está em target/release");
        return Ok(());
    }
    rodar(
        "cargo",
        &[
            "build",
            "--release",
            "--bin",
            "cardeal-desktop",
            "--bin",
            "cardeal-server",
        ],
        raiz,
    )
}

/// O ícone que a outra sessão de trabalho está integrando (ver `docs/19-estado-e-processo.md`
/// — este agente não cria ícone novo). Procura nos caminhos que essa sessão já usou; se
/// nenhum existir ainda, o pacote sai sem ícone customizado (o `.desktop` funciona igual,
/// só sem arte no menu) e avisa claramente em vez de falhar.
fn encontrar_icone(raiz: &Path) -> Option<PathBuf> {
    [
        "assets/marca/cardeal-icone-256.png",
        "assets/marca/cardeal-icone-512.png",
        "packaging/linux/icon.png",
    ]
    .iter()
    .map(|p| raiz.join(p))
    .find(|p| p.is_file())
}

// ── RPM (padrão) ─────────────────────────────────────────────────────────────

fn empacotar_linux_rpm(raiz: &Path, pular_build: bool) -> Result<()> {
    if Command::new("rpmbuild").arg("--version").output().is_err() {
        bail!(
            "rpmbuild não encontrado — instale com `sudo dnf install rpm-build` (Fedora já \
             traz por padrão em muitas instalações; se não tiver, é este o único requisito)."
        );
    }

    cargo_build_release(raiz, pular_build)?;

    let versao = versao_do_workspace(raiz)?;
    let topdir = raiz.join("target/rpmbuild");
    let sourcedir = topdir.join("SOURCES");
    for sub in ["BUILD", "RPMS", "SOURCES", "SPECS", "SRPMS", "BUILDROOT"] {
        fs::create_dir_all(topdir.join(sub))?;
    }

    fs::copy(
        raiz.join("target/release/cardeal-desktop"),
        sourcedir.join("cardeal-desktop"),
    )
    .context("copiando cardeal-desktop para SOURCES — rode o build release antes")?;
    fs::copy(
        raiz.join("target/release/cardeal-server"),
        sourcedir.join("cardeal-server"),
    )
    .context("copiando cardeal-server para SOURCES — rode o build release antes")?;
    fs::copy(
        raiz.join("packaging/linux/cardeal-desktop.desktop"),
        sourcedir.join("cardeal-desktop.desktop"),
    )
    .context("copiando packaging/linux/cardeal-desktop.desktop")?;

    let tem_icone = if let Some(icone) = encontrar_icone(raiz) {
        fs::copy(&icone, sourcedir.join("cardeal.png"))
            .with_context(|| format!("copiando ícone de {}", icone.display()))?;
        println!(
            "ícone encontrado em {} — incluído no pacote",
            icone.display()
        );
        true
    } else {
        println!(
            "aviso: nenhum ícone encontrado (esperado em assets/marca/cardeal-icone-256.png, \
             integrado por outra sessão de trabalho) — o pacote sai sem ícone customizado."
        );
        false
    };

    let spec = raiz.join("packaging/rpm/cardeal.spec");
    rodar(
        "rpmbuild",
        &[
            "--define",
            &format!("_topdir {}", topdir.display()),
            "--define",
            &format!("app_version {versao}"),
            "--define",
            &format!("have_icon {}", i32::from(tem_icone)),
            "-bb",
            spec.to_str().context("caminho do spec não é UTF-8")?,
        ],
        raiz,
    )?;

    let rpms_dir = topdir.join("RPMS/x86_64");
    let gerado = fs::read_dir(&rpms_dir)
        .with_context(|| format!("lendo {}", rpms_dir.display()))?
        .filter_map(std::result::Result::ok)
        .find(|e| e.path().extension().is_some_and(|ext| ext == "rpm"))
        .map(|e| e.path())
        .context("rpmbuild rodou mas nenhum .rpm apareceu em RPMS/x86_64")?;

    let dist = raiz.join("dist");
    fs::create_dir_all(&dist)?;
    let alvo = dist.join(gerado.file_name().context("nome de arquivo do rpm")?);
    fs::copy(&gerado, &alvo)?;
    println!("\nPronto: {}", alvo.display());
    println!("Instalar:   sudo dnf install {}", alvo.display());
    println!("Desinstalar: sudo dnf remove cardeal");
    Ok(())
}

// ── AppImage (portátil entre distros — ver docs/build/empacotamento.md) ─────

fn empacotar_linux_appimage(raiz: &Path, pular_build: bool) -> Result<()> {
    cargo_build_release(raiz, pular_build)?;

    let appdir = raiz.join("target/AppDir");
    if appdir.exists() {
        fs::remove_dir_all(&appdir)?;
    }
    let bin_dir = appdir.join("usr/bin");
    fs::create_dir_all(&bin_dir)?;
    fs::copy(
        raiz.join("target/release/cardeal-desktop"),
        bin_dir.join("cardeal-desktop"),
    )
    .context("copiando cardeal-desktop para o AppDir — rode o build release antes")?;
    fs::copy(
        raiz.join("target/release/cardeal-server"),
        bin_dir.join("cardeal-server"),
    )
    .context("copiando cardeal-server para o AppDir")?;

    fs::copy(
        raiz.join("packaging/linux/cardeal-desktop.desktop"),
        appdir.join("cardeal-desktop.desktop"),
    )?;

    let icon_dest = appdir.join("cardeal.png");
    if let Some(icone) = encontrar_icone(raiz) {
        fs::copy(&icone, &icon_dest)
            .with_context(|| format!("copiando ícone de {}", icone.display()))?;
        println!(
            "ícone encontrado em {} — incluído no AppImage",
            icone.display()
        );
    } else {
        // AppImage exige um ícone em algum formato no AppDir para o AppRun/appimagetool não
        // recusarem o pacote — sem a logo (integrada por outra sessão) ainda não temos um
        // arquivo real para copiar. Documentado como pendência em vez de inventar um ícone.
        bail!(
            "AppImage precisa de um ícone em assets/marca/cardeal-icone-256.png (ainda não \
             integrado nesta branch) — use `--formato rpm` por enquanto, ou rode de novo \
             depois que a logo for mesclada."
        );
    }

    fs::write(
        appdir.join("AppRun"),
        "#!/bin/sh\nHERE=\"$(dirname \"$(readlink -f \"$0\")\")\"\nexec \"$HERE/usr/bin/cardeal-desktop\" \"$@\"\n",
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let caminho = appdir.join("AppRun");
        let mut perms = fs::metadata(&caminho)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&caminho, perms)?;
    }

    let appimagetool = obter_appimagetool(raiz)?;
    let dist = raiz.join("dist");
    fs::create_dir_all(&dist)?;
    let versao = versao_do_workspace(raiz)?;
    let saida = dist.join(format!("Cardeal-{versao}-x86_64.AppImage"));
    rodar(
        appimagetool
            .to_str()
            .context("caminho do appimagetool não é UTF-8")?,
        &[
            appdir.to_str().context("caminho do AppDir não é UTF-8")?,
            saida.to_str().context("caminho de saída não é UTF-8")?,
        ],
        raiz,
    )?;
    println!("\nPronto: {}", saida.display());
    println!(
        "Rodar:  chmod +x {} && {}",
        saida.display(),
        saida.display()
    );
    Ok(())
}

/// `appimagetool` não é uma dependência do workspace (só usada para empacotar, nunca em
/// tempo de execução do app) — baixa o AppImage oficial uma vez para `target/tools/` e
/// reaproveita depois. Precisa de rede só na primeira vez.
fn obter_appimagetool(raiz: &Path) -> Result<PathBuf> {
    if let Ok(saida) = Command::new("appimagetool").arg("--version").output() {
        if saida.status.success() {
            return Ok(PathBuf::from("appimagetool"));
        }
    }
    let ferramentas = raiz.join("target/tools");
    fs::create_dir_all(&ferramentas)?;
    let caminho = ferramentas.join("appimagetool");
    if !caminho.exists() {
        println!("baixando appimagetool (primeira vez apenas)...");
        rodar(
            "curl",
            &[
                "-fL",
                "-o",
                caminho.to_str().context("caminho appimagetool não é UTF-8")?,
                "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage",
            ],
            raiz,
        )
        .context(
            "falha ao baixar appimagetool — sem rede? baixe manualmente e passe pelo PATH \
             como `appimagetool`",
        )?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&caminho)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&caminho, perms)?;
        }
    }
    Ok(caminho)
}
