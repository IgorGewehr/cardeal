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

    /// Transferência sem nenhum produto.
    #[error("A transferência precisa de pelo menos um produto")]
    TransferenciaSemItens,

    /// Transferir mais do que a origem tem.
    #[error("{produto}: a origem tem {disponivel}, foram pedidos {pedido}")]
    SaldoInsuficienteNaOrigem {
        /// O produto.
        produto: String,
        /// O disponível na origem.
        disponivel: String,
        /// O que foi pedido.
        pedido: String,
    },

    /// Produto com lote ativo na origem: o lote guarda o local.
    #[error("{produto} tem peças com lote neste local: a transferência por lote ainda não existe")]
    TransferenciaComLote {
        /// O produto.
        produto: String,
    },

    /// Um local só pode ter um inventário aberto por vez.
    #[error("Este local já tem um inventário em aberto: encerre ou cancele o atual antes")]
    LocalJaEmInventario,

    /// Contagem aberta sem nenhum produto para contar.
    #[error("Não há nada para contar neste local (e grupo): nenhum produto com saldo registrado")]
    InventarioSemItens,

    /// `EncerrarInventario` com itens ainda sem contagem.
    #[error("Ainda há {0} item(ns) sem contagem")]
    ContagensPendentes(usize),

    /// Código de lote/peça (o do post-it) vazio.
    #[error("O código do lote/peça não pode ser vazio")]
    CodigoLoteVazio,

    /// `Lote` com `origem = AparelhoUsado` sem `aparelho_origem` informado.
    #[error("Peça de aparelho usado exige informar de qual aparelho de origem ela saiu")]
    AparelhoOrigemAusente,

    /// Descrição vazia no cadastro de um aparelho de origem (trade-in).
    #[error("A descrição do aparelho de origem não pode ser vazia")]
    DescricaoDoAparelhoVazia,

    /// Consumo de um lote específico além do que ele ainda tem disponível — diferente de
    /// [`ErroEstoque::SaldoInsuficiente`] (que é do saldo agregado do produto/local): este é
    /// sobre a peça física identificada pelo código, que já foi total ou parcialmente usada.
    #[error("Este lote/peça só tem {disponivel} disponível, foram pedidos {pedido}")]
    LoteSaldoInsuficiente {
        /// O disponível no lote.
        disponivel: String,
        /// O que foi pedido.
        pedido: String,
    },

    /// Lote referenciado que não existe (código digitado errado, ou já removido).
    #[error("Lote/peça não encontrado")]
    LoteInexistente,

    /// O lote informado é de um produto diferente do item que está sendo aplicado/movimentado.
    #[error("Este lote/peça é de outro produto")]
    LoteDeOutroProduto,
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
            | Self::MotivoObrigatorio
            | Self::CodigoLoteVazio
            | Self::AparelhoOrigemAusente
            | Self::DescricaoDoAparelhoVazia
            | Self::TransferenciaSemItens => CodigoErro::ENTRADA_INVALIDA,
            Self::GtinInvalido => CodigoErro::DOCUMENTO_INVALIDO,
            Self::ProdutoSemGrade
            | Self::LocalJaEmInventario
            | Self::TransferenciaComLote { .. }
            | Self::InventarioSemItens
            | Self::LocalIgual
            | Self::ReservaInsuficiente
            | Self::LoteDeOutroProduto => CodigoErro::REGRA_VIOLADA,
            Self::SaldoInsuficiente { .. }
            | Self::LoteSaldoInsuficiente { .. }
            | Self::SaldoInsuficienteNaOrigem { .. } => CodigoErro::ESTOQUE_INSUFICIENTE,
            Self::LocalCongelado => CodigoErro::RECURSO_TRAVADO,
            Self::EstadoDeInventarioInvalido { .. } | Self::ContagensPendentes(_) => {
                CodigoErro::ESTADO_INVALIDO
            }
            Self::LoteInexistente => CodigoErro::NAO_ENCONTRADO,
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
            Self::TransferenciaComLote { .. } => Some(Detalhes::nova(
                "Peça rastreada por lote",
                "O lote guarda o local onde a peça está; mover só o saldo deixaria o código do \
                 post-it apontando para o lugar errado. Por enquanto, dê saída da peça pelo lote \
                 na origem e entrada com o mesmo código no destino.",
            )),
            Self::LocalCongelado => Some(Detalhes::nova(
                "Inventário em andamento",
                "Entradas e saídas neste local ficam bloqueadas até o inventário sair do \
                 estado Congelado — é o que impede a contagem de perseguir um alvo que se move.",
            )),
            Self::LoteSaldoInsuficiente { disponivel, pedido } => Some(Detalhes::nova(
                "Peça específica indisponível",
                format!(
                    "O lote/peça identificado pelo código só tem {disponivel} disponível, mas \
                     foram pedidos {pedido} — confira se é o código certo ou se a peça já foi \
                     usada em outra OS."
                ),
            )),
            _ => None,
        }
    }
}
