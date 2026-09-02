//! O construtor validado e o tipo que prova o balanceamento.
//!
//! Este é o arquivo mais importante do projeto inteiro. Ver `docs/05-nucleo-financeiro.md`
//! §3.3: um lançamento desbalanceado não é validado — é **impossível de construir**.

use cardeal_kernel::{Data, Dinheiro, Id, Instante, Quantidade};
use smallvec::SmallVec;

use crate::erros::ErroRazao;
use crate::lancamento::{Contraparte, EstadoLancamento, Lancamento, Origem, Partida};

/// Um [`Lancamento`] cujas partidas já foram provadas balanceadas.
///
/// **O campo é privado.** Não existe caminho para instanciar este tipo fora de
/// [`ConstrutorLancamento::construir`] — nem `Default`, nem `From`, nem construtor público.
/// Uma função que recebe `LancamentoBalanceado` sabe, só pelo tipo do parâmetro, que débito
/// é igual a crédito, sem precisar checar de novo. Isso é o que
/// `docs/contratos-internos.md` §3 chama de "a assinatura de `Razao::registrar`": ela só
/// aceita este tipo.
#[derive(Debug, Clone)]
pub struct LancamentoBalanceado(Lancamento);

impl LancamentoBalanceado {
    /// O lançamento por dentro, para quem vai persistir ou inspecionar (testes, exibição).
    #[must_use]
    pub const fn interno(&self) -> &Lancamento {
        &self.0
    }

    /// Consome o invólucro e devolve o lançamento — usado pelo repositório na hora de
    /// gravar, quando não há mais necessidade da garantia estática.
    #[must_use]
    pub fn dentro(self) -> Lancamento {
        self.0
    }
}

/// Uma partida ainda sem lançamento — estado intermediário do construtor.
struct PartidaEmConstrucao {
    conta: Id,
    valor: Dinheiro,
    contraparte: Option<Contraparte>,
    centro_custo: Option<Id>,
    projeto: Option<Id>,
    documento: Option<String>,
    quantidade: Option<Quantidade>,
    complemento: Option<String>,
}

/// Constrói um [`LancamentoBalanceado`] passo a passo, no vocabulário do domínio contábil
/// (`debitar`/`creditar`), e só entrega o resultado se as partidas somarem zero.
///
/// ```
/// use cardeal_kernel::{Data, Dinheiro, Fuso, Id};
/// use cardeal_ledger::ConstrutorLancamento;
///
/// let caixa = Id::novo();
/// let receita = Id::novo();
/// let hoje = Data::hoje(Fuso::BRASILIA);
///
/// let lancamento = ConstrutorLancamento::novo(Id::novo(), hoje, "Venda 1 — PDV 1")
///     .criado_por(Id::novo(), Id::novo())
///     .debitar(caixa, Dinheiro::reais(50))
///     .creditar(receita, Dinheiro::reais(50))
///     .construir()
///     .unwrap();
///
/// assert!(lancamento.interno().esta_balanceado());
/// ```
///
/// Um lançamento desbalanceado nunca sai do construtor:
///
/// ```
/// use cardeal_kernel::{Data, Dinheiro, Fuso, Id};
/// use cardeal_ledger::ConstrutorLancamento;
///
/// let erro = ConstrutorLancamento::novo(Id::novo(), Data::hoje(Fuso::BRASILIA), "Errado")
///     .criado_por(Id::novo(), Id::novo())
///     .debitar(Id::novo(), Dinheiro::reais(50))
///     .creditar(Id::novo(), Dinheiro::reais(40)) // faltam 10 reais
///     .construir();
///
/// assert!(erro.is_err());
/// ```
pub struct ConstrutorLancamento {
    empresa: Id,
    competencia: Data,
    historico: String,
    origem: Option<Origem>,
    estado: Option<EstadoLancamento>,
    vencimento: Option<Data>,
    liquidacao: Option<Data>,
    criado_por: Option<Id>,
    dispositivo: Option<Id>,
    agora: Option<Instante>,
    partidas: SmallVec<[PartidaEmConstrucao; 4]>,
}

impl ConstrutorLancamento {
    /// Começa um lançamento novo. `historico` é o texto legível que aparece no extrato
    /// da conta — escreva para o dono da empresa ler, não para o desenvolvedor.
    #[must_use]
    pub fn novo(empresa: Id, competencia: Data, historico: impl Into<String>) -> Self {
        Self {
            empresa,
            competencia,
            historico: historico.into(),
            origem: None,
            estado: None,
            vencimento: None,
            liquidacao: None,
            criado_por: None,
            dispositivo: None,
            agora: None,
            partidas: SmallVec::new(),
        }
    }

    /// De qual módulo e operação vem este lançamento.
    #[must_use]
    pub fn origem(mut self, o: Origem) -> Self {
        self.origem = Some(o);
        self
    }

    /// Fixa o estado explicitamente. Se omitido, `construir` deriva:
    /// [`EstadoLancamento::Realizado`] quando há `liquidacao`, senão
    /// [`EstadoLancamento::Confirmado`].
    #[must_use]
    pub const fn estado(mut self, e: EstadoLancamento) -> Self {
        self.estado = Some(e);
        self
    }

    /// A data de vencimento, para lançamentos ainda não liquidados.
    #[must_use]
    pub const fn vencimento(mut self, d: Data) -> Self {
        self.vencimento = Some(d);
        self
    }

    /// A data em que o dinheiro efetivamente andou.
    #[must_use]
    pub const fn liquidacao(mut self, d: Data) -> Self {
        self.liquidacao = Some(d);
        self
    }

    /// Quem está criando o lançamento e de qual dispositivo. Obrigatório — `construir`
    /// recusa sem isso, porque todo lançamento do Razão tem autoria (auditoria).
    #[must_use]
    pub const fn criado_por(mut self, usuario: Id, dispositivo: Id) -> Self {
        self.criado_por = Some(usuario);
        self.dispositivo = Some(dispositivo);
        self
    }

    /// O instante de criação. Se omitido, `construir` usa [`Instante::agora`] — em código
    /// de domínio testável, prefira sempre fixar explicitamente a partir de um relógio
    /// controlado (`docs/03-pilar-resiliencia.md`, testkit).
    #[must_use]
    pub const fn agora(mut self, i: Instante) -> Self {
        self.agora = Some(i);
        self
    }

    /// Acrescenta uma partida a débito (valor positivo).
    #[must_use]
    pub fn debitar(mut self, conta: Id, valor: Dinheiro) -> Self {
        self.partidas.push(PartidaEmConstrucao {
            conta,
            valor,
            contraparte: None,
            centro_custo: None,
            projeto: None,
            documento: None,
            quantidade: None,
            complemento: None,
        });
        self
    }

    /// Acrescenta uma partida a crédito. Recebe o valor **positivo** — o sinal é
    /// invertido internamente, para que o chamador nunca precise lembrar de negar.
    #[must_use]
    pub fn creditar(mut self, conta: Id, valor: Dinheiro) -> Self {
        self.partidas.push(PartidaEmConstrucao {
            conta,
            valor: -valor,
            contraparte: None,
            centro_custo: None,
            projeto: None,
            documento: None,
            quantidade: None,
            complemento: None,
        });
        self
    }

    /// Como [`Self::debitar`], mas não faz nada se `valor` for zero. Poupa o chamador de
    /// escrever `if valor.e_positivo() { ... }` para partidas condicionais (juros, desconto).
    #[must_use]
    pub fn debitar_se(self, conta: Id, valor: Dinheiro) -> Self {
        if valor.e_zero() {
            self
        } else {
            self.debitar(conta, valor)
        }
    }

    /// Como [`Self::creditar`], ignorando valor zero.
    #[must_use]
    pub fn creditar_se(self, conta: Id, valor: Dinheiro) -> Self {
        if valor.e_zero() {
            self
        } else {
            self.creditar(conta, valor)
        }
    }

    /// Associa uma contraparte à **última** partida adicionada.
    ///
    /// # Panics
    /// Se chamado antes de qualquer `debitar`/`creditar`.
    #[must_use]
    pub fn contraparte(mut self, c: Contraparte) -> Self {
        self.ultima_partida().contraparte = Some(c);
        self
    }

    /// Associa um centro de custo à última partida adicionada.
    ///
    /// # Panics
    /// Se chamado antes de qualquer `debitar`/`creditar`.
    #[must_use]
    pub fn centro_custo(mut self, cc: Id) -> Self {
        self.ultima_partida().centro_custo = Some(cc);
        self
    }

    /// Associa um projeto à última partida adicionada.
    ///
    /// # Panics
    /// Se chamado antes de qualquer `debitar`/`creditar`.
    #[must_use]
    pub fn projeto(mut self, p: Id) -> Self {
        self.ultima_partida().projeto = Some(p);
        self
    }

    /// Associa uma referência de documento à última partida adicionada.
    ///
    /// # Panics
    /// Se chamado antes de qualquer `debitar`/`creditar`.
    #[must_use]
    pub fn documento(mut self, d: impl Into<String>) -> Self {
        self.ultima_partida().documento = Some(d.into());
        self
    }

    /// Associa uma quantidade à última partida adicionada (contas de estoque).
    ///
    /// # Panics
    /// Se chamado antes de qualquer `debitar`/`creditar`.
    #[must_use]
    pub fn quantidade(mut self, q: Quantidade) -> Self {
        self.ultima_partida().quantidade = Some(q);
        self
    }

    /// Associa um complemento de histórico à última partida adicionada.
    ///
    /// # Panics
    /// Se chamado antes de qualquer `debitar`/`creditar`.
    #[must_use]
    pub fn complemento(mut self, c: impl Into<String>) -> Self {
        self.ultima_partida().complemento = Some(c.into());
        self
    }

    fn ultima_partida(&mut self) -> &mut PartidaEmConstrucao {
        self.partidas
            .last_mut()
            .expect("chame debitar/creditar antes de anotar uma partida")
    }

    /// Valida e produz o lançamento balanceado.
    ///
    /// Verificações feitas aqui, sem acesso a banco (validação de conta existir/estar
    /// ativa/aceitar lançamento e de período não estar fechado é responsabilidade de
    /// `Razao::registrar`, que tem acesso à conexão — ver `docs/contratos-internos.md` §3):
    ///
    /// 1. Ao menos duas partidas.
    /// 2. Nenhuma partida com valor zero (partida zero não é partida — é ruído no extrato).
    /// 3. A soma das partidas é exatamente zero.
    /// 4. Autoria (`criado_por`) foi informada.
    ///
    /// # Errors
    /// Ver [`ErroRazao`] para cada caso.
    pub fn construir(self) -> Result<LancamentoBalanceado, ErroRazao> {
        if self.partidas.len() < 2 {
            return Err(ErroRazao::PartidasInsuficientes);
        }
        if self.partidas.iter().any(|p| p.valor.e_zero()) {
            return Err(ErroRazao::ValorZerado);
        }

        let soma: Dinheiro = self.partidas.iter().map(|p| p.valor).sum();
        if !soma.e_zero() {
            return Err(ErroRazao::Desbalanceado {
                diferenca: soma,
                partidas: self.partidas.len(),
            });
        }

        let criado_por = self.criado_por.ok_or(ErroRazao::AutoriaAusente)?;
        let dispositivo = self.dispositivo.ok_or(ErroRazao::AutoriaAusente)?;

        let estado = self.estado.unwrap_or(if self.liquidacao.is_some() {
            EstadoLancamento::Realizado
        } else {
            EstadoLancamento::Confirmado
        });

        let partidas = self
            .partidas
            .into_iter()
            .map(|p| Partida {
                conta: p.conta,
                valor: p.valor,
                contraparte: p.contraparte,
                centro_custo: p.centro_custo,
                projeto: p.projeto,
                documento: p.documento,
                quantidade: p.quantidade,
                complemento: p.complemento,
            })
            .collect();

        Ok(LancamentoBalanceado(Lancamento {
            id: Id::novo(),
            empresa: self.empresa,
            // Atribuído de verdade por `Razao::registrar` (sequencial por empresa, no
            // commit). Zero aqui é só o valor antes de existir persistência.
            numero: 0,
            competencia: self.competencia,
            vencimento: self.vencimento,
            liquidacao: self.liquidacao,
            estado,
            origem: self
                .origem
                .unwrap_or_else(|| Origem::modulo("desconhecido")),
            historico: self.historico,
            estorna: None,
            estornado_por: None,
            criado_em: self.agora.unwrap_or_else(Instante::agora),
            criado_por,
            dispositivo,
            partidas,
        }))
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

    fn base() -> ConstrutorLancamento {
        ConstrutorLancamento::novo(Id::novo(), hoje(), "teste").criado_por(Id::novo(), Id::novo())
    }

    #[test]
    fn venda_tipica_gera_as_quatro_partidas_do_receituario() {
        let caixa = Id::novo();
        let receita = Id::novo();
        let cmv = Id::novo();
        let estoque = Id::novo();

        let l = base()
            .debitar(caixa, Dinheiro::reais(50))
            .creditar(receita, Dinheiro::reais(50))
            .debitar(cmv, Dinheiro::centavos(3400))
            .creditar(estoque, Dinheiro::centavos(3400))
            .construir()
            .expect("lançamento balanceado deve construir");

        let l = l.interno();
        assert_eq!(l.partidas.len(), 4);
        assert!(l.esta_balanceado());
        assert_eq!(
            l.total_debitos(),
            Dinheiro::reais(50) + Dinheiro::centavos(3400)
        );
        assert_eq!(l.total_debitos(), l.total_creditos());
    }

    #[test]
    fn desbalanceado_nunca_constroi() {
        let erro = base()
            .debitar(Id::novo(), Dinheiro::reais(50))
            .creditar(Id::novo(), Dinheiro::reais(40))
            .construir()
            .unwrap_err();
        match erro {
            ErroRazao::Desbalanceado {
                diferenca,
                partidas,
            } => {
                assert_eq!(diferenca, Dinheiro::reais(10));
                assert_eq!(partidas, 2);
            }
            outro => panic!("esperava Desbalanceado, veio {outro:?}"),
        }
    }

    #[test]
    fn uma_unica_partida_nao_constroi() {
        let erro = base()
            .debitar(Id::novo(), Dinheiro::reais(50))
            .construir()
            .unwrap_err();
        assert!(matches!(erro, ErroRazao::PartidasInsuficientes));
    }

    #[test]
    fn partida_zerada_nao_constroi() {
        let erro = base()
            .debitar(Id::novo(), Dinheiro::reais(50))
            .creditar(Id::novo(), Dinheiro::reais(50))
            .debitar(Id::novo(), Dinheiro::ZERO)
            .construir()
            .unwrap_err();
        assert!(matches!(erro, ErroRazao::ValorZerado));
    }

    #[test]
    fn sem_autoria_nao_constroi() {
        let erro = ConstrutorLancamento::novo(Id::novo(), hoje(), "sem autor")
            .debitar(Id::novo(), Dinheiro::reais(10))
            .creditar(Id::novo(), Dinheiro::reais(10))
            .construir()
            .unwrap_err();
        assert!(matches!(erro, ErroRazao::AutoriaAusente));
    }

    #[test]
    fn debitar_se_ignora_valor_zero() {
        let l = base()
            .debitar(Id::novo(), Dinheiro::reais(50))
            .creditar(Id::novo(), Dinheiro::reais(50))
            .debitar_se(Id::novo(), Dinheiro::ZERO)
            .creditar_se(Id::novo(), Dinheiro::ZERO)
            .construir()
            .unwrap();
        assert_eq!(l.interno().partidas.len(), 2);
    }

    #[test]
    fn estado_derivado_da_liquidacao() {
        let sem_liquidacao = base()
            .debitar(Id::novo(), Dinheiro::reais(10))
            .creditar(Id::novo(), Dinheiro::reais(10));
        assert_eq!(
            sem_liquidacao.construir().unwrap().interno().estado,
            EstadoLancamento::Confirmado
        );

        let com_liquidacao = base()
            .liquidacao(hoje())
            .debitar(Id::novo(), Dinheiro::reais(10))
            .creditar(Id::novo(), Dinheiro::reais(10));
        assert_eq!(
            com_liquidacao.construir().unwrap().interno().estado,
            EstadoLancamento::Realizado
        );
    }

    // ── propriedade: nenhuma combinação de débitos/créditos gerados aleatoriamente
    // sobrevive ao construtor a menos que a soma seja exatamente zero. ──────────────
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]

        #[test]
        fn qualquer_conjunto_de_partidas_so_constroi_se_soma_zero(
            valores in prop::collection::vec(-1_000_000i64..=1_000_000i64, 2..12)
                .prop_filter("nenhum valor pode ser zero", |v| v.iter().all(|x| *x != 0)),
        ) {
            let soma: i64 = valores.iter().sum();
            let mut c = base();
            for v in &valores {
                c = c.debitar(Id::novo(), Dinheiro::centavos(*v));
            }
            let resultado = c.construir();

            if soma == 0 {
                let lanc = resultado.expect("soma zero deveria construir");
                prop_assert!(lanc.interno().esta_balanceado());
            } else {
                prop_assert!(resultado.is_err());
            }
        }
    }
}
