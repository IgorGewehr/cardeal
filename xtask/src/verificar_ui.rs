//! `cargo xtask verificar-ui` — impõe a regra do ADR-0015: **toda UI das telas vem do design
//! system (`cardeal-ui`)**, nunca de `egui` cru.
//!
//! O que faz: varre `crates/cardeal-desktop/src` procurando construções proibidas
//! ([`PROIBIDOS`]) e compara a contagem, por arquivo e por padrão, com o
//! `xtask/ui-baseline.toml`.
//!
//! É um **catraca** (*ratchet*), não uma lista de tolerância:
//!
//! - contagem **acima** do baseline → falha (violação nova);
//! - contagem **abaixo** do baseline → falha também, pedindo `--gerar-baseline` — assim o
//!   arquivo nunca mente e a dívida só desce;
//! - arquivo fora do baseline parte de **zero** permitido;
//! - `--gerar-baseline` recusa qualquer contagem que **suba**. Para pagar a dívida, troque o
//!   `egui` cru pelo componente; para uma exceção legítima, use `// ui-livre: <motivo>`.
//!
//! Se o design system não tem o componente que a tela precisa, a resposta **não** é usar
//! `egui` cru: é criar o componente em `cardeal-ui` (ver `docs/adr/0015`).

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use walkdir::WalkDir;

/// Onde a regra vale. Os crates `mod-*` não têm UI; `cardeal-ui` é onde o `egui` cru mora.
const RAIZ_TELAS: &str = "crates/cardeal-desktop/src";

/// Onde a catraca guarda a dívida atual, relativo à raiz do workspace.
const ARQUIVO_BASELINE: &str = "xtask/ui-baseline.toml";

/// Marcador de exceção justificada, na linha ou na linha imediatamente acima.
const MARCA_EXCECAO: &str = "ui-livre:";

/// Um padrão proibido e o que usar no lugar.
pub struct Proibido {
    /// O trecho de código procurado (comparação literal, sem regex).
    pub padrao: &'static str,
    /// A instrução que aparece quando falha.
    pub use_em_vez: &'static str,
}

/// Construções de `egui` cru vetadas nas telas. A ordem é a do relatório.
pub const PROIBIDOS: &[Proibido] = &[
    Proibido {
        padrao: "egui::Frame",
        use_em_vez: "um organismo de painel/cartão de cardeal-ui (crie-o lá se faltar)",
    },
    Proibido {
        padrao: "egui::Stroke",
        use_em_vez: "um componente que já traga a borda do tema",
    },
    Proibido {
        padrao: "Color32",
        use_em_vez: "ui.cores() (tokens do tema); nunca cor literal numa tela",
    },
    Proibido {
        padrao: "ui.painter(",
        use_em_vez: "um átomo/organismo que desenhe (crie-o em cardeal-ui)",
    },
    Proibido {
        padrao: "ui.separator(",
        use_em_vez: "o divisor do design system (crie o átomo se faltar)",
    },
    Proibido {
        padrao: "ui.checkbox(",
        use_em_vez: "um átomo de caixa de seleção do design system (crie-o se faltar)",
    },
    Proibido {
        padrao: "egui::ComboBox",
        use_em_vez: "SeletorOpcao ou SeletorBusca",
    },
    Proibido {
        padrao: "egui::TextEdit",
        use_em_vez: "Campo / CampoTexto",
    },
    Proibido {
        padrao: "egui::RichText",
        use_em_vez: "Rotulo (papéis tipográficos)",
    },
    Proibido {
        padrao: "egui::Window",
        use_em_vez: "Dialogo ou dialogo_confirmacao",
    },
    Proibido {
        padrao: "egui::Grid",
        use_em_vez: "Grade",
    },
    Proibido {
        padrao: "TableBuilder",
        use_em_vez: "Grade",
    },
    Proibido {
        padrao: "ui.button(",
        use_em_vez: "Botao",
    },
    Proibido {
        padrao: "ui.label(",
        use_em_vez: "Rotulo",
    },
    Proibido {
        padrao: "ui.heading(",
        use_em_vez: "Rotulo::titulo_secao / CabecalhoTela",
    },
    Proibido {
        padrao: "ui.add_sized(",
        use_em_vez: "o layout do componente (largura é do componente, não da tela)",
    },
];

/// Contagem de violações: arquivo → padrão → quantidade.
type Contagens = BTreeMap<String, BTreeMap<String, u32>>;

/// Conta as ocorrências de cada padrão proibido em `fonte`, ignorando comentários e as
/// linhas marcadas com [`MARCA_EXCECAO`] (na própria linha ou na anterior).
pub fn contar(fonte: &str) -> BTreeMap<String, u32> {
    let mut contagem = BTreeMap::new();
    let mut anterior_isenta = false;
    for linha in fonte.lines() {
        let limpa = linha.trim_start();
        let e_comentario = limpa.starts_with("//");
        let isenta = tem_excecao(linha);
        if !e_comentario && !isenta && !anterior_isenta {
            for p in PROIBIDOS {
                let n = linha.matches(p.padrao).count();
                if n > 0 {
                    *contagem.entry(p.padrao.to_owned()).or_insert(0) +=
                        u32::try_from(n).unwrap_or(u32::MAX);
                }
            }
        }
        // Só uma linha de comentário isolada isenta a linha seguinte.
        anterior_isenta = e_comentario && isenta;
    }
    contagem
}

/// `// ui-livre: motivo` — o motivo é obrigatório (exceção sem justificativa não vale).
fn tem_excecao(linha: &str) -> bool {
    linha
        .split_once(MARCA_EXCECAO)
        .is_some_and(|(_, motivo)| !motivo.trim().is_empty())
}

fn varrer(raiz: &Path) -> Result<Contagens> {
    let base = raiz.join(RAIZ_TELAS);
    let mut tudo = Contagens::new();
    for entrada in WalkDir::new(&base)
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
        let caminho = entrada.path();
        if caminho.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let fonte =
            fs::read_to_string(caminho).with_context(|| format!("lendo {}", caminho.display()))?;
        let contagem = contar(&fonte);
        if !contagem.is_empty() {
            let rel = caminho
                .strip_prefix(&base)
                .unwrap_or(caminho)
                .to_string_lossy()
                .replace('\\', "/");
            tudo.insert(rel, contagem);
        }
    }
    Ok(tudo)
}

fn ler_baseline(raiz: &Path) -> Result<Contagens> {
    let caminho = raiz.join(ARQUIVO_BASELINE);
    if !caminho.exists() {
        return Ok(Contagens::new());
    }
    let texto =
        fs::read_to_string(&caminho).with_context(|| format!("lendo {ARQUIVO_BASELINE}"))?;
    let doc: toml::Table = texto
        .parse()
        .with_context(|| format!("parseando {ARQUIVO_BASELINE}"))?;
    let mut saida = Contagens::new();
    if let Some(toml::Value::Table(arquivos)) = doc.get("arquivo") {
        for (nome, padroes) in arquivos {
            let Some(padroes) = padroes.as_table() else {
                bail!("{ARQUIVO_BASELINE}: [arquivo.\"{nome}\"] deve ser uma tabela");
            };
            let mut m = BTreeMap::new();
            for (padrao, n) in padroes {
                let n = n
                    .as_integer()
                    .and_then(|n| u32::try_from(n).ok())
                    .with_context(|| {
                        format!("{ARQUIVO_BASELINE}: contagem inválida em {nome}/{padrao}")
                    })?;
                m.insert(padrao.clone(), n);
            }
            saida.insert(nome.clone(), m);
        }
    }
    Ok(saida)
}

fn escrever_baseline(raiz: &Path, atual: &Contagens) -> Result<()> {
    let mut s = String::from(
        "# Dívida de UI: `egui` cru ainda presente nas telas (ADR-0015).\n\
         #\n\
         # NÃO edite à mão. Só desce: `cargo xtask verificar-ui --gerar-baseline` recusa qualquer\n\
         # contagem que suba. Para pagar a dívida, troque o `egui` cru pelo componente de\n\
         # `cardeal-ui` (ou crie o componente lá). Exceção legítima: `// ui-livre: <motivo>`.\n",
    );
    for (arquivo, padroes) in atual {
        s.push_str(&format!("\n[arquivo.\"{arquivo}\"]\n"));
        for (padrao, n) in padroes {
            s.push_str(&format!("\"{padrao}\" = {n}\n"));
        }
    }
    fs::write(raiz.join(ARQUIVO_BASELINE), s)
        .with_context(|| format!("escrevendo {ARQUIVO_BASELINE}"))
}

/// Uma divergência entre o código e o baseline.
#[derive(Debug, PartialEq, Eq)]
pub enum Divergencia {
    /// Mais ocorrências do que o permitido — violação nova.
    Subiu {
        arquivo: String,
        padrao: String,
        antes: u32,
        agora: u32,
    },
    /// Menos ocorrências do que o baseline registra — a dívida foi paga, registre.
    Desceu {
        arquivo: String,
        padrao: String,
        antes: u32,
        agora: u32,
    },
}

/// Compara o estado atual com o baseline.
pub fn comparar(baseline: &Contagens, atual: &Contagens) -> Vec<Divergencia> {
    let mut chaves: Vec<(&String, &String)> = Vec::new();
    for (a, ps) in baseline.iter().chain(atual.iter()) {
        for p in ps.keys() {
            chaves.push((a, p));
        }
    }
    chaves.sort();
    chaves.dedup();

    let ler =
        |c: &Contagens, a: &str, p: &str| c.get(a).and_then(|m| m.get(p)).copied().unwrap_or(0);
    chaves
        .into_iter()
        .filter_map(|(a, p)| {
            let (antes, agora) = (ler(baseline, a, p), ler(atual, a, p));
            match agora.cmp(&antes) {
                std::cmp::Ordering::Greater => Some(Divergencia::Subiu {
                    arquivo: a.clone(),
                    padrao: p.clone(),
                    antes,
                    agora,
                }),
                std::cmp::Ordering::Less => Some(Divergencia::Desceu {
                    arquivo: a.clone(),
                    padrao: p.clone(),
                    antes,
                    agora,
                }),
                std::cmp::Ordering::Equal => None,
            }
        })
        .collect()
}

fn instrucao(padrao: &str) -> &'static str {
    PROIBIDOS
        .iter()
        .find(|p| p.padrao == padrao)
        .map_or("o design system", |p| p.use_em_vez)
}

/// Ponto de entrada de `cargo xtask verificar-ui`.
pub fn executar(raiz: &Path, gerar_baseline: bool) -> Result<()> {
    let atual = varrer(raiz)?;
    let baseline = ler_baseline(raiz)?;
    let divergencias = comparar(&baseline, &atual);

    if gerar_baseline {
        let subiu: Vec<_> = divergencias
            .iter()
            .filter(|d| matches!(d, Divergencia::Subiu { .. }))
            .collect();
        if !subiu.is_empty() && raiz.join(ARQUIVO_BASELINE).exists() {
            relatar(&divergencias);
            bail!("--gerar-baseline só registra dívida que DESCEU; há violação nova acima");
        }
        escrever_baseline(raiz, &atual)?;
        let total: u32 = atual.values().flat_map(BTreeMap::values).sum();
        println!("baseline gravado em {ARQUIVO_BASELINE}: {total} ocorrência(s) restante(s).");
        return Ok(());
    }

    if divergencias.is_empty() {
        let total: u32 = atual.values().flat_map(BTreeMap::values).sum();
        println!("verificar-ui: ok — nenhuma violação nova ({total} de dívida antiga a pagar).");
        return Ok(());
    }
    relatar(&divergencias);
    bail!("verificar-ui falhou (ADR-0015: a UI das telas vem de cardeal-ui, não de egui cru)");
}

fn relatar(divergencias: &[Divergencia]) {
    for d in divergencias {
        match d {
            Divergencia::Subiu {
                arquivo,
                padrao,
                antes,
                agora,
            } => eprintln!(
                "✗ {RAIZ_TELAS}/{arquivo}: `{padrao}` {agora}× (permitido {antes}). Use {}.",
                instrucao(padrao)
            ),
            Divergencia::Desceu {
                arquivo,
                padrao,
                antes,
                agora,
            } => eprintln!(
                "↓ {RAIZ_TELAS}/{arquivo}: `{padrao}` caiu de {antes} para {agora} — \
                 registre com `cargo xtask verificar-ui --gerar-baseline`."
            ),
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn conta_construcoes_proibidas() {
        let c = contar("let f = egui::Frame::none();\nui.separator();\nui.separator();\n");
        assert_eq!(c.get("egui::Frame"), Some(&1));
        assert_eq!(c.get("ui.separator("), Some(&2));
    }

    #[test]
    fn ignora_comentarios() {
        let c = contar("// usa egui::Frame antigamente\n/// ver ui.label(\nlet x = 1;\n");
        assert!(c.is_empty());
    }

    #[test]
    fn excecao_exige_motivo() {
        let com = contar("ui.painter(); // ui-livre: desenho do Rio do Caixa\n");
        assert!(com.is_empty());
        let sem = contar("ui.painter(); // ui-livre:\n");
        assert_eq!(sem.get("ui.painter("), Some(&1));
    }

    #[test]
    fn excecao_na_linha_anterior_isenta_so_a_seguinte() {
        let c = contar("// ui-livre: gráfico\nui.painter();\nui.painter();\n");
        assert_eq!(c.get("ui.painter("), Some(&1));
    }

    #[test]
    fn catraca_falha_quando_sobe_e_quando_desce() {
        let mut base = Contagens::new();
        base.entry("a.rs".into())
            .or_default()
            .insert("egui::Frame".into(), 2);

        let mut mais = Contagens::new();
        mais.entry("a.rs".into())
            .or_default()
            .insert("egui::Frame".into(), 3);
        assert!(matches!(
            comparar(&base, &mais)[0],
            Divergencia::Subiu { .. }
        ));

        let mut menos = Contagens::new();
        menos
            .entry("a.rs".into())
            .or_default()
            .insert("egui::Frame".into(), 1);
        assert!(matches!(
            comparar(&base, &menos)[0],
            Divergencia::Desceu { .. }
        ));

        assert!(comparar(&base, &base).is_empty());
    }

    #[test]
    fn arquivo_novo_parte_de_zero() {
        let base = Contagens::new();
        let mut novo = Contagens::new();
        novo.entry("tela_nova.rs".into())
            .or_default()
            .insert("ui.label(".into(), 1);
        assert!(matches!(
            comparar(&base, &novo)[0],
            Divergencia::Subiu { antes: 0, .. }
        ));
    }
}
