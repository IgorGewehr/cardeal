//! Saldo por local, movimento e **custo médio ponderado móvel**.
//!
//! `docs/modulos/estoque.md` §3, §7 e §11. Regras críticas modeladas aqui:
//!
//! - **§11.1** — o custo médio é recalculado **a cada entrada, nunca a cada saída**; a
//!   saída sempre usa o custo vigente. Toda a conta em `i128` na escala de [`Preco`]
//!   (1e-6), jamais `f64`.
//! - **§11.2** — o saldo **pode ficar negativo** e isso é aceito, não corrigido às cegas;
//!   na entrada seguinte o custo trata o saldo negativo como zero (`max(saldo, 0)`).
//! - **§11.3** — reserva **não é saída**: move de disponível para reservada sem gerar
//!   movimento físico nem lançamento.

use cardeal_kernel::{Arredondamento, Dinheiro, Id, Instante, Preco, Quantidade, Versao};
use serde::{Deserialize, Serialize};

use crate::erros::ErroEstoque;

/// O tipo de um [`Movimento`] de estoque. O sinal do efeito no saldo vem daqui; a
/// `quantidade` do movimento é sempre positiva.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TipoMovimento {
    /// Entrada por compra/doação — recalcula o custo médio.
    Entrada,
    /// Saída por venda/consumo — usa o custo médio vigente.
    Saida,
    /// Saída da origem numa transferência.
    TransferenciaSaida,
    /// Entrada no destino numa transferência.
    TransferenciaEntrada,
    /// Ajuste de inventário para mais.
    AjustePositivo,
    /// Ajuste de inventário para menos.
    AjusteNegativo,
    /// Reserva (disponível → reservada).
    Reserva,
    /// Liberação de reserva (reservada → disponível).
    LiberacaoReserva,
    /// Entrada por produção — recalcula o custo médio.
    Producao,
    /// Perda (vencimento, quebra).
    Perda,
}

impl TipoMovimento {
    /// Verdadeiro se o tipo recalcula o custo médio (traz custo novo para dentro).
    #[must_use]
    pub const fn recalcula_custo(self) -> bool {
        matches!(
            self,
            Self::Entrada | Self::Producao | Self::TransferenciaEntrada
        )
    }
}

/// O saldo de um produto (ou variação) em um local.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaldoLocal {
    /// Identidade.
    pub id: Id,
    /// A empresa.
    pub empresa: Id,
    /// O produto.
    pub produto: Id,
    /// A variação de grade, quando aplicável.
    pub variacao: Option<Id>,
    /// O local.
    pub local: Id,
    /// Quantidade livre para venda/consumo. **Pode ser negativa** (§11.2).
    pub quantidade_disponivel: Quantidade,
    /// Quantidade reservada para pedidos confirmados. Nunca negativa.
    pub quantidade_reservada: Quantidade,
    /// Custo médio ponderado móvel, escala 1e-6.
    pub custo_medio: Preco,
    /// Quando foi atualizado por último.
    pub atualizado_em: Instante,
    /// Versão para bloqueio otimista — todo movimento faz `UPDATE ... WHERE versao = ?`.
    pub versao: Versao,
}

/// O efeito apurado de uma saída — o que o módulo consumidor usa para montar o CMV.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SaidaAplicada {
    /// A quantidade que saiu.
    pub quantidade: Quantidade,
    /// O custo médio unitário no momento da saída (não recalculado).
    pub custo_unitario: Preco,
    /// `quantidade × custo_unitario`, arredondado a centavos — o valor do CMV.
    pub custo_total: Dinheiro,
    /// Verdadeiro se a saída deixou (ou manteve) o disponível negativo — divergência a
    /// conferir (`docs/modulos/estoque.md` §11.2).
    pub gerou_divergencia: bool,
}

/// Recalcula o custo médio ponderado móvel ao entrar `qtd_entrada` a `custo_entrada`.
///
/// `novo = (max(saldo,0)·custo_atual + qtd_entrada·custo_entrada) / (max(saldo,0) + qtd_entrada)`
///
/// Tudo em `i128` na escala de [`Preco`]. Se o denominador for zero (nada entrando e
/// saldo ≤ 0) devolve `custo_atual` inalterado.
#[must_use]
pub fn custo_medio_movel(
    saldo: Quantidade,
    custo_atual: Preco,
    qtd_entrada: Quantidade,
    custo_entrada: Preco,
) -> Preco {
    let sq = i128::from(saldo.unidades_internas().max(0)); // 1e-4
    let qe = i128::from(qtd_entrada.unidades_internas().max(0)); // 1e-4
    let den = sq + qe; // 1e-4
    if den == 0 {
        return custo_atual;
    }
    let ca = i128::from(custo_atual.unidades_internas()); // 1e-6
    let ce = i128::from(custo_entrada.unidades_internas()); // 1e-6
    let num = sq * ca + qe * ce; // 1e-10
                                 // num/den → 1e-6, com arredondamento meio-para-cima (positivo).
    let novo = (2 * num + den) / (2 * den);
    Preco::interna(i64::try_from(novo).unwrap_or(i64::MAX))
}

impl SaldoLocal {
    /// Cria um saldo zerado.
    #[must_use]
    pub fn zerado(
        empresa: Id,
        produto: Id,
        variacao: Option<Id>,
        local: Id,
        agora: Instante,
    ) -> Self {
        Self {
            id: Id::novo(),
            empresa,
            produto,
            variacao,
            local,
            quantidade_disponivel: Quantidade::ZERO,
            quantidade_reservada: Quantidade::ZERO,
            custo_medio: Preco::ZERO,
            atualizado_em: agora,
            versao: Versao::INICIAL,
        }
    }

    /// Registra uma entrada: recalcula o custo médio e soma ao disponível.
    ///
    /// # Errors
    /// [`ErroEstoque::QuantidadeInvalida`], [`ErroEstoque::CustoUnitarioAusente`] (custo ≤ 0).
    pub fn entrada(
        &mut self,
        quantidade: Quantidade,
        custo_unitario: Preco,
        agora: Instante,
    ) -> Result<(), ErroEstoque> {
        if !quantidade.e_positiva() {
            return Err(ErroEstoque::QuantidadeInvalida);
        }
        if custo_unitario.unidades_internas() <= 0 {
            return Err(ErroEstoque::CustoUnitarioAusente);
        }
        self.custo_medio = custo_medio_movel(
            self.quantidade_disponivel,
            self.custo_medio,
            quantidade,
            custo_unitario,
        );
        self.quantidade_disponivel += quantidade;
        self.tocar(agora);
        Ok(())
    }

    /// Registra uma saída física (venda/consumo/perda). Não recalcula o custo. A saída é
    /// aceita mesmo sem saldo — a divergência é sinalizada, não bloqueada (§11.2).
    ///
    /// # Errors
    /// [`ErroEstoque::QuantidadeInvalida`].
    pub fn saida(
        &mut self,
        quantidade: Quantidade,
        agora: Instante,
    ) -> Result<SaidaAplicada, ErroEstoque> {
        if !quantidade.e_positiva() {
            return Err(ErroEstoque::QuantidadeInvalida);
        }
        self.quantidade_disponivel -= quantidade;
        self.tocar(agora);
        Ok(SaidaAplicada {
            quantidade,
            custo_unitario: self.custo_medio,
            custo_total: Dinheiro::de_total(
                quantidade,
                self.custo_medio,
                Arredondamento::MeioAcima,
            ),
            gerou_divergencia: self.quantidade_disponivel.e_negativa(),
        })
    }

    /// Move `quantidade` de disponível para reservada. **Bloqueia** se não houver disponível
    /// suficiente (`docs/modulos/estoque.md` §5, `ReservarEstoque`).
    ///
    /// # Errors
    /// [`ErroEstoque::QuantidadeInvalida`], [`ErroEstoque::SaldoInsuficiente`].
    pub fn reservar(&mut self, quantidade: Quantidade, agora: Instante) -> Result<(), ErroEstoque> {
        if !quantidade.e_positiva() {
            return Err(ErroEstoque::QuantidadeInvalida);
        }
        if self.quantidade_disponivel < quantidade {
            return Err(ErroEstoque::SaldoInsuficiente {
                disponivel: self.quantidade_disponivel.formatar(0),
                pedido: quantidade.formatar(0),
            });
        }
        self.quantidade_disponivel -= quantidade;
        self.quantidade_reservada += quantidade;
        self.tocar(agora);
        Ok(())
    }

    /// Devolve `quantidade` de reservada para disponível (pedido cancelado).
    ///
    /// # Errors
    /// [`ErroEstoque::QuantidadeInvalida`], [`ErroEstoque::ReservaInsuficiente`].
    pub fn liberar_reserva(
        &mut self,
        quantidade: Quantidade,
        agora: Instante,
    ) -> Result<(), ErroEstoque> {
        if !quantidade.e_positiva() {
            return Err(ErroEstoque::QuantidadeInvalida);
        }
        if self.quantidade_reservada < quantidade {
            return Err(ErroEstoque::ReservaInsuficiente);
        }
        self.quantidade_reservada -= quantidade;
        self.quantidade_disponivel += quantidade;
        self.tocar(agora);
        Ok(())
    }

    /// Consome uma reserva de verdade (venda faturada): a quantidade reservada sai do
    /// estoque físico. É a única forma de uma reserva virar CMV.
    ///
    /// # Errors
    /// [`ErroEstoque::QuantidadeInvalida`], [`ErroEstoque::ReservaInsuficiente`].
    pub fn consumir_reserva(
        &mut self,
        quantidade: Quantidade,
        agora: Instante,
    ) -> Result<SaidaAplicada, ErroEstoque> {
        if !quantidade.e_positiva() {
            return Err(ErroEstoque::QuantidadeInvalida);
        }
        if self.quantidade_reservada < quantidade {
            return Err(ErroEstoque::ReservaInsuficiente);
        }
        self.quantidade_reservada -= quantidade;
        self.tocar(agora);
        Ok(SaidaAplicada {
            quantidade,
            custo_unitario: self.custo_medio,
            custo_total: Dinheiro::de_total(
                quantidade,
                self.custo_medio,
                Arredondamento::MeioAcima,
            ),
            gerou_divergencia: false,
        })
    }

    /// Ajuste de saldo (inventário): aplica um delta com sinal ao disponível.
    pub fn ajustar(&mut self, delta: Quantidade, agora: Instante) {
        self.quantidade_disponivel += delta;
        self.tocar(agora);
    }

    fn tocar(&mut self, agora: Instante) {
        self.atualizado_em = agora;
        self.versao = self.versao.proxima();
    }
}

/// Um movimento de estoque — a própria ordem (`UUIDv7`) é o log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Movimento {
    /// Identidade.
    pub id: Id,
    /// A empresa.
    pub empresa: Id,
    /// O produto.
    pub produto: Id,
    /// A variação.
    pub variacao: Option<Id>,
    /// O local.
    pub local: Id,
    /// O tipo.
    pub tipo: TipoMovimento,
    /// A quantidade, sempre positiva.
    pub quantidade: Quantidade,
    /// O custo unitário (obrigatório em `Entrada`/`Producao`).
    pub custo_unitario: Option<Preco>,
    /// O lote, quando o produto o exige.
    pub lote: Option<Id>,
    /// Módulo que originou o movimento.
    pub origem_modulo: String,
    /// Agregado de origem.
    pub origem_id: Option<Id>,
    /// Lançamento no Razão, quando o próprio estoque o posta (`docs/modulos/estoque.md` §7).
    pub lancamento: Option<Id>,
    /// Quando.
    pub criado_em: Instante,
    /// Quem.
    pub criado_por: Id,
}

#[cfg(test)]
mod testes {
    use proptest::prelude::*;

    use super::*;

    fn agora() -> Instante {
        Instante::agora()
    }

    fn saldo() -> SaldoLocal {
        SaldoLocal::zerado(Id::novo(), Id::novo(), None, Id::novo(), agora())
    }

    #[test]
    fn custo_medio_recalcula_so_na_entrada() {
        let mut s = saldo();
        s.entrada(Quantidade::unidades(100), Preco::reais(4), agora())
            .unwrap();
        assert_eq!(s.custo_medio, Preco::reais(4));

        s.entrada(Quantidade::unidades(100), Preco::reais(6), agora())
            .unwrap();
        // (100*4 + 100*6) / 200 = 5
        assert_eq!(s.custo_medio, Preco::reais(5));

        // Saída não muda o custo.
        let saida = s.saida(Quantidade::unidades(50), agora()).unwrap();
        assert_eq!(saida.custo_unitario, Preco::reais(5));
        assert_eq!(saida.custo_total, Dinheiro::reais(250));
        assert_eq!(s.custo_medio, Preco::reais(5));
        assert_eq!(s.quantidade_disponivel, Quantidade::unidades(150));
    }

    #[test]
    fn saldo_negativo_e_aceito_e_sinalizado() {
        let mut s = saldo();
        s.entrada(Quantidade::unidades(10), Preco::reais(2), agora())
            .unwrap();
        let saida = s.saida(Quantidade::unidades(15), agora()).unwrap();
        assert!(saida.gerou_divergencia);
        assert_eq!(s.quantidade_disponivel, Quantidade::unidades(-5));

        // Entrada seguinte trata o saldo negativo como zero no cálculo do custo.
        s.entrada(Quantidade::unidades(5), Preco::reais(10), agora())
            .unwrap();
        assert_eq!(s.custo_medio, Preco::reais(10));
    }

    #[test]
    fn reserva_move_sem_saida_e_bloqueia_sem_saldo() {
        let mut s = saldo();
        s.entrada(Quantidade::unidades(10), Preco::reais(2), agora())
            .unwrap();
        s.reservar(Quantidade::unidades(4), agora()).unwrap();
        assert_eq!(s.quantidade_disponivel, Quantidade::unidades(6));
        assert_eq!(s.quantidade_reservada, Quantidade::unidades(4));
        assert_eq!(s.custo_medio, Preco::reais(2)); // reserva não mexe no custo

        assert!(matches!(
            s.reservar(Quantidade::unidades(999), agora()).unwrap_err(),
            ErroEstoque::SaldoInsuficiente { .. }
        ));

        s.liberar_reserva(Quantidade::unidades(1), agora()).unwrap();
        assert_eq!(s.quantidade_reservada, Quantidade::unidades(3));
        assert_eq!(s.quantidade_disponivel, Quantidade::unidades(7));

        let saida = s
            .consumir_reserva(Quantidade::unidades(3), agora())
            .unwrap();
        assert_eq!(saida.quantidade, Quantidade::unidades(3));
        assert_eq!(s.quantidade_reservada, Quantidade::ZERO);
    }

    #[test]
    fn quantidade_zero_e_recusada() {
        let mut s = saldo();
        assert_eq!(
            s.entrada(Quantidade::ZERO, Preco::reais(1), agora())
                .unwrap_err(),
            ErroEstoque::QuantidadeInvalida
        );
        assert_eq!(
            s.saida(Quantidade::ZERO, agora()).unwrap_err(),
            ErroEstoque::QuantidadeInvalida
        );
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(500))]

        /// O custo médio fica sempre entre o menor e o maior custo já entrado, para
        /// qualquer sequência de entradas positivas.
        #[test]
        fn custo_medio_fica_entre_o_min_e_o_max(
            entradas in prop::collection::vec(
                (1i64..=10_000i64, 1i64..=1_000_000i64),
                1..30,
            ),
        ) {
            let hoje = agora();
            let mut s = saldo();
            let mut menor = i64::MAX;
            let mut maior = i64::MIN;
            for (q, c) in entradas {
                menor = menor.min(c);
                maior = maior.max(c);
                s.entrada(Quantidade::interna(q), Preco::interna(c), hoje).unwrap();
                prop_assert!(s.custo_medio.unidades_internas() >= menor);
                prop_assert!(s.custo_medio.unidades_internas() <= maior);
            }
        }
    }
}
