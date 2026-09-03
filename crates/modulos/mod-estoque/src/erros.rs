//! Erros do módulo de estoque.

use cardeal_kernel::{CodigoErro, Detalhes, ErroDominio};

/// Tudo que pode dar errado no cadastro de produto, no saldo, no lote e no inventário.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ErroEstoque {
    /// NCM fora do formato de 8 dígitos.
    #[error("O NCM precisa ter 8 dígitos")]
    NcmInvalido,

    /// `controla_validade` sem `controla_lote` — validade só existe presa a um lote (§11.7).
    #[error("Controle de validade exige controle de lote")]
    ValidadeSemLote,

    /// Nome de produto vazio.
    #[error("O nome do produto não pode ser vazio")]
    NomeVazio,

    /// GTIN com dígito verificador inválido.
    #[error("Código de barras (GTIN) com dígito verificador inválido")]
    GtinInvalido,

    /// GTIN com comprimento fora de 8/12/13/14.
    #[error("GTIN precisa ter 8, 12, 13 ou 14 dígitos")]
    GtinComprimento,

    /// Variação pedida para um produto que não controla grade.
    #[error("Este produto não controla grade")]
    ProdutoSemGrade,

    /// Quantidade de movimento não positiva.
    #[error("A quantidade do movimento precisa ser maior que zero")]
    QuantidadeInvalida,

    /// Entrada/produção sem custo unitário.
    #[error("Entrada e produção exigem o custo unitário")]
    CustoUnitarioAusente,

    /// Reserva/consumo acima do disponível.
    #[error("Saldo insuficiente: disponível {disponivel}, pedido {pedido}")]
    SaldoInsuficiente {
        /// O disponível no momento.
        disponivel: String,
        /// O que foi pedido.
        pedido: String,
    },

    /// Liberação de uma reserva maior que a reservada.
    #[error("Não há reserva suficiente para liberar")]
    ReservaInsuficiente,

    /// Ajuste manual sem motivo com no mínimo 10 caracteres (§11.8).
    #[error("O ajuste manual exige um motivo com ao menos 10 caracteres")]
    MotivoObrigatorio,

    /// Transferência com origem e destino iguais.
    #[error("Origem e destino da transferência são o mesmo local")]
    LocalIgual,

    /// Movimento contra um local com inventário congelado (§11.4).
    #[error("O local está com inventário congelado — nenhum movimento é aceito agora")]
    LocalCongelado,

    /// Transição de estado inválida no inventário.
    #[error("O inventário está \"{atual}\", não \"{esperado}\"")]
    EstadoDeInventarioInvalido {
        /// O estado atual.
        atual: &'static str,
        /// O estado exigido.
        esperado: &'static str,
    },

    /// `EncerrarInventario` com itens ainda sem contagem.
    #[error("Ainda há {0} item(ns) sem contagem")]
    ContagensPendentes(usize),
}

impl ErroDominio for ErroEstoque {
    fn codigo(&self) -> CodigoErro {
        match self {
            Self::NcmInvalido
            | Self::ValidadeSemLote
            | Self::NomeVazio
            | Self::GtinComprimento
            | Self::QuantidadeInvalida
            | Self::CustoUnitarioAusente
            | Self::MotivoObrigatorio => CodigoErro::ENTRADA_INVALIDA,
            Self::GtinInvalido => CodigoErro::DOCUMENTO_INVALIDO,
            Self::ProdutoSemGrade | Self::LocalIgual | Self::ReservaInsuficiente => {
                CodigoErro::REGRA_VIOLADA
            }
            Self::SaldoInsuficiente { .. } => CodigoErro::ESTOQUE_INSUFICIENTE,
            Self::LocalCongelado => CodigoErro::RECURSO_TRAVADO,
            Self::EstadoDeInventarioInvalido { .. } | Self::ContagensPendentes(_) => {
                CodigoErro::ESTADO_INVALIDO
            }
        }
    }

    fn detalhes(&self) -> Option<Detalhes> {
        match self {
            Self::SaldoInsuficiente { disponivel, pedido } => Some(Detalhes::nova(
                "Estoque abaixo do pedido",
                format!(
                    "O disponível é {disponivel} e foram pedidos {pedido}. A venda pode seguir \
                     (o saldo negativo é registrado e sinalizado), mas confira a contagem física."
                ),
            )),
            Self::LocalCongelado => Some(Detalhes::nova(
                "Inventário em andamento",
                "Entradas e saídas neste local ficam bloqueadas até o inventário sair do \
                 estado Congelado — é o que impede a contagem de perseguir um alvo que se move.",
            )),
            _ => None,
        }
    }
}
