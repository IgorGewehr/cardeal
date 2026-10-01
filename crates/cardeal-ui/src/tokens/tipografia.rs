//! Camada 0 (ions) — tipografia. `docs/12-ui-ux.md` §3.
//!
//! As fontes da marca vão **embutidas no binário** (`assets/fontes/`, licença OFL):
//!
//! - **Inter** Regular / Medium / semibold — interface, rótulos/botões e títulos. Cada peso
//!   é uma família própria (`Proportional`, `cardeal-medio`, `cardeal-forte`), porque o egui
//!   não lê fonte variável e o `RichText::strong` só muda a cor, não engorda o traço;
//! - **`JetBrains` Mono** Regular / Medium — código e a família `cardeal-num` das colunas de
//!   dinheiro (monoespaçada é tabular por construção: a vírgula alinha).
//!
//! Antes (até 2026-10-01) as fontes eram lidas de `C:\Windows\Fonts`: no Linux nada
//! carregava e **todo** papel caía na mesma fonte e peso do egui — título, rótulo, botão e
//! explicação só se distinguiam pelo tamanho. A hierarquia agora vem de três eixos juntos:
//! tamanho, peso e cor (`texto_forte` → `texto` → `texto_medio`).

use egui::{FontData, FontDefinitions, FontFamily, FontId, RichText};

use super::cores::Cores;

/// A família de peso forte (títulos, valores) — Inter semibold.
const FAMILIA_FORTE: &str = "cardeal-forte";
/// A família de peso médio (rótulos, botões, abas) — Inter Medium.
const FAMILIA_MEDIA: &str = "cardeal-medio";
/// A família tabular (números, dinheiro) — monoespaçada.
const FAMILIA_NUM: &str = "cardeal-num";

/// Um papel tipográfico — tamanho, peso e família. `docs/12-ui-ux.md` §3.
///
/// A escala (2026-10-01): títulos e valores em semibold e `texto_forte`; rótulos, botões e
/// sobrelinhas em Medium; corpo em Regular; explicação (`Apoio`) menor e em `texto_medio`.
/// Dois papéis vizinhos nunca diferem só em um eixo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Papel {
    /// Texto de interface padrão (corpo, célula de tabela). 14px / Regular, `texto`.
    Interface,
    /// Título de tela. 24px / semibold, `texto_forte`.
    TituloTela,
    /// Título de seção, de cartão e de diálogo. 16px / semibold, `texto_forte`.
    TituloSecao,
    /// Rótulo de campo, cabeçalho de coluna, aba. 13px / Medium, `texto_medio`.
    RotuloCampo,
    /// Rótulo de botão. 14px / Medium.
    Acao,
    /// Sobrelinha: o rótulo curto em CAIXA ALTA espaçada acima de um valor ou grupo
    /// ("A RECEBER EM ABERTO", "PERÍODO", grupos da barra lateral). 11px / semibold,
    /// `texto_medio`. Use [`Papel::preparar`] (ou `Rotulo::sobrelinha`) para a caixa alta.
    Sobrelinha,
    /// Texto de apoio: explicação, dica, descrição de diálogo, linha secundária de um cartão.
    /// 13px / Regular, `texto_medio`.
    Apoio,
    /// Números e dinheiro em tabela — tabular. 13px / `JetBrains` Mono Medium.
    Numero,
    /// Valor em destaque (KPI). 30px / semibold, `texto_forte`.
    ValorDestaque,
    /// Código, chave de acesso, log, tecla de atalho. 12px / `JetBrains` Mono.
    Codigo,
}

impl Papel {
    /// Tamanho em pixels lógicos.
    #[must_use]
    pub const fn tamanho(self) -> f32 {
        match self {
            Self::ValorDestaque => 30.0,
            Self::TituloTela => 24.0,
            Self::TituloSecao => 16.0,
            Self::Interface | Self::Acao => 14.0,
            Self::RotuloCampo | Self::Apoio | Self::Numero => 13.0,
            Self::Codigo => 12.0,
            Self::Sobrelinha => 11.0,
        }
    }

    /// Espaço extra entre letras (px) — só a sobrelinha, que é caixa alta pequena.
    #[must_use]
    pub const fn espacamento(self) -> f32 {
        match self {
            Self::Sobrelinha => 0.8,
            _ => 0.0,
        }
    }

    /// O texto como o papel o escreve (a sobrelinha é sempre caixa alta).
    #[must_use]
    pub fn preparar(self, texto: String) -> String {
        match self {
            Self::Sobrelinha => texto.to_uppercase(),
            _ => texto,
        }
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
            Self::TituloTela | Self::TituloSecao | Self::ValorDestaque | Self::Sobrelinha => {
                FontFamily::Name(FAMILIA_FORTE.into())
            }
            Self::RotuloCampo | Self::Acao => FontFamily::Name(FAMILIA_MEDIA.into()),
            Self::Interface | Self::Apoio => FontFamily::Proportional,
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
        RichText::new(self.preparar(conteudo.into()))
            .font(self.font_id())
            .color(self.cor_padrao(cores))
            .extra_letter_spacing(self.espacamento())
    }

    /// A cor do papel no tema.
    #[must_use]
    pub const fn cor_padrao(self, cores: &Cores) -> egui::Color32 {
        match self {
            Self::RotuloCampo | Self::Sobrelinha | Self::Apoio => cores.texto_medio,
            Self::TituloTela | Self::TituloSecao | Self::ValorDestaque => cores.texto_forte,
            _ => cores.texto,
        }
    }
}

/// Instala as fontes embutidas e as famílias de peso. Os glifos que a Inter não tem (setas,
/// símbolos) caem nas fontes padrão do egui, mantidas como reserva no fim de cada família.
pub fn instalar_fontes(ctx: &egui::Context) {
    const INTER_REGULAR: &[u8] = include_bytes!("../../assets/fontes/Inter-Regular.ttf");
    const INTER_MEDIUM: &[u8] = include_bytes!("../../assets/fontes/Inter-Medium.ttf");
    const INTER_SEMIBOLD: &[u8] = include_bytes!("../../assets/fontes/Inter-SemiBold.ttf");
    const MONO_REGULAR: &[u8] = include_bytes!("../../assets/fontes/JetBrainsMono-Regular.ttf");
    const MONO_MEDIUM: &[u8] = include_bytes!("../../assets/fontes/JetBrainsMono-Medium.ttf");

    let mut defs = FontDefinitions::default();
    for (nome, bytes) in [
        ("inter-regular", INTER_REGULAR),
        ("inter-medium", INTER_MEDIUM),
        ("inter-semibold", INTER_SEMIBOLD),
        ("mono-regular", MONO_REGULAR),
        ("mono-medium", MONO_MEDIUM),
    ] {
        defs.font_data
            .insert(nome.to_owned(), FontData::from_static(bytes));
    }

    let reserva_prop = defs
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let reserva_mono = defs
        .families
        .get(&FontFamily::Monospace)
        .cloned()
        .unwrap_or_default();
    let com_reserva = |primeira: &str, reserva: &[String]| {
        let mut v = vec![primeira.to_owned()];
        v.extend(reserva.iter().cloned());
        v
    };

    defs.families.insert(
        FontFamily::Proportional,
        com_reserva("inter-regular", &reserva_prop),
    );
    defs.families.insert(
        FontFamily::Name(FAMILIA_MEDIA.into()),
        com_reserva("inter-medium", &reserva_prop),
    );
    defs.families.insert(
        FontFamily::Name(FAMILIA_FORTE.into()),
        com_reserva("inter-semibold", &reserva_prop),
    );
    defs.families.insert(
        FontFamily::Monospace,
        com_reserva("mono-regular", &reserva_mono),
    );
    defs.families.insert(
        FontFamily::Name(FAMILIA_NUM.into()),
        com_reserva("mono-medium", &reserva_mono),
    );
    ctx.set_fonts(defs);
}
