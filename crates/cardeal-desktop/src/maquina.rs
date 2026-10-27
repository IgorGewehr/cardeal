//! O que é **deste computador**, não da empresa: a identidade dele como terminal (caixa) e a
//! impressora do balcão. Fica num arquivo na pasta de dados do Cardeal (`maquina.json`, ao lado da
//! base) — com vários caixas no mesmo servidor, cada máquina tem a sua impressora e a sua
//! identidade, e nada disso viaja para as outras.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use cardeal_kernel::Id;
use serde::{Deserialize, Serialize};

/// Para onde vai o que se imprime no balcão (o DANFCE e o comprovante).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tipo", content = "nome", rename_all = "snake_case")]
pub enum Destino {
    /// Abre o PDF no visualizador do sistema, de onde se imprime (o padrão: funciona sem
    /// configurar nada).
    #[default]
    Pdf,
    /// Uma impressora instalada no sistema (o nome dela), recebendo ESC/POS cru.
    Instalada(String),
    /// Uma impressora de rede: `IP` ou `IP:porta` (a porta padrão das térmicas é a 9100).
    Rede(String),
}

/// A impressora do balcão desta máquina.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ImpressoraDaMaquina {
    /// Para onde vai.
    pub destino: Destino,
    /// Imprimir com acento (página PC860). Sem: o texto sai sem acento, que toda térmica imprime.
    pub acentos: bool,
}

/// As preferências desta máquina.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Maquina {
    /// A identidade deste computador como terminal: o número das vendas e a série da NFC-e são
    /// por terminal. Gerada na primeira vez ([`terminal`]).
    pub terminal: Option<Id>,
    /// A impressora do balcão.
    pub impressora: ImpressoraDaMaquina,
}

impl Maquina {
    /// Lê o arquivo; sem ele (ou ilegível), o padrão — o PDF.
    #[must_use]
    pub fn ler(caminho: &Path) -> Self {
        std::fs::read_to_string(caminho)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    /// Grava o arquivo.
    ///
    /// # Errors
    /// Falha de E/S.
    pub fn gravar(&self, caminho: &Path) -> Result<(), String> {
        if let Some(pasta) = caminho.parent() {
            std::fs::create_dir_all(pasta).map_err(|e| e.to_string())?;
        }
        let texto = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(caminho, texto).map_err(|e| e.to_string())
    }
}

fn caminho() -> PathBuf {
    crate::diretorio_de_dados().join("maquina.json")
}

/// As preferências lidas uma vez por execução.
static ATUAL: Mutex<Option<Maquina>> = Mutex::new(None);

fn carregada(g: &mut Option<Maquina>) -> &mut Maquina {
    g.get_or_insert_with(|| {
        if cfg!(test) {
            Maquina::default()
        } else {
            Maquina::ler(&caminho())
        }
    })
}

/// As preferências desta máquina. Nos testes, sempre o padrão (o PDF): um teste nunca manda
/// nada à impressora de verdade de quem o roda.
#[must_use]
pub fn atual() -> Maquina {
    let mut g = ATUAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    carregada(&mut g).clone()
}

/// Este computador como terminal (caixa): o mesmo a cada abertura do app — antes era um novo a
/// cada vez, e o número das vendas recomeçava do 1. Na primeira vez é gerado e guardado; se a
/// gravação falhar, vale nesta execução.
#[must_use]
pub fn terminal() -> Id {
    let mut g = ATUAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let m = carregada(&mut g);
    if let Some(t) = m.terminal {
        return t;
    }
    let t = Id::novo();
    m.terminal = Some(t);
    if !cfg!(test) {
        if let Err(e) = m.gravar(&caminho()) {
            tracing::warn!("não deu para guardar a identidade do terminal: {e}");
        }
    }
    t
}

/// Grava as preferências desta máquina e passa a usá-las.
///
/// # Errors
/// Falha de E/S ao gravar o arquivo.
pub fn salvar(m: Maquina) -> Result<(), String> {
    if !cfg!(test) {
        m.gravar(&caminho())?;
    }
    *ATUAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(m);
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn grava_le_e_sem_arquivo_e_o_pdf() {
        let dir = tempfile::tempdir().unwrap();
        let caminho = dir.path().join("sub").join("maquina.json");
        assert_eq!(Maquina::ler(&caminho), Maquina::default());
        let m = Maquina {
            terminal: Some(Id::novo()),
            impressora: ImpressoraDaMaquina {
                destino: Destino::Rede("192.168.0.50".into()),
                acentos: true,
            },
        };
        m.gravar(&caminho).unwrap();
        assert_eq!(Maquina::ler(&caminho), m);
        // Arquivo estragado: o padrão, sem travar a abertura.
        std::fs::write(&caminho, "{ não é json").unwrap();
        assert_eq!(Maquina::ler(&caminho), Maquina::default());
        // Nos testes, a máquina é sempre a do PDF; o terminal é um só por execução.
        assert_eq!(atual().impressora.destino, Destino::Pdf);
        assert_eq!(terminal(), terminal());
    }
}
