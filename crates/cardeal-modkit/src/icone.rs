//! Os ícones que um módulo pode usar na sidebar e no menu.
//!
//! `cardeal-ui` é quem sabe desenhar cada um (`docs/12-ui-ux.md` §7); aqui só a identidade —
//! um módulo escolhe `Icone::Estoque`, nunca um caminho de arquivo ou glifo de fonte.

use std::fmt;

/// Um ícone do sistema.
///
/// `#[non_exhaustive]`: módulos futuros vão precisar de ícones novos (combustível, quarto
/// de hotel, ordem de produção). Acrescentar uma variante não quebra `match` existente que
/// tenha um braço `_`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Icone {
    /// O Pulso — a tela inicial financeira.
    Pulso,
    /// Dinheiro, financeiro em geral.
    Dinheiro,
    /// Carrinho de compras, vendas.
    Carrinho,
    /// Caixa registradora, PDV.
    Caixa,
    /// Pessoas, clientes.
    Pessoas,
    /// Estoque, produtos.
    Estoque,
    /// Nota fiscal, documentos.
    Nota,
    /// Gráfico, análises.
    Grafico,
    /// Agenda, compromissos.
    Agenda,
    /// Ferramenta, ordem de serviço.
    Ferramenta,
    /// Chave, segurança/permissões.
    Chave,
    /// Engrenagem, configurações.
    Config,
    /// Banco, contas bancárias.
    Banco,
    /// Conciliação.
    Conciliar,
    /// Alerta, pendência.
    Alerta,
}

impl fmt::Display for Icone {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
