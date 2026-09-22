//! Camada 3 (organisms): peças de tela inteiras, montadas só a partir de átomos/moléculas.
//! `docs/12-ui-ux.md` §7.

mod agenda_calendario;
mod agenda_mes;
mod cartao;
mod confirmacao;
mod dialogo;
mod faixa_kpi;
mod grade;
mod grafico;
mod janela;
mod layout_tela;
mod notificacoes;
mod painel;
mod paleta_comandos;
mod sidebar;

pub use agenda_calendario::{AcaoAgenda, AgendaCalendario, BlocoAgenda, ModoCalendario, TagAgenda};
pub use agenda_mes::AgendaMes;
pub use cartao::Cartao;
pub use confirmacao::{dialogo_confirmacao, RespostaConfirmacao};
pub use dialogo::Dialogo;
pub use faixa_kpi::FaixaKpi;
pub use grade::{ColunaGrade, Direcao, Grade, LinhaGrade, Ordenacao, RespostaGrade};
pub use grafico::{GraficoBarras, GraficoBarrasHorizontais, ItemBarraHorizontal, SerieBarras};
pub use janela::Janela;
pub use layout_tela::LayoutTela;
pub use notificacoes::{notificar, Notificacao, Notificacoes, Tom};
pub use painel::Painel;
pub use paleta_comandos::{ItemComando, PaletaComandos};
pub use sidebar::{GrupoSidebar, ItemSidebar, Sidebar};
