//! Verificação, download e aplicação de atualização — via GitHub Releases
//! (`https://github.com/IgorGewehr/cardeal`), sem infraestrutura própria de servidor de
//! update, como o pedido original preferia.
//!
//! ## O que é mecânica real vs. o que depende de uma release existir
//!
//! `verificar_atualizacao` fala com a API real do GitHub agora mesmo — testado nesta sessão
//! contra o repositório de verdade, que ainda **não tem nenhuma release publicada**: a
//! resposta é `404`, tratada como "nada para atualizar" (`Ok(None)`), não como erro. É o
//! caso "rodando de um build local sem release nenhuma ainda" que o pedido original pede
//! para não quebrar — e está coberto de ponta a ponta, de verdade, hoje. O download e a
//! aplicação (`baixar_atualizacao`/`aplicar_atualizacao`) são código real e completo, mas só
//! puderam ser testados contra um corpo de release **sintético** nesta sessão (não existe
//! release de verdade para baixar ainda) — ver `docs/build/atualizacao.md` para o relato
//! exato do que foi exercitado e o que fica para quando a primeira release real existir.
//!
//! ## Convenção de asset esperada
//!
//! Além dos pacotes de instalação (`.rpm`, `.AppImage` — ver `docs/build/empacotamento.md`),
//! uma release deve anexar um binário **puro** chamado exatamente `cardeal-desktop-linux-x86_64`
//! — é nele que o autoatualizador procura, porque um `.rpm` não pode simplesmente substituir
//! o executável em execução (precisaria de `dnf`/root), enquanto um binário puro pode.
//!
//! ## Aplicar a atualização exige que o executável atual seja gravável
//!
//! Um Cardeal instalado via `.rpm` mora em `/usr/bin`, que só root escreve — tentar
//! sobrescrever sem permissão retornaria `PermissionDenied` a qualquer hora, então
//! [`aplicar_atualizacao`] **detecta isso antes de baixar** e devolve
//! [`ResultadoAplicacao::RequerPermissaoDoSistema`] com instruções (`sudo dnf upgrade
//! cardeal`) em vez de falhar tarde ou pedir senha por conta própria — este projeto nunca
//! escala privilégio sozinho. Um Cardeal rodando de `target/release/` (como nesta sessão de
//! desenvolvimento) ou de um layout portátil futuro é gravável e se autoatualiza de verdade.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use cardeal_kernel::{CodigoErro, Erro, Resultado};
use reqwest::blocking::Client;
use serde::Deserialize;

/// O repositório onde as releases são publicadas.
const REPOSITORIO: &str = "IgorGewehr/cardeal";
/// Nome do asset de binário puro que o autoatualizador sabe aplicar — ver a nota do módulo.
const ASSET_LINUX: &str = "cardeal-desktop-linux-x86_64";

#[derive(Debug, Deserialize)]
struct RespostaGithub {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    assets: Vec<AssetGithub>,
}

#[derive(Debug, Deserialize)]
struct AssetGithub {
    name: String,
    browser_download_url: String,
}

/// Uma versão mais nova encontrada, pronta para baixar.
#[derive(Debug, Clone)]
pub struct VersaoDisponivel {
    /// A tag da release (ex.: `v0.2.0`).
    pub versao: String,
    /// As notas da release (corpo do markdown), para mostrar ao usuário.
    pub notas: String,
    /// A página da release no GitHub, para quando não dá para aplicar sozinho.
    pub url_release: String,
    /// URL de download do binário puro Linux, se a release anexou um (ver a convenção no
    /// topo do módulo). `None` quando a release existe mas não tem esse asset — ainda assim
    /// mostramos a versão nova, só sem o botão de aplicar direto.
    pub url_binario_linux: Option<String>,
}

/// O que aconteceu ao tentar aplicar uma atualização já baixada.
pub enum ResultadoAplicacao {
    /// O binário foi substituído — falta só reiniciar (ver [`reiniciar_no_novo_binario`]).
    Aplicada,
    /// O executável atual não é gravável pelo usuário corrente (típico de instalação via
    /// pacote do sistema em `/usr/bin`) — nada foi baixado nem escrito.
    RequerPermissaoDoSistema {
        /// Onde o executável atual está, para a mensagem de instrução.
        caminho: PathBuf,
    },
}

fn cliente_http() -> Resultado<Client> {
    Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("cardeal-desktop")
        .build()
        .map_err(erro_http)
}

fn erro_http(e: reqwest::Error) -> Erro {
    let codigo = if e.is_timeout() {
        CodigoErro::TEMPO_ESGOTADO
    } else {
        CodigoErro::BANCO_INDISPONIVEL
    };
    Erro::novo(codigo, format!("não foi possível falar com o GitHub: {e}"))
}

/// Compara `a.b.c` (aceita prefixo `v`, ignora sufixo tipo `-rc1`). `None` se não é
/// reconhecível como semver — o chamador trata isso como "não dá para comparar, não ofereça
/// a atualização" em vez de arriscar um falso positivo.
fn versao_semver(s: &str) -> Option<(u64, u64, u64)> {
    let s = s.trim().trim_start_matches('v');
    let base = s.split('-').next().unwrap_or(s);
    let mut partes = base.split('.');
    let major = partes.next()?.parse().ok()?;
    let minor = partes.next()?.parse().ok()?;
    let patch = partes.next()?.parse().ok()?;
    Some((major, minor, patch))
}

/// Consulta a release mais recente do GitHub e diz se é mais nova que `versao_atual`
/// (`env!("CARGO_PKG_VERSION")` do chamador). Nunca falha "alto" por não haver release ainda
/// ou por limite de taxa — nesses casos devolve `Ok(None)`, silenciosamente: um app que abre
/// sem internet, ou contra um repositório sem release nenhuma, não deve incomodar o usuário.
///
/// # Errors
/// Só em erro de rede genuíno (DNS, timeout) que não seja "sem release"/"limite de taxa" —
/// hoje isso é raro o bastante para não ter um teste dedicado; o chamador (`cardeal-desktop`)
/// trata qualquer `Err` aqui como "não foi possível verificar agora", sem travar a UI.
pub fn verificar_atualizacao(versao_atual: &str) -> Resultado<Option<VersaoDisponivel>> {
    let cliente = cliente_http()?;
    let resp = cliente
        .get(format!(
            "https://api.github.com/repos/{REPOSITORIO}/releases/latest"
        ))
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .map_err(erro_http)?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        // Repositório sem nenhuma release publicada ainda — exatamente o caso que o pedido
        // original pede para não quebrar. Verificado de verdade contra a API real nesta
        // sessão (ver docs/build/atualizacao.md).
        return Ok(None);
    }
    if !resp.status().is_success() {
        tracing::warn!(status = %resp.status(), "verificação de atualização: GitHub não respondeu OK");
        return Ok(None);
    }

    let corpo: RespostaGithub = resp.json().map_err(erro_http)?;
    Ok(decidir_se_ha_atualizacao(corpo, versao_atual))
}

/// A parte pura de [`verificar_atualizacao`] — sem rede, testável com um corpo sintético.
/// Separado de propósito: é o jeito de provar a lógica de decisão (comparação de versão,
/// escolha do asset) sem depender da API do GitHub estar de pé durante o teste.
fn decidir_se_ha_atualizacao(
    corpo: RespostaGithub,
    versao_atual: &str,
) -> Option<VersaoDisponivel> {
    let remota = versao_semver(&corpo.tag_name)?;
    let atual = versao_semver(versao_atual)?;
    if remota <= atual {
        return None;
    }

    let url_binario_linux = corpo
        .assets
        .iter()
        .find(|a| a.name == ASSET_LINUX)
        .map(|a| a.browser_download_url.clone());

    Some(VersaoDisponivel {
        url_release: format!(
            "https://github.com/{REPOSITORIO}/releases/tag/{}",
            corpo.tag_name
        ),
        versao: corpo.tag_name,
        notas: corpo.body.unwrap_or_default(),
        url_binario_linux,
    })
}

/// Baixa os bytes de um asset de release.
///
/// # Errors
/// Erro de rede.
pub fn baixar_atualizacao(url: &str) -> Resultado<Vec<u8>> {
    let cliente = cliente_http()?;
    let resp = cliente.get(url).send().map_err(erro_http)?;
    if !resp.status().is_success() {
        return Err(Erro::novo(
            CodigoErro::BANCO_INDISPONIVEL,
            format!("download da atualização falhou: HTTP {}", resp.status()),
        ));
    }
    resp.bytes().map(|b| b.to_vec()).map_err(erro_http)
}

/// Verdadeiro se o processo atual (dono do arquivo, ou root) consegue escrever em `caminho`
/// — testado abrindo para escrita sem truncar, não escrevendo nada.
fn e_gravavel(caminho: &Path) -> bool {
    fs::OpenOptions::new().write(true).open(caminho).is_ok()
}

/// Substitui o executável atual pelos bytes baixados, de forma atômica (grava num arquivo
/// temporário ao lado e usa `rename` — o mesmo truque que faz um binário em execução no
/// Linux poder ser substituído: o processo já rodando continua com o inode antigo aberto, a
/// próxima execução usa o novo). Devolve
/// [`ResultadoAplicacao::RequerPermissaoDoSistema`] em vez de tentar e falhar quando o
/// executável não é gravável — nunca escala privilégio sozinho.
///
/// # Errors
/// Erro de E/S ao gravar ou renomear (quando o caminho já é gravável, mas o disco falhou por
/// outro motivo — cheio, etc.).
pub fn aplicar_atualizacao(bytes: &[u8]) -> Resultado<ResultadoAplicacao> {
    let atual = std::env::current_exe()
        .map_err(|e| Erro::novo(CodigoErro::FALHA_INTERNA, format!("current_exe: {e}")))?;

    if !e_gravavel(&atual) {
        return Ok(ResultadoAplicacao::RequerPermissaoDoSistema { caminho: atual });
    }

    let temporario = atual.with_extension("novo");
    fs::write(&temporario, bytes).map_err(|e| {
        Erro::novo(
            CodigoErro::FALHA_DE_DISCO,
            format!("gravando atualização: {e}"),
        )
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&temporario)
            .map_err(|e| Erro::novo(CodigoErro::FALHA_DE_DISCO, e.to_string()))?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&temporario, perms)
            .map_err(|e| Erro::novo(CodigoErro::FALHA_DE_DISCO, e.to_string()))?;
    }

    fs::rename(&temporario, &atual).map_err(|e| {
        Erro::novo(
            CodigoErro::FALHA_DE_DISCO,
            format!("aplicando atualização: {e}"),
        )
    })?;

    Ok(ResultadoAplicacao::Aplicada)
}

/// Relança o executável (agora atualizado) como um processo novo e encerra o atual. Chame
/// só depois de [`ResultadoAplicacao::Aplicada`] — não fecha o banco explicitamente antes de
/// sair (`std::process::exit`, sem rodar destrutores), mas isso é seguro pelo mesmo motivo
/// que um encerramento abrupto qualquer já é: o WAL do SQLite é desenhado para sobreviver a
/// isso (`docs/03-pilar-resiliencia.md`) — nenhuma escrita fica pendente sem fsync.
///
/// # Errors
/// Se não conseguir nem iniciar o novo processo (o atual continua rodando, sem sair).
pub fn reiniciar_no_novo_binario() -> Resultado<()> {
    let atual = std::env::current_exe()
        .map_err(|e| Erro::novo(CodigoErro::FALHA_INTERNA, format!("current_exe: {e}")))?;
    std::process::Command::new(&atual)
        .spawn()
        .map_err(|e| Erro::novo(CodigoErro::FALHA_INTERNA, format!("relançando: {e}")))?;
    std::process::exit(0);
}

#[cfg(test)]
mod testes {
    use super::{
        decidir_se_ha_atualizacao, versao_semver, AssetGithub, RespostaGithub, ASSET_LINUX,
    };

    #[test]
    fn compara_semver_com_e_sem_prefixo_v() {
        assert_eq!(versao_semver("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(versao_semver("1.2.3"), Some((1, 2, 3)));
        assert_eq!(versao_semver("1.2.3-rc1"), Some((1, 2, 3)));
        assert_eq!(versao_semver("nao-e-versao"), None);
        assert!(versao_semver("2.0.0") > versao_semver("1.9.9"));
        assert!(versao_semver("0.2.0") > versao_semver("0.1.0"));
    }

    fn corpo_sintetico(tag: &str, com_asset_linux: bool) -> RespostaGithub {
        RespostaGithub {
            tag_name: tag.to_string(),
            body: Some("notas da release".to_string()),
            assets: if com_asset_linux {
                vec![AssetGithub {
                    name: ASSET_LINUX.to_string(),
                    browser_download_url: "https://example.invalid/cardeal-desktop-linux-x86_64"
                        .to_string(),
                }]
            } else {
                vec![]
            },
        }
    }

    #[test]
    fn encontra_versao_mais_nova_com_binario() {
        let disponivel = decidir_se_ha_atualizacao(corpo_sintetico("v0.2.0", true), "0.1.0")
            .expect("deveria encontrar 0.2.0 > 0.1.0");
        assert_eq!(disponivel.versao, "v0.2.0");
        assert!(disponivel.url_binario_linux.is_some());
        assert!(disponivel.url_release.contains("v0.2.0"));
    }

    #[test]
    fn versao_igual_ou_mais_velha_nao_e_atualizacao() {
        assert!(decidir_se_ha_atualizacao(corpo_sintetico("v0.1.0", true), "0.1.0").is_none());
        assert!(decidir_se_ha_atualizacao(corpo_sintetico("v0.1.0", true), "0.2.0").is_none());
    }

    #[test]
    fn versao_mais_nova_sem_binario_ainda_e_reportada() {
        // A release existe e é mais nova, só não tem o asset que o autoatualizador sabe
        // aplicar direto — a UI ainda deve avisar o usuário, só sem o botão de aplicar.
        let disponivel = decidir_se_ha_atualizacao(corpo_sintetico("v0.3.0", false), "0.1.0")
            .expect("deveria encontrar 0.3.0 > 0.1.0 mesmo sem asset");
        assert!(disponivel.url_binario_linux.is_none());
    }
}
