//! Camada 0 (ions) — tipografia. `docs/12-ui-ux.md` §3.
//!
//! O doc pede **Inter** (interface), **Inter Tabular** (números) e **`JetBrains` Mono**
//! (código), embutidas no binário. Enquanto os arquivos `.ttf` da marca não estão no
//! repositório, [`instalar_fontes`] carrega os substitutos que **todo Windows já tem**:
//!
//! - **Segoe UI** como fonte de interface — moderna, nativa, no espírito da Inter;
//! - **Segoe UI Semibold/Bold** como a família `cardeal-forte`, dando peso **de verdade** aos
//!   títulos (o `RichText::strong` do egui só muda a cor, não engorda o traço);
//! - **Cascadia Mono** (ou Consolas) como monoespaçada — e também como a família
//!   `cardeal-num`: monoespaçada é perfeitamente tabular, então as colunas de dinheiro
//!   alinham a vírgula sem depender da feature `tnum`.
//!
//! Quando `crates/cardeal-ui/assets/fontes/` ganhar os arquivos reais, `instalar_fontes`
//! passa a embuti-los e o visual fica idêntico em qualquer máquina.

use egui::{FontData, FontDefinitions, FontFamily, FontId, RichText};

use super::cores::Cores;

/// A família de peso forte (títulos) — Segoe UI Semibold quando disponível.
const FAMILIA_FORTE: &str = "cardeal-forte";
/// A família tabular (números, dinheiro) — monoespaçada.
const FAMILIA_NUM: &str = "cardeal-num";

/// Um papel tipográfico — tamanho, peso e família. `docs/12-ui-ux.md` §3.
///
/// **Escala aumentada em 2026-09-11** (revisão de UI/UX pedida pelo usuário: "as coisas
/// estão muito pequenas" — evidenciado por capturas de tela reais do app rodando em
/// 1920×1080 com o conteúdo espremido e ilegível de longe). Valores antigos entre
/// parênteses. O aumento é só aqui — todo componente que já lê `Papel` cresce junto, sem
/// precisar mexer em cada tela.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Papel {
    /// Texto de interface padrão. 14px / 400 (era 13px).
    Interface,
    /// Título de tela. 23px / 600 (era 20px).
    TituloTela,
    /// Título de seção. 17px / 600 (era 15px).
    TituloSecao,
    /// Rótulo de campo. 13px / 500, `texto_medio` (era 12px).
    RotuloCampo,
    /// Números e dinheiro — tabular. 14px / 500 (era 13px).
    Numero,
    /// Valor em destaque (Pulso). 36px / 600, tabular (era 32px).
    ValorDestaque,
    /// Código, chave de acesso, log. 13px / 400, monoespaçada (era 12px).
    Codigo,
}

impl Papel {
    /// Tamanho em pixels lógicos.
    #[must_use]
    pub const fn tamanho(self) -> f32 {
        match self {
            Self::TituloTela => 23.0,
            Self::ValorDestaque => 36.0,
            Self::TituloSecao => 17.0,
            Self::RotuloCampo | Self::Codigo => 13.0,
            Self::Interface | Self::Numero => 14.0,
        }
    }

    /// Verdadeiro para papéis que ganham um leve reforço de contraste (`RichText::strong`).
    /// O **peso** de verdade vem da família (`cardeal-forte`), não daqui.
    #[must_use]
    pub const fn enfatico(self) -> bool {
        matches!(
            self,
            Self::TituloTela | Self::TituloSecao | Self::ValorDestaque
        )
    }

    /// Verdadeiro para papéis que usam algarismos tabulares.
    #[must_use]
    pub const fn tabular(self) -> bool {
        matches!(self, Self::Numero | Self::ValorDestaque)
    }

    #[must_use]
    fn familia(self) -> FontFamily {
        match self {
            Self::Codigo => FontFamily::Monospace,
            // Só as colunas de tabela usam a monoespaçada (alinham a vírgula). O valor de
            // destaque é grande e único — proporcional, com peso.
            Self::Numero => FontFamily::Name(FAMILIA_NUM.into()),
            Self::TituloTela | Self::TituloSecao | Self::ValorDestaque => {
                FontFamily::Name(FAMILIA_FORTE.into())
            }
            Self::Interface | Self::RotuloCampo => FontFamily::Proportional,
        }
    }

    /// A fonte/tamanho `egui` correspondente a este papel.
    #[must_use]
    pub fn font_id(self) -> FontId {
        FontId::new(self.tamanho(), self.familia())
    }

    /// Aplica o papel a um texto, na cor apropriada do tema.
    #[must_use]
    pub fn texto(self, conteudo: impl Into<String>, cores: &Cores) -> RichText {
        let mut rt = RichText::new(conteudo)
            .font(self.font_id())
            .color(self.cor_padrao(cores));
        if self.enfatico() {
            rt = rt.strong();
        }
        rt
    }

    #[must_use]
    const fn cor_padrao(self, cores: &Cores) -> egui::Color32 {
        match self {
            Self::RotuloCampo => cores.texto_medio,
            Self::TituloTela | Self::ValorDestaque => cores.texto_forte,
            _ => cores.texto,
        }
    }
}

fn ler(defs: &mut FontDefinitions, nome: &str, caminhos: &[&str]) -> bool {
    for c in caminhos {
        if let Ok(bytes) = std::fs::read(c) {
            defs.font_data
                .insert(nome.to_owned(), FontData::from_owned(bytes));
            return true;
        }
    }
    false
}

/// Instala as fontes de interface. Best-effort: se um arquivo não existir, o egui segue com
/// a fonte que já tinha para aquele papel. No Windows os três substitutos existem desde o
/// Windows 10/11.
pub fn instalar_fontes(ctx: &egui::Context) {
    let mut defs = FontDefinitions::default();

    let tem_ui = ler(&mut defs, "cardeal-ui", &[r"C:\Windows\Fonts\segoeui.ttf"]);
    let tem_forte = ler(
        &mut defs,
        "cardeal-ui-forte",
        &[
            r"C:\Windows\Fonts\seguisb.ttf",
            r"C:\Windows\Fonts\segoeuib.ttf",
        ],
    );
    let tem_mono = ler(
        &mut defs,
        "cardeal-mono",
        &[
            r"C:\Windows\Fonts\CascadiaMono.ttf",
            r"C:\Windows\Fonts\CascadiaCode.ttf",
            r"C:\Windows\Fonts\consola.ttf",
        ],
    );

    if tem_ui {
        if let Some(fam) = defs.families.get_mut(&FontFamily::Proportional) {
            fam.insert(0, "cardeal-ui".to_owned());
        }
    }
    if tem_mono {
        if let Some(fam) = defs.families.get_mut(&FontFamily::Monospace) {
            fam.insert(0, "cardeal-mono".to_owned());
        }
    }

    // A família de fallback para os nomes customizados, quando o arquivo não carregou.
    let base_prop = defs
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let base_mono = defs
        .families
        .get(&FontFamily::Monospace)
        .cloned()
        .unwrap_or_default();

    let forte = if tem_forte {
        let mut v = vec!["cardeal-ui-forte".to_owned()];
        v.extend(base_prop.iter().cloned());
        v
    } else {
        base_prop.clone()
    };
    defs.families
        .insert(FontFamily::Name(FAMILIA_FORTE.into()), forte);
    defs.families
        .insert(FontFamily::Name(FAMILIA_NUM.into()), base_mono);

    ctx.set_fonts(defs);
    tracing::debug!(
        tem_ui,
        tem_forte,
        tem_mono,
        "instalar_fontes: substitutos do Windows (Inter/JetBrains Mono ainda não embutidas)"
    );
}
