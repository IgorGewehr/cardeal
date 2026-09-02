//! Lançamento e partida — a unidade de escrita do Razão.
//!
//! Ver `docs/05-nucleo-financeiro.md` §3.2 e §3.4.

use cardeal_kernel::{Data, Dinheiro, Id, Instante, Quantidade};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

/// O estado de um lançamento — o motor por trás das três visões simultâneas do sistema
/// (caixa, competência, projeção). Ver `docs/05-nucleo-financeiro.md` §3.4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EstadoLancamento {
    /// Ainda não é fato: recorrência projetada, estimativa. Entra só no fluxo futuro.
    Previsto,
    /// O fato ocorreu, mas o dinheiro não andou. Entra na DRE por competência.
    Confirmado,
    /// O dinheiro andou. Entra em tudo, inclusive saldo de caixa/banco.
    Realizado,
    /// Anulado por lançamento de estorno. Permanece na base; some dos saldos.
    Estornado,
}

impl EstadoLancamento {
    /// Verdadeiro se o lançamento conta para o saldo de disponibilidade (caixa/banco) —
    /// só o que de fato aconteceu.
    #[must_use]
    pub const fn conta_para_caixa(self) -> bool {
        matches!(self, Self::Realizado)
    }

    /// Verdadeiro se o lançamento conta para a DRE por competência.
    #[must_use]
    pub const fn conta_para_competencia(self) -> bool {
        matches!(self, Self::Confirmado | Self::Realizado)
    }

    /// Verdadeiro se o lançamento entra na projeção de fluxo futuro.
    #[must_use]
    pub const fn conta_para_projecao(self) -> bool {
        !matches!(self, Self::Estornado)
    }
}

/// Com quem a partida foi: cliente, fornecedor, funcionário ou sócio.
///
/// Guardar a contraparte na própria partida (em vez de só no cabeçalho do lançamento)
/// é o que permite `SELECT` de "saldo do cliente X" e "saldo do fornecedor Y" sem tabela
/// auxiliar — é a mesma tabela `razao_partida` que sustenta contas a receber e a pagar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Contraparte {
    /// Um cliente.
    Cliente(Id),
    /// Um fornecedor.
    Fornecedor(Id),
    /// Um funcionário.
    Funcionario(Id),
    /// Um sócio.
    Socio(Id),
    /// Qualquer outra pessoa cadastrada.
    Outro(Id),
}

impl Contraparte {
    /// O identificador da pessoa, qualquer que seja o papel.
    #[must_use]
    pub const fn id(self) -> Id {
        match self {
            Self::Cliente(id)
            | Self::Fornecedor(id)
            | Self::Funcionario(id)
            | Self::Socio(id)
            | Self::Outro(id) => id,
        }
    }
}

/// De onde veio o lançamento: qual módulo, que tipo de operação, qual agregado.
///
/// É o que permite `consultas::lancamentos_da_origem` reconstruir "quais lançamentos essa
/// venda gerou" sem que o Razão precise conhecer o conceito de "venda".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Origem {
    /// O id do módulo que originou o lançamento, ex.: `"pdv"`.
    pub modulo: String,
    /// O tipo de operação dentro do módulo, ex.: `"venda"`.
    pub tipo: String,
    /// O identificador do agregado de origem, quando existir.
    pub id: Option<Id>,
}

impl Origem {
    /// Começa a origem a partir do id do módulo.
    #[must_use]
    pub fn modulo(modulo: impl Into<String>) -> Self {
        Self {
            modulo: modulo.into(),
            tipo: String::new(),
            id: None,
        }
    }

    /// Define o tipo de operação.
    #[must_use]
    pub fn tipo(mut self, tipo: impl Into<String>) -> Self {
        self.tipo = tipo.into();
        self
    }

    /// Define o agregado de origem.
    #[must_use]
    pub const fn agregado(mut self, id: Id) -> Self {
        self.id = Some(id);
        self
    }
}

/// Uma linha do lançamento.
///
/// **Um único campo com sinal**, não `debito`/`credito` separados: positivo é débito,
/// negativo é crédito. O invariante do Razão vira uma soma. Ver
/// `docs/05-nucleo-financeiro.md` §3.2 e §3.3.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Partida {
    /// A conta afetada. Deve ser analítica.
    pub conta: Id,
    /// Positivo = débito, negativo = crédito.
    pub valor: Dinheiro,
    /// Com quem foi, quando aplicável (contas de cliente/fornecedor).
    pub contraparte: Option<Contraparte>,
    /// O centro de custo, quando aplicável.
    pub centro_custo: Option<Id>,
    /// O projeto, quando aplicável.
    pub projeto: Option<Id>,
    /// Referência a um documento externo (nota, cheque, boleto), como texto livre.
    pub documento: Option<String>,
    /// Quantidade movimentada, para partidas de conta de estoque — dá custo unitário
    /// de graça (`valor / quantidade`) sem tabela adicional.
    pub quantidade: Option<Quantidade>,
    /// Complemento livre do histórico, específico desta partida.
    pub complemento: Option<String>,
}

impl Partida {
    /// Verdadeiro se a partida é um débito (valor positivo).
    #[must_use]
    pub const fn e_debito(&self) -> bool {
        self.valor.e_positivo()
    }

    /// Verdadeiro se a partida é um crédito (valor negativo).
    #[must_use]
    pub const fn e_credito(&self) -> bool {
        self.valor.e_negativo()
    }
}

/// O cabeçalho e as partidas de um lançamento contábil.
///
/// Imutável após confirmado — corrigir é estornar (`docs/05-nucleo-financeiro.md` §6), nunca
/// `UPDATE`. Este tipo por si só **não garante** o balanceamento; quem garante é
/// [`crate::LancamentoBalanceado`], o único produzido por [`crate::ConstrutorLancamento`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lancamento {
    /// Identidade.
    pub id: Id,
    /// A empresa.
    pub empresa: Id,
    /// O número sequencial, legível, por empresa. Atribuído no commit.
    pub numero: u64,
    /// Data de competência — quando o fato ocorreu.
    pub competencia: Data,
    /// Data de vencimento, para lançamentos ainda não liquidados.
    pub vencimento: Option<Data>,
    /// Data em que o dinheiro efetivamente andou.
    pub liquidacao: Option<Data>,
    /// O estado atual.
    pub estado: EstadoLancamento,
    /// De onde veio.
    pub origem: Origem,
    /// Texto legível do histórico.
    pub historico: String,
    /// Se este lançamento é um estorno, aponta para o original.
    pub estorna: Option<Id>,
    /// Se este lançamento foi estornado, aponta para o estorno.
    pub estornado_por: Option<Id>,
    /// Quando foi criado.
    pub criado_em: Instante,
    /// Quem criou.
    pub criado_por: Id,
    /// De qual dispositivo.
    pub dispositivo: Id,
    /// As partidas. `SmallVec<4>` porque a maioria dos lançamentos do receituário
    /// (`docs/05-nucleo-financeiro.md` §5) tem entre 2 e 4 partidas — cabem inline,
    /// sem alocação no heap.
    pub partidas: SmallVec<[Partida; 4]>,
}

impl Lancamento {
    /// A soma de todas as partidas. Um lançamento correto tem soma zero.
    #[must_use]
    pub fn soma(&self) -> Dinheiro {
        self.partidas.iter().map(|p| p.valor).sum()
    }

    /// A soma de todos os débitos (partidas positivas).
    #[must_use]
    pub fn total_debitos(&self) -> Dinheiro {
        self.partidas
            .iter()
            .map(|p| p.valor)
            .filter(|v| v.e_positivo())
            .sum()
    }

    /// A soma de todos os créditos (partidas negativas), como valor positivo.
    #[must_use]
    pub fn total_creditos(&self) -> Dinheiro {
        self.partidas
            .iter()
            .map(|p| p.valor)
            .filter(|v| v.e_negativo())
            .sum::<Dinheiro>()
            .abs()
    }

    /// Verdadeiro se as partidas somam exatamente zero.
    #[must_use]
    pub fn esta_balanceado(&self) -> bool {
        self.soma().e_zero()
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn partida(conta: Id, valor: Dinheiro) -> Partida {
        Partida {
            conta,
            valor,
            contraparte: None,
            centro_custo: None,
            projeto: None,
            documento: None,
            quantidade: None,
            complemento: None,
        }
    }

    #[test]
    fn soma_debitos_e_creditos() {
        let caixa = Id::novo();
        let receita = Id::novo();
        let l = Lancamento {
            id: Id::novo(),
            empresa: Id::novo(),
            numero: 1,
            competencia: Data::hoje(cardeal_kernel::Fuso::BRASILIA),
            vencimento: None,
            liquidacao: None,
            estado: EstadoLancamento::Realizado,
            origem: Origem::modulo("pdv").tipo("venda"),
            historico: "Venda 1".into(),
            estorna: None,
            estornado_por: None,
            criado_em: Instante::agora(),
            criado_por: Id::novo(),
            dispositivo: Id::novo(),
            partidas: smallvec::smallvec![
                partida(caixa, Dinheiro::reais(100)),
                partida(receita, Dinheiro::reais(-100)),
            ],
        };
        assert!(l.esta_balanceado());
        assert_eq!(l.total_debitos(), Dinheiro::reais(100));
        assert_eq!(l.total_creditos(), Dinheiro::reais(100));
    }

    #[test]
    fn estados_alimentam_as_tres_visoes() {
        assert!(EstadoLancamento::Realizado.conta_para_caixa());
        assert!(!EstadoLancamento::Confirmado.conta_para_caixa());
        assert!(EstadoLancamento::Confirmado.conta_para_competencia());
        assert!(!EstadoLancamento::Previsto.conta_para_competencia());
        assert!(EstadoLancamento::Previsto.conta_para_projecao());
        assert!(!EstadoLancamento::Estornado.conta_para_projecao());
    }
}
