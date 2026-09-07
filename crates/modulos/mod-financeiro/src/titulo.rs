//! Título e parcela — a obrigação (a receber ou a pagar) e suas frações no tempo.
//!
//! Ver `docs/modulos/financeiro.md` §3 e §4. Domínio puro: nenhum acesso a banco, nenhum
//! `async`. O construtor só entrega parcelas cuja soma é **exatamente** igual ao valor do
//! título (`docs/modulos/financeiro.md` §11.2) — a mesma disciplina do
//! [`ConstrutorLancamento`](cardeal_ledger::ConstrutorLancamento) do Razão.

use cardeal_kernel::{Data, Dinheiro, Id, Instante, Percentual, Versao};
use cardeal_ledger::Contraparte;
use serde::{Deserialize, Serialize};

use crate::erros::ErroFinanceiro;

/// Se o título é um direito (a receber) ou uma obrigação (a pagar).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EspecieTitulo {
    /// Alguém deve à empresa.
    Receber,
    /// A empresa deve a alguém.
    Pagar,
}

impl EspecieTitulo {
    /// O rótulo em português.
    #[must_use]
    pub const fn rotulo(self) -> &'static str {
        match self {
            Self::Receber => "a receber",
            Self::Pagar => "a pagar",
        }
    }
}

/// Como a cobrança do título é (ou será) operacionalizada.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FormaCobranca {
    /// Boleto bancário registrado.
    Boleto,
    /// Pix (cobrança com QR dinâmico).
    Pix,
    /// Carteira — a empresa cobra por conta própria, sem banco.
    Carteira,
    /// Débito automático em conta.
    DebitoAutomatico,
    /// Cartão (recorrência ou link).
    Cartao,
}

/// A política de juros de mora de uma parcela em atraso.
///
/// Juros e multa **nunca são materializados fora da baixa** (`docs/modulos/financeiro.md`
/// §11.1): a parcela guarda só a política e a taxa; o valor é calculado na leitura, a partir
/// da data de referência. Ver [`crate::SituacaoParcela`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PoliticaJuros {
    /// Sem juros de mora.
    Nenhum,
    /// Juros simples calculados por dia de atraso.
    SimplesDiario,
    /// Juros simples calculados por mês de atraso (proporcional aos dias, base 30).
    SimplesMensal,
}

impl PoliticaJuros {
    /// O rótulo em português.
    #[must_use]
    pub const fn rotulo(self) -> &'static str {
        match self {
            Self::Nenhum => "sem juros",
            Self::SimplesDiario => "juros simples ao dia",
            Self::SimplesMensal => "juros simples ao mês",
        }
    }

    /// Verdadeiro se a política exige uma taxa informada.
    #[must_use]
    pub const fn exige_taxa(self) -> bool {
        !matches!(self, Self::Nenhum)
    }
}

/// O estado de uma parcela na sua máquina (`docs/modulos/financeiro.md` §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EstadoParcela {
    /// Nenhuma baixa ainda.
    Aberta,
    /// Baixada em parte; ainda há saldo.
    Parcial,
    /// Saldo zerado por baixa(s).
    Quitada,
    /// O título foi cancelado (estorno do `Confirmado` original).
    Cancelada,
    /// A parcela foi absorvida por uma renegociação e substituída por outro título.
    Renegociada,
}

impl EstadoParcela {
    /// Verdadeiro se a parcela ainda pode receber baixa.
    #[must_use]
    pub const fn aceita_baixa(self) -> bool {
        matches!(self, Self::Aberta | Self::Parcial)
    }
}

/// A obrigação inteira, antes de fatiada em parcelas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Titulo {
    /// Identidade.
    pub id: Id,
    /// A empresa.
    pub empresa: Id,
    /// A receber ou a pagar.
    pub especie: EspecieTitulo,
    /// Com quem — cliente, fornecedor, funcionário ou sócio (o mesmo tipo do Razão).
    pub contraparte: Contraparte,
    /// O módulo que originou o título (`"vendas"`, `"compras"`, `"avulso"`…).
    pub origem_modulo: String,
    /// O id do agregado de origem, quando houver.
    pub origem_id: Option<Id>,
    /// Data de emissão.
    pub emissao: Data,
    /// A soma das parcelas na criação — imutável.
    pub valor_original: Dinheiro,
    /// Como a cobrança é feita.
    pub forma_cobranca: FormaCobranca,
    /// Centro de custo, quando o submódulo está ativo.
    pub centro_custo: Option<Id>,
    /// Categoria livre para relatório (`docs/modulos/financeiro.md` §11.9) — "Aluguel",
    /// "Assinatura `SaaS` — Cliente X". Não participa da contabilização; só alimenta
    /// agregações por categoria/mês.
    pub categoria: Option<Id>,
    /// Observação livre.
    pub observacao: Option<String>,
    /// Preenchido quando o título é cancelado — nunca é apagado.
    pub cancelado_em: Option<Instante>,
    /// Versão para bloqueio otimista.
    pub versao: Versao,
}

impl Titulo {
    /// Verdadeiro se o título foi cancelado.
    #[must_use]
    pub const fn esta_cancelado(&self) -> bool {
        self.cancelado_em.is_some()
    }
}

/// Uma fração do título, com vencimento próprio.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Parcela {
    /// Identidade.
    pub id: Id,
    /// A empresa.
    pub empresa: Id,
    /// O título dono.
    pub titulo: Id,
    /// Ordinal dentro do título, de 1 a N.
    pub numero: u16,
    /// Quando vence.
    pub vencimento: Data,
    /// O valor original da parcela — resultado de [`Dinheiro::ratear`] sobre o título.
    pub valor: Dinheiro,
    /// O estado atual.
    pub estado: EstadoParcela,
    /// A soma de tudo que já foi baixado (só o principal, não juros nem multa).
    pub valor_baixado: Dinheiro,
    /// O `Lancamento` `Confirmado` que representa a parcela no Razão. `None` enquanto o
    /// título ainda não foi postado (estado transitório do domínio puro).
    pub lancamento: Option<Id>,
    /// "Nosso número" atribuído pela cobrança, quando gerada.
    pub nosso_numero: Option<String>,
    /// A política de juros de mora.
    pub politica_juros: PoliticaJuros,
    /// A taxa de juros (por dia ou por mês, conforme a política). Obrigatória se a política
    /// não for [`PoliticaJuros::Nenhum`].
    pub taxa_juros: Option<Percentual>,
    /// A multa por atraso, aplicada **uma única vez** no primeiro dia de atraso.
    pub multa: Option<Percentual>,
    /// Até quando vale o desconto por antecipação.
    pub desconto_ate_data: Option<Data>,
    /// O valor do desconto por antecipação, válido só até `desconto_ate_data`.
    pub desconto_valor: Option<Dinheiro>,
    /// Versão para bloqueio otimista.
    pub versao: Versao,
}

impl Parcela {
    /// O saldo ainda em aberto do principal (sem juros nem multa).
    #[must_use]
    pub fn saldo_principal(&self) -> Dinheiro {
        (self.valor - self.valor_baixado).nao_negativo()
    }

    /// Verdadeiro se ainda há principal a receber e o estado permite baixa.
    #[must_use]
    pub fn esta_em_aberto(&self) -> bool {
        self.estado.aceita_baixa() && self.saldo_principal().e_positivo()
    }
}

/// Um título com suas parcelas já rateadas — o que [`ConstrutorTitulo::construir`] entrega.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TituloComParcelas {
    /// O cabeçalho.
    pub titulo: Titulo,
    /// As parcelas, na ordem (1..N). A soma dos `valor` é exatamente `titulo.valor_original`.
    pub parcelas: Vec<Parcela>,
}

/// Monta um [`Titulo`] e suas [`Parcela`]s garantindo que a soma feche com o total.
///
/// ```
/// use cardeal_kernel::{Data, Dinheiro, Fuso, Id};
/// use cardeal_ledger::Contraparte;
/// use mod_financeiro::{ConstrutorTitulo, EspecieTitulo};
///
/// let hoje = Data::hoje(Fuso::BRASILIA);
/// let tcp = ConstrutorTitulo::novo(
///     Id::novo(),
///     EspecieTitulo::Receber,
///     Contraparte::Cliente(Id::novo()),
///     Dinheiro::reais(100),
///     hoje,
/// )
/// .parcelas(3, hoje.mais_dias(30), 30)
/// .construir()
/// .unwrap();
///
/// assert_eq!(tcp.parcelas.len(), 3);
/// let soma: Dinheiro = tcp.parcelas.iter().map(|p| p.valor).sum();
/// assert_eq!(soma, Dinheiro::reais(100));
/// assert_eq!(tcp.parcelas[0].valor, Dinheiro::centavos(3334)); // a "quebrada" é a primeira
/// ```
pub struct ConstrutorTitulo {
    empresa: Id,
    especie: EspecieTitulo,
    contraparte: Contraparte,
    valor_total: Dinheiro,
    emissao: Data,
    origem_modulo: String,
    origem_id: Option<Id>,
    forma_cobranca: FormaCobranca,
    centro_custo: Option<Id>,
    categoria: Option<Id>,
    observacao: Option<String>,
    n_parcelas: u16,
    primeiro_vencimento: Option<Data>,
    intervalo_dias: i32,
    politica_juros: PoliticaJuros,
    taxa_juros: Option<Percentual>,
    multa: Option<Percentual>,
    desconto_ate_data: Option<Data>,
    desconto_valor: Option<Dinheiro>,
}

impl ConstrutorTitulo {
    /// Começa um título novo. `emissao` também é o vencimento padrão da 1ª parcela até que
    /// [`Self::parcelas`] diga o contrário.
    pub fn novo(
        empresa: Id,
        especie: EspecieTitulo,
        contraparte: Contraparte,
        valor_total: Dinheiro,
        emissao: Data,
    ) -> Self {
        Self {
            empresa,
            especie,
            contraparte,
            valor_total,
            emissao,
            origem_modulo: "avulso".to_string(),
            origem_id: None,
            forma_cobranca: FormaCobranca::Carteira,
            centro_custo: None,
            categoria: None,
            observacao: None,
            n_parcelas: 1,
            primeiro_vencimento: None,
            intervalo_dias: 30,
            politica_juros: PoliticaJuros::Nenhum,
            taxa_juros: None,
            multa: None,
            desconto_ate_data: None,
            desconto_valor: None,
        }
    }

    /// De qual módulo e agregado vem o título (`vendas`, `compras`, `os`…).
    #[must_use]
    pub fn origem(mut self, modulo: impl Into<String>, id: Option<Id>) -> Self {
        self.origem_modulo = modulo.into();
        self.origem_id = id;
        self
    }

    /// Define a forma de cobrança (padrão: [`FormaCobranca::Carteira`]).
    #[must_use]
    pub const fn forma_cobranca(mut self, f: FormaCobranca) -> Self {
        self.forma_cobranca = f;
        self
    }

    /// Associa um centro de custo ao título.
    #[must_use]
    pub const fn centro_custo(mut self, cc: Id) -> Self {
        self.centro_custo = Some(cc);
        self
    }

    /// Associa uma categoria de relatório ao título (`docs/modulos/financeiro.md` §11.9) —
    /// não afeta a contabilização, só agregações por categoria/mês.
    #[must_use]
    pub const fn categoria(mut self, categoria: Id) -> Self {
        self.categoria = Some(categoria);
        self
    }

    /// Anexa uma observação livre.
    #[must_use]
    pub fn observacao(mut self, o: impl Into<String>) -> Self {
        self.observacao = Some(o.into());
        self
    }

    /// Define o parcelamento: quantidade, vencimento da 1ª parcela e intervalo em dias
    /// entre parcelas consecutivas.
    #[must_use]
    pub const fn parcelas(
        mut self,
        quantidade: u16,
        primeiro_vencimento: Data,
        intervalo_dias: i32,
    ) -> Self {
        self.n_parcelas = quantidade;
        self.primeiro_vencimento = Some(primeiro_vencimento);
        self.intervalo_dias = intervalo_dias;
        self
    }

    /// Define a política e a taxa de juros de mora aplicadas a todas as parcelas.
    #[must_use]
    pub const fn juros(mut self, politica: PoliticaJuros, taxa: Percentual) -> Self {
        self.politica_juros = politica;
        self.taxa_juros = Some(taxa);
        self
    }

    /// Define a multa por atraso (aplicada uma vez no primeiro dia de atraso).
    #[must_use]
    pub const fn multa(mut self, p: Percentual) -> Self {
        self.multa = Some(p);
        self
    }

    /// Define o desconto por antecipação e a data-limite para aproveitá-lo.
    #[must_use]
    pub const fn desconto(mut self, ate: Data, valor: Dinheiro) -> Self {
        self.desconto_ate_data = Some(ate);
        self.desconto_valor = Some(valor);
        self
    }

    /// Valida e produz o título com as parcelas rateadas.
    ///
    /// # Errors
    /// - [`ErroFinanceiro::ValorInvalido`] se o total não for positivo.
    /// - [`ErroFinanceiro::NumeroDeParcelasInvalido`] se estiver fora de 1..=360.
    /// - [`ErroFinanceiro::TaxaDeJurosAusente`] se a política de juros não for
    ///   [`PoliticaJuros::Nenhum`] e a taxa vier zerada ou ausente.
    pub fn construir(self) -> Result<TituloComParcelas, ErroFinanceiro> {
        if !self.valor_total.e_positivo() {
            return Err(ErroFinanceiro::ValorInvalido);
        }
        if self.n_parcelas < 1 || self.n_parcelas > 360 {
            return Err(ErroFinanceiro::NumeroDeParcelasInvalido(self.n_parcelas));
        }
        let taxa_juros = if self.politica_juros.exige_taxa() {
            match self.taxa_juros {
                Some(t) if !t.e_zero() => Some(t),
                _ => {
                    return Err(ErroFinanceiro::TaxaDeJurosAusente(
                        self.politica_juros.rotulo(),
                    ))
                }
            }
        } else {
            None
        };

        let titulo_id = Id::novo();
        let primeiro_vencimento = self.primeiro_vencimento.unwrap_or(self.emissao);
        let valores = self.valor_total.ratear(self.n_parcelas as usize);

        let parcelas = valores
            .into_iter()
            .enumerate()
            .map(|(i, valor)| {
                let k = i32::try_from(i).unwrap_or(i32::MAX);
                Parcela {
                    id: Id::novo(),
                    empresa: self.empresa,
                    titulo: titulo_id,
                    numero: u16::try_from(i + 1).unwrap_or(u16::MAX),
                    vencimento: primeiro_vencimento.mais_dias(k * self.intervalo_dias),
                    valor,
                    estado: EstadoParcela::Aberta,
                    valor_baixado: Dinheiro::ZERO,
                    lancamento: None,
                    nosso_numero: None,
                    politica_juros: self.politica_juros,
                    taxa_juros,
                    multa: self.multa,
                    desconto_ate_data: self.desconto_ate_data,
                    desconto_valor: self.desconto_valor,
                    versao: Versao::INICIAL,
                }
            })
            .collect();

        let titulo = Titulo {
            id: titulo_id,
            empresa: self.empresa,
            especie: self.especie,
            contraparte: self.contraparte,
            origem_modulo: self.origem_modulo,
            origem_id: self.origem_id,
            emissao: self.emissao,
            valor_original: self.valor_total,
            forma_cobranca: self.forma_cobranca,
            centro_custo: self.centro_custo,
            categoria: self.categoria,
            observacao: self.observacao,
            cancelado_em: None,
            versao: Versao::INICIAL,
        };

        Ok(TituloComParcelas { titulo, parcelas })
    }
}

#[cfg(test)]
mod testes {
    use cardeal_kernel::Fuso;
    use proptest::prelude::*;

    use super::*;

    fn hoje() -> Data {
        Data::hoje(Fuso::BRASILIA)
    }

    fn construtor(valor: Dinheiro) -> ConstrutorTitulo {
        ConstrutorTitulo::novo(
            Id::novo(),
            EspecieTitulo::Receber,
            Contraparte::Cliente(Id::novo()),
            valor,
            hoje(),
        )
    }

    #[test]
    fn parcela_unica_por_padrao_vence_na_emissao() {
        let tcp = construtor(Dinheiro::reais(250)).construir().unwrap();
        assert_eq!(tcp.parcelas.len(), 1);
        assert_eq!(tcp.parcelas[0].numero, 1);
        assert_eq!(tcp.parcelas[0].valor, Dinheiro::reais(250));
        assert_eq!(tcp.parcelas[0].vencimento, hoje());
        assert_eq!(tcp.titulo.valor_original, Dinheiro::reais(250));
    }

    #[test]
    fn vencimentos_seguem_o_intervalo() {
        let tcp = construtor(Dinheiro::reais(300))
            .parcelas(3, hoje().mais_dias(10), 30)
            .construir()
            .unwrap();
        assert_eq!(tcp.parcelas[0].vencimento, hoje().mais_dias(10));
        assert_eq!(tcp.parcelas[1].vencimento, hoje().mais_dias(40));
        assert_eq!(tcp.parcelas[2].vencimento, hoje().mais_dias(70));
    }

    #[test]
    fn valor_nao_positivo_e_recusado() {
        assert_eq!(
            construtor(Dinheiro::ZERO).construir().unwrap_err(),
            ErroFinanceiro::ValorInvalido
        );
        assert_eq!(
            construtor(Dinheiro::reais(-1)).construir().unwrap_err(),
            ErroFinanceiro::ValorInvalido
        );
    }

    #[test]
    fn parcelas_fora_da_faixa_sao_recusadas() {
        assert!(matches!(
            construtor(Dinheiro::reais(10))
                .parcelas(0, hoje(), 30)
                .construir()
                .unwrap_err(),
            ErroFinanceiro::NumeroDeParcelasInvalido(0)
        ));
        assert!(matches!(
            construtor(Dinheiro::reais(10))
                .parcelas(361, hoje(), 30)
                .construir()
                .unwrap_err(),
            ErroFinanceiro::NumeroDeParcelasInvalido(361)
        ));
    }

    #[test]
    fn categoria_e_opcional_e_fica_no_titulo() {
        let sem = construtor(Dinheiro::reais(10)).construir().unwrap();
        assert_eq!(sem.titulo.categoria, None);

        let cat = Id::novo();
        let com = construtor(Dinheiro::reais(10))
            .categoria(cat)
            .construir()
            .unwrap();
        assert_eq!(com.titulo.categoria, Some(cat));
    }

    #[test]
    fn juros_sem_taxa_e_recusado() {
        let erro = construtor(Dinheiro::reais(10))
            .juros(PoliticaJuros::SimplesDiario, Percentual::ZERO)
            .construir()
            .unwrap_err();
        assert!(matches!(erro, ErroFinanceiro::TaxaDeJurosAusente(_)));
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(400))]

        /// A soma das parcelas é sempre exatamente igual ao total do título, e a maior e a
        /// menor parcela nunca diferem por mais de um centavo (`docs/modulos/financeiro.md` §11.2).
        #[test]
        fn rateio_de_parcelas_conserva_o_total(
            centavos in 1i64..=50_000_000,
            n in 1u16..=120,
        ) {
            let total = Dinheiro::centavos(centavos);
            let tcp = construtor(total).parcelas(n, hoje(), 30).construir().unwrap();

            prop_assert_eq!(tcp.parcelas.len(), n as usize);
            let soma: Dinheiro = tcp.parcelas.iter().map(|p| p.valor).sum();
            prop_assert_eq!(soma, total);

            let maior = tcp.parcelas.iter().map(|p| p.valor.em_centavos()).max().unwrap();
            let menor = tcp.parcelas.iter().map(|p| p.valor.em_centavos()).min().unwrap();
            prop_assert!(maior - menor <= 1);
        }
    }
}
