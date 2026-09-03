//! Sessão de caixa — o único lugar do sistema que sabe abrir e fechar um caixa físico.
//!
//! Ver `docs/modulos/financeiro.md` §2 (submódulo `caixa`), §4 (máquina de estado) e §11.3
//! (**fechamento sempre cego**). Domínio puro: as transições produzem o
//! [`LancamentoBalanceado`](cardeal_ledger::LancamentoBalanceado) correspondente, mas não
//! gravam nada — persistir e numerar é do comando.

use cardeal_kernel::{Data, Dinheiro, Id, Instante, Versao};
use cardeal_ledger::{ConstrutorLancamento, LancamentoBalanceado, Origem};

use crate::erros::ErroFinanceiro;
use crate::receituario::Autoria;

/// Tolerância padrão de quebra de caixa: abaixo disso não exige justificativa
/// (`docs/modulos/financeiro.md` §10, tela de fechamento).
pub const TOLERANCIA_QUEBRA: Dinheiro = Dinheiro::centavos(500);

/// A configuração de um caixa físico (uma gaveta, um cofre, um terminal).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Caixa {
    /// Identidade.
    pub id: Id,
    /// A empresa.
    pub empresa: Id,
    /// Nome exibido: "Caixa 1", "Cofre".
    pub nome: String,
    /// Filial/PDV a que pertence, quando aplicável.
    pub local_operacao: Option<Id>,
    /// A conta analítica em 1.1.01 exclusiva deste caixa.
    pub conta_razao: Id,
    /// Se o caixa pode ficar com saldo negativo (padrão: não).
    pub permite_negativo: bool,
    /// Se o caixa está ativo.
    pub ativo: bool,
    /// Versão para bloqueio otimista.
    pub versao: Versao,
}

/// O estado de uma [`SessaoCaixa`] (`docs/modulos/financeiro.md` §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EstadoSessao {
    /// Recebe movimentos.
    Aberta,
    /// Congelada; o operador já informou o valor contado.
    Fechada,
    /// Um supervisor conferiu a quebra.
    Auditada,
}

impl EstadoSessao {
    /// O rótulo em português.
    #[must_use]
    pub const fn rotulo(self) -> &'static str {
        match self {
            Self::Aberta => "Aberta",
            Self::Fechada => "Fechada",
            Self::Auditada => "Auditada",
        }
    }
}

/// Uma sessão de trabalho de um caixa: da abertura ao fechamento cego.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessaoCaixa {
    /// Identidade.
    pub id: Id,
    /// A empresa.
    pub empresa: Id,
    /// O caixa físico.
    pub caixa: Id,
    /// O usuário que abriu a sessão.
    pub operador: Id,
    /// O terminal onde a sessão corre.
    pub dispositivo: Id,
    /// Quando abriu.
    pub abertura: Instante,
    /// Quando fechou (nulo enquanto aberta).
    pub fechamento: Option<Instante>,
    /// O suprimento inicial (pode ser zero).
    pub valor_abertura: Dinheiro,
    /// O saldo que o sistema esperava no fechamento — preenchido **depois** da contagem.
    pub valor_esperado: Option<Dinheiro>,
    /// O valor contado pelo operador no fechamento cego.
    pub valor_contado: Option<Dinheiro>,
    /// `valor_contado − valor_esperado`; negativo = falta.
    pub quebra: Option<Dinheiro>,
    /// O estado atual.
    pub estado: EstadoSessao,
    /// Versão para bloqueio otimista.
    pub versao: Versao,
}

/// O tipo de um [`MovimentoCaixa`] — o sinal do efeito no caixa vem daqui, não do valor
/// (que é sempre positivo).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TipoMovimento {
    /// Entrada de troco/fundo vinda do cofre ou banco.
    Suprimento,
    /// Retirada de dinheiro do caixa para o cofre ou banco.
    Sangria,
    /// Venda recebida em espécie na sessão.
    Venda,
    /// Recebimento de título em espécie na sessão.
    Recebimento,
    /// Pagamento feito em espécie pela sessão.
    Pagamento,
    /// Ajuste de quebra apurado no fechamento.
    QuebraCaixa,
}

impl TipoMovimento {
    /// Verdadeiro se o movimento exige um motivo informado.
    #[must_use]
    pub const fn exige_motivo(self) -> bool {
        matches!(self, Self::Sangria | Self::QuebraCaixa)
    }

    /// O efeito no saldo do caixa: `+1` entra, `-1` sai. Para [`Self::QuebraCaixa`] o efeito
    /// depende do sinal da quebra, então devolve `0` (o chamador decide).
    #[must_use]
    pub const fn efeito_no_caixa(self) -> i8 {
        match self {
            Self::Suprimento | Self::Venda | Self::Recebimento => 1,
            Self::Sangria | Self::Pagamento => -1,
            Self::QuebraCaixa => 0,
        }
    }
}

/// Um movimento de dinheiro dentro de uma sessão de caixa.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MovimentoCaixa {
    /// Identidade.
    pub id: Id,
    /// A empresa.
    pub empresa: Id,
    /// A sessão dona.
    pub sessao: Id,
    /// O tipo.
    pub tipo: TipoMovimento,
    /// O valor, sempre positivo.
    pub valor: Dinheiro,
    /// A forma de pagamento, quando aplicável.
    pub forma_pagamento: Option<Id>,
    /// O `Lancamento` no Razão que este movimento representa. `None` enquanto o comando não
    /// o gravou.
    pub lancamento: Option<Id>,
    /// O motivo — obrigatório para [`TipoMovimento::Sangria`] e [`TipoMovimento::QuebraCaixa`].
    pub motivo: Option<String>,
    /// Quando.
    pub criado_em: Instante,
    /// Quem.
    pub criado_por: Id,
}

/// As contas do Razão que os lançamentos de caixa usam. O comando as resolve por papel
/// ([`PapelConta`](cardeal_ledger::PapelConta)) antes de chamar as funções deste módulo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContasCaixa {
    /// A conta analítica do caixa (`Caixa::conta_razao`).
    pub caixa: Id,
    /// A contrapartida de suprimento/sangria: cofre ou conta bancária.
    pub contrapartida: Id,
    /// Quebra de caixa (despesa) — usada quando falta dinheiro.
    pub quebra: Id,
    /// Outras receitas — usada quando sobra dinheiro.
    pub sobra: Id,
}

/// O resultado de abrir uma sessão: a sessão nova e, se houve suprimento inicial, o
/// lançamento que o registra.
#[derive(Debug, Clone)]
pub struct AberturaCaixa {
    /// A sessão recém-criada, em [`EstadoSessao::Aberta`].
    pub sessao: SessaoCaixa,
    /// O lançamento `Realizado` D Caixa / C contrapartida, se `valor_abertura > 0`.
    pub lancamento_suprimento: Option<LancamentoBalanceado>,
}

/// O resultado de fechar uma sessão (fechamento cego).
#[derive(Debug, Clone)]
pub struct FechamentoCaixa {
    /// A sessão atualizada: [`EstadoSessao::Fechada`], com `valor_esperado`, `valor_contado`
    /// e `quebra` preenchidos.
    pub sessao: SessaoCaixa,
    /// A quebra apurada (`valor_contado − valor_esperado`); negativa = falta.
    pub quebra: Dinheiro,
    /// O movimento e o lançamento de ajuste, quando a quebra não é zero.
    pub ajuste: Option<(MovimentoCaixa, LancamentoBalanceado)>,
}

fn origem_caixa(sessao: Id) -> Origem {
    Origem::modulo("financeiro").tipo("caixa").agregado(sessao)
}

/// Monta um lançamento `Realizado` de duas partidas — D `debito` / C `credito`, mesmo valor.
/// Interno: como o valor de débito e crédito é sempre idêntico, `construir` nunca falha.
fn lancamento_dc(
    empresa: Id,
    data: Data,
    historico: String,
    origem: Origem,
    autoria: Autoria,
    debito: (Id, Dinheiro),
    credito: (Id, Dinheiro),
) -> LancamentoBalanceado {
    ConstrutorLancamento::novo(empresa, data, historico)
        .origem(origem)
        .liquidacao(data)
        .criado_por(autoria.usuario, autoria.dispositivo)
        .agora(autoria.agora)
        .debitar(debito.0, debito.1)
        .creditar(credito.0, credito.1)
        .construir()
        .expect("débito e crédito de mesmo valor sempre balanceiam")
}

impl SessaoCaixa {
    /// Abre uma sessão para `caixa`. Se `valor_abertura` for positivo, também devolve o
    /// lançamento do suprimento inicial (D Caixa / C contrapartida, `Realizado` —
    /// `docs/modulos/financeiro.md` §7). O comando valida antes que o caixa esteja ativo e
    /// que não haja outra sessão aberta.
    #[must_use]
    pub fn abrir(
        caixa: &Caixa,
        operador: Id,
        dispositivo: Id,
        valor_abertura: Dinheiro,
        contas: ContasCaixa,
        autoria: Autoria,
    ) -> AberturaCaixa {
        let id = Id::novo();
        let sessao = SessaoCaixa {
            id,
            empresa: caixa.empresa,
            caixa: caixa.id,
            operador,
            dispositivo,
            abertura: autoria.agora,
            fechamento: None,
            valor_abertura: valor_abertura.nao_negativo(),
            valor_esperado: None,
            valor_contado: None,
            quebra: None,
            estado: EstadoSessao::Aberta,
            versao: Versao::INICIAL,
        };

        let lancamento_suprimento = valor_abertura.e_positivo().then(|| {
            lancamento_dc(
                caixa.empresa,
                autoria.hoje(),
                format!("Abertura de caixa \"{}\" — suprimento inicial", caixa.nome),
                origem_caixa(id),
                autoria,
                (contas.caixa, valor_abertura),
                (contas.contrapartida, valor_abertura),
            )
        });

        AberturaCaixa {
            sessao,
            lancamento_suprimento,
        }
    }

    /// Registra um suprimento (entrada de fundo de troco): D Caixa / C contrapartida.
    ///
    /// # Errors
    /// [`ErroFinanceiro::CaixaFechado`] se a sessão não está aberta; [`ErroFinanceiro::BaixaZerada`]
    /// se o valor não é positivo.
    pub fn registrar_suprimento(
        &self,
        valor: Dinheiro,
        contas: ContasCaixa,
        autoria: Autoria,
    ) -> Result<(MovimentoCaixa, LancamentoBalanceado), ErroFinanceiro> {
        self.exigir_aberta()?;
        if !valor.e_positivo() {
            return Err(ErroFinanceiro::BaixaZerada);
        }
        let lancamento = lancamento_dc(
            self.empresa,
            autoria.hoje(),
            "Suprimento de caixa".to_string(),
            origem_caixa(self.id),
            autoria,
            (contas.caixa, valor),
            (contas.contrapartida, valor),
        );
        Ok((
            self.movimento(TipoMovimento::Suprimento, valor, None, autoria),
            lancamento,
        ))
    }

    /// Registra uma sangria (retirada para o cofre/banco): D contrapartida / C Caixa.
    ///
    /// `saldo_atual` é o saldo em espécie do caixa antes da sangria — o comando o obtém do
    /// Razão. A sangria não pode passar dele, a menos que o caixa permita saldo negativo.
    ///
    /// # Errors
    /// - [`ErroFinanceiro::CaixaFechado`] se a sessão não está aberta.
    /// - [`ErroFinanceiro::BaixaZerada`] se o valor não é positivo.
    /// - [`ErroFinanceiro::QuebraExigeMotivo`] se o motivo vier vazio (a sangria sempre exige um).
    /// - [`ErroFinanceiro::ValorSuperaSaldoDoCaixa`] se a sangria passa do saldo.
    pub fn registrar_sangria(
        &self,
        valor: Dinheiro,
        saldo_atual: Dinheiro,
        permite_negativo: bool,
        motivo: impl Into<String>,
        contas: ContasCaixa,
        autoria: Autoria,
    ) -> Result<(MovimentoCaixa, LancamentoBalanceado), ErroFinanceiro> {
        self.exigir_aberta()?;
        if !valor.e_positivo() {
            return Err(ErroFinanceiro::BaixaZerada);
        }
        let motivo = motivo.into();
        if motivo.trim().is_empty() {
            return Err(ErroFinanceiro::QuebraExigeMotivo { quebra: valor });
        }
        if !permite_negativo && valor > saldo_atual {
            return Err(ErroFinanceiro::ValorSuperaSaldoDoCaixa {
                valor,
                saldo: saldo_atual,
            });
        }
        let lancamento = lancamento_dc(
            self.empresa,
            autoria.hoje(),
            format!("Sangria de caixa — {}", motivo.trim()),
            origem_caixa(self.id),
            autoria,
            (contas.contrapartida, valor),
            (contas.caixa, valor),
        );
        Ok((
            self.movimento(TipoMovimento::Sangria, valor, Some(motivo), autoria),
            lancamento,
        ))
    }

    /// Fecha a sessão pelo **fechamento cego** (`docs/modulos/financeiro.md` §11.3): o
    /// chamador já coletou `valor_contado` do operador **sem** ter mostrado `saldo_esperado`.
    /// Esta função só apura a quebra e monta o lançamento de ajuste (falta: D Quebra de
    /// caixa / C Caixa; sobra: D Caixa / C Outras receitas).
    ///
    /// # Errors
    /// - [`ErroFinanceiro::EstadoDeSessaoInvalido`] se a sessão não está aberta.
    /// - [`ErroFinanceiro::QuebraExigeMotivo`] se `|quebra| > tolerancia` e falta motivo.
    pub fn fechar(
        &self,
        valor_contado: Dinheiro,
        saldo_esperado: Dinheiro,
        tolerancia: Dinheiro,
        motivo: Option<String>,
        contas: ContasCaixa,
        autoria: Autoria,
    ) -> Result<FechamentoCaixa, ErroFinanceiro> {
        if self.estado != EstadoSessao::Aberta {
            return Err(ErroFinanceiro::EstadoDeSessaoInvalido {
                atual: self.estado.rotulo(),
                esperado: "Aberta",
            });
        }
        let quebra = valor_contado - saldo_esperado;
        let motivo = motivo.filter(|m| !m.trim().is_empty());
        if quebra.abs() > tolerancia && motivo.is_none() {
            return Err(ErroFinanceiro::QuebraExigeMotivo { quebra });
        }

        let mut sessao = self.clone();
        sessao.estado = EstadoSessao::Fechada;
        sessao.fechamento = Some(autoria.agora);
        sessao.valor_esperado = Some(saldo_esperado);
        sessao.valor_contado = Some(valor_contado);
        sessao.quebra = Some(quebra);
        sessao.versao = sessao.versao.proxima();

        let ajuste = (!quebra.e_zero()).then(|| {
            let falta = quebra.e_negativo();
            let (debito, credito, historico) = if falta {
                (
                    (contas.quebra, quebra.abs()),
                    (contas.caixa, quebra.abs()),
                    "Fechamento de caixa — quebra (falta)".to_string(),
                )
            } else {
                (
                    (contas.caixa, quebra),
                    (contas.sobra, quebra),
                    "Fechamento de caixa — sobra".to_string(),
                )
            };
            let lancamento = lancamento_dc(
                self.empresa,
                autoria.hoje(),
                historico,
                origem_caixa(self.id),
                autoria,
                debito,
                credito,
            );
            let movimento = MovimentoCaixa {
                motivo: motivo.clone(),
                ..self.movimento(TipoMovimento::QuebraCaixa, quebra.abs(), None, autoria)
            };
            (movimento, lancamento)
        });

        Ok(FechamentoCaixa {
            sessao,
            quebra,
            ajuste,
        })
    }

    /// Um supervisor confere a quebra e marca a sessão como [`EstadoSessao::Auditada`].
    ///
    /// # Errors
    /// [`ErroFinanceiro::EstadoDeSessaoInvalido`] se a sessão não está fechada.
    pub fn auditar(&mut self) -> Result<(), ErroFinanceiro> {
        if self.estado != EstadoSessao::Fechada {
            return Err(ErroFinanceiro::EstadoDeSessaoInvalido {
                atual: self.estado.rotulo(),
                esperado: "Fechada",
            });
        }
        self.estado = EstadoSessao::Auditada;
        self.versao = self.versao.proxima();
        Ok(())
    }

    fn exigir_aberta(&self) -> Result<(), ErroFinanceiro> {
        if self.estado == EstadoSessao::Aberta {
            Ok(())
        } else {
            Err(ErroFinanceiro::CaixaFechado)
        }
    }

    fn movimento(
        &self,
        tipo: TipoMovimento,
        valor: Dinheiro,
        motivo: Option<String>,
        autoria: Autoria,
    ) -> MovimentoCaixa {
        MovimentoCaixa {
            id: Id::novo(),
            empresa: self.empresa,
            sessao: self.id,
            tipo,
            valor,
            forma_pagamento: None,
            lancamento: None,
            motivo,
            criado_em: autoria.agora,
            criado_por: autoria.usuario,
        }
    }
}

#[cfg(test)]
mod testes {
    use cardeal_kernel::Fuso;

    use super::*;

    fn autoria() -> Autoria {
        Autoria {
            usuario: Id::novo(),
            dispositivo: Id::novo(),
            agora: Instante::agora(),
            fuso: Fuso::BRASILIA,
        }
    }

    fn contas() -> ContasCaixa {
        ContasCaixa {
            caixa: Id::novo(),
            contrapartida: Id::novo(),
            quebra: Id::novo(),
            sobra: Id::novo(),
        }
    }

    fn caixa() -> Caixa {
        Caixa {
            id: Id::novo(),
            empresa: Id::novo(),
            nome: "Caixa 1".to_string(),
            local_operacao: None,
            conta_razao: Id::novo(),
            permite_negativo: false,
            ativo: true,
            versao: Versao::INICIAL,
        }
    }

    fn sessao_aberta(suprimento: Dinheiro) -> SessaoCaixa {
        SessaoCaixa::abrir(
            &caixa(),
            Id::novo(),
            Id::novo(),
            suprimento,
            contas(),
            autoria(),
        )
        .sessao
    }

    #[test]
    fn abertura_sem_suprimento_nao_gera_lancamento() {
        let a = SessaoCaixa::abrir(
            &caixa(),
            Id::novo(),
            Id::novo(),
            Dinheiro::ZERO,
            contas(),
            autoria(),
        );
        assert_eq!(a.sessao.estado, EstadoSessao::Aberta);
        assert!(a.lancamento_suprimento.is_none());
    }

    #[test]
    fn abertura_com_suprimento_debita_o_caixa() {
        let a = SessaoCaixa::abrir(
            &caixa(),
            Id::novo(),
            Id::novo(),
            Dinheiro::reais(200),
            contas(),
            autoria(),
        );
        let l = a.lancamento_suprimento.expect("suprimento gera lançamento");
        assert!(l.interno().esta_balanceado());
        assert_eq!(l.interno().total_debitos(), Dinheiro::reais(200));
    }

    #[test]
    fn sangria_exige_motivo_e_respeita_o_saldo() {
        let s = sessao_aberta(Dinheiro::reais(100));

        let erro = s
            .registrar_sangria(
                Dinheiro::reais(50),
                Dinheiro::reais(100),
                false,
                "  ",
                contas(),
                autoria(),
            )
            .unwrap_err();
        assert!(matches!(erro, ErroFinanceiro::QuebraExigeMotivo { .. }));

        let erro = s
            .registrar_sangria(
                Dinheiro::reais(150),
                Dinheiro::reais(100),
                false,
                "troco para o cofre",
                contas(),
                autoria(),
            )
            .unwrap_err();
        assert!(matches!(
            erro,
            ErroFinanceiro::ValorSuperaSaldoDoCaixa { .. }
        ));

        let (mov, l) = s
            .registrar_sangria(
                Dinheiro::reais(40),
                Dinheiro::reais(100),
                false,
                "troco para o cofre",
                contas(),
                autoria(),
            )
            .unwrap();
        assert_eq!(mov.tipo, TipoMovimento::Sangria);
        assert_eq!(mov.motivo.as_deref(), Some("troco para o cofre"));
        assert!(l.interno().esta_balanceado());
    }

    #[test]
    fn suprimento_balanceia_e_recusa_sessao_fechada() {
        let mut s = sessao_aberta(Dinheiro::ZERO);
        let (_mov, l) = s
            .registrar_suprimento(Dinheiro::reais(30), contas(), autoria())
            .unwrap();
        assert!(l.interno().esta_balanceado());

        s.estado = EstadoSessao::Fechada;
        let erro = s
            .registrar_suprimento(Dinheiro::reais(10), contas(), autoria())
            .unwrap_err();
        assert!(matches!(erro, ErroFinanceiro::CaixaFechado));
    }

    #[test]
    fn fechamento_cego_apura_quebra_e_exige_motivo_acima_da_tolerancia() {
        let s = sessao_aberta(Dinheiro::reais(100));

        let erro = s
            .fechar(
                Dinheiro::reais(88),
                Dinheiro::reais(100),
                TOLERANCIA_QUEBRA,
                None,
                contas(),
                autoria(),
            )
            .unwrap_err();
        assert!(matches!(erro, ErroFinanceiro::QuebraExigeMotivo { .. }));

        let f = s
            .fechar(
                Dinheiro::reais(88),
                Dinheiro::reais(100),
                TOLERANCIA_QUEBRA,
                Some("conferência do supervisor pendente".to_string()),
                contas(),
                autoria(),
            )
            .unwrap();
        assert_eq!(f.quebra, Dinheiro::reais(-12));
        assert_eq!(f.sessao.estado, EstadoSessao::Fechada);
        assert_eq!(f.sessao.valor_esperado, Some(Dinheiro::reais(100)));
        let (mov, l) = f.ajuste.expect("quebra != 0 gera ajuste");
        assert_eq!(mov.tipo, TipoMovimento::QuebraCaixa);
        assert!(l.interno().esta_balanceado());
        assert_eq!(l.interno().total_debitos(), Dinheiro::reais(12));
    }

    #[test]
    fn quebra_dentro_da_tolerancia_dispensa_motivo() {
        let s = sessao_aberta(Dinheiro::reais(100));
        let f = s
            .fechar(
                Dinheiro::centavos(9_700),
                Dinheiro::reais(100),
                TOLERANCIA_QUEBRA,
                None,
                contas(),
                autoria(),
            )
            .unwrap();
        assert_eq!(f.quebra, Dinheiro::reais(-3));
        assert!(f.ajuste.is_some());
    }

    #[test]
    fn fechamento_exato_nao_gera_ajuste() {
        let s = sessao_aberta(Dinheiro::reais(100));
        let f = s
            .fechar(
                Dinheiro::reais(100),
                Dinheiro::reais(100),
                TOLERANCIA_QUEBRA,
                None,
                contas(),
                autoria(),
            )
            .unwrap();
        assert_eq!(f.quebra, Dinheiro::ZERO);
        assert!(f.ajuste.is_none());
    }

    #[test]
    fn sobra_credita_outras_receitas() {
        let s = sessao_aberta(Dinheiro::reais(100));
        let f = s
            .fechar(
                Dinheiro::reais(103),
                Dinheiro::reais(100),
                TOLERANCIA_QUEBRA,
                None,
                contas(),
                autoria(),
            )
            .unwrap();
        assert_eq!(f.quebra, Dinheiro::reais(3));
        let (_mov, l) = f.ajuste.unwrap();
        assert!(l.interno().esta_balanceado());
        assert_eq!(l.interno().total_debitos(), Dinheiro::reais(3));
    }

    #[test]
    fn auditar_so_a_partir_de_fechada() {
        let mut s = sessao_aberta(Dinheiro::ZERO);
        assert!(s.auditar().is_err());
        s.estado = EstadoSessao::Fechada;
        assert!(s.auditar().is_ok());
        assert_eq!(s.estado, EstadoSessao::Auditada);
    }
}
