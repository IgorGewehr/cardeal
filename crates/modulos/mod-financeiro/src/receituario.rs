//! O receituário contábil do financeiro: cada evento de negócio vira partida dobrada.
//!
//! `docs/modulos/financeiro.md` §7 e `docs/05-nucleo-financeiro.md` §5. Estas funções são o
//! elo entre o domínio do módulo e o Razão — elas só **montam** o
//! [`LancamentoBalanceado`](cardeal_ledger::LancamentoBalanceado); quem o grava (com número,
//! na transação) é o comando, via [`Razao::registrar`](cardeal_ledger::Razao::registrar).
//!
//! Nenhuma conta é referenciada por código: o comando resolve os papéis
//! ([`PapelConta`](cardeal_ledger::PapelConta)) e passa os [`Id`]s já prontos nas structs
//! `Contas*` abaixo (`docs/contratos-internos.md` §6, regra 4).

use cardeal_kernel::{Data, Fuso, Id, Instante};
use cardeal_ledger::{ConstrutorLancamento, Contraparte, LancamentoBalanceado, Origem};

use crate::baixa::PlanoBaixa;
use crate::erros::ErroFinanceiro;
use crate::titulo::{EspecieTitulo, Parcela, TituloComParcelas};

/// Quem está postando, de onde e quando — repassado ao [`ConstrutorLancamento`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Autoria {
    /// O usuário responsável.
    pub usuario: Id,
    /// O dispositivo de origem.
    pub dispositivo: Id,
    /// O instante da unidade de trabalho.
    pub agora: Instante,
    /// O fuso da empresa, para derivar a data corrente de [`Self::agora`].
    pub fuso: Fuso,
}

impl Autoria {
    /// A data corrente, no fuso da empresa.
    #[must_use]
    pub const fn hoje(&self) -> Data {
        self.agora.data(self.fuso)
    }
}

/// As contas que o lançamento de abertura de um título usa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContasTitulo {
    /// Clientes a receber (espécie `Receber`) ou Fornecedores (espécie `Pagar`).
    pub contraparte: Id,
    /// A conta de resultado: receita a creditar (`Receber`) ou despesa a debitar (`Pagar`).
    pub resultado: Id,
}

/// As contas que o lançamento de baixa de uma parcela usa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContasBaixa {
    /// O caixa ou a conta bancária que recebeu (ou pagou) o dinheiro.
    pub conta_destino: Id,
    /// Clientes a receber (`Receber`) ou Fornecedores (`Pagar`).
    pub contraparte: Id,
    /// Onde vão os encargos: Receita financeira (`Receber`) ou Despesa financeira (`Pagar`).
    pub encargos: Id,
    /// Onde vai o desconto por antecipação: Descontos concedidos (`Receber`) ou uma conta
    /// de receita, p. ex. Outras receitas (`Pagar`).
    pub desconto: Id,
}

fn origem_titulo(titulo: Id) -> Origem {
    Origem::modulo("financeiro").tipo("titulo").agregado(titulo)
}

/// Monta um lançamento `Confirmado` por parcela do título (`docs/modulos/financeiro.md` §7,
/// "Lançamento de título ... avulso"). O dinheiro ainda não andou — só a obrigação existe.
///
/// A ordem do `Vec` de saída acompanha `tcp.parcelas`.
///
/// # Errors
/// [`ErroFinanceiro`] não é retornado aqui; o `Result` propaga apenas um eventual
/// desbalanceamento do [`ConstrutorLancamento`], impossível na prática (D e C usam o mesmo
/// valor).
pub fn lancar_titulo(
    tcp: &TituloComParcelas,
    contas: ContasTitulo,
    autoria: Autoria,
) -> Result<Vec<LancamentoBalanceado>, ErroFinanceiro> {
    let titulo = &tcp.titulo;
    let mut lancamentos = Vec::with_capacity(tcp.parcelas.len());

    for parcela in &tcp.parcelas {
        let historico = format!(
            "Título {} {} — parcela {}/{}",
            titulo.origem_modulo,
            titulo.especie.rotulo(),
            parcela.numero,
            tcp.parcelas.len()
        );
        let mut c = ConstrutorLancamento::novo(titulo.empresa, titulo.emissao, historico)
            .origem(origem_titulo(titulo.id))
            .vencimento(parcela.vencimento)
            .criado_por(autoria.usuario, autoria.dispositivo)
            .agora(autoria.agora);

        c = match titulo.especie {
            EspecieTitulo::Receber => c
                .debitar(contas.contraparte, parcela.valor)
                .contraparte(titulo.contraparte)
                .creditar(contas.resultado, parcela.valor),
            EspecieTitulo::Pagar => c
                .debitar(contas.resultado, parcela.valor)
                .creditar(contas.contraparte, parcela.valor)
                .contraparte(titulo.contraparte),
        };

        lancamentos.push(c.construir().map_err(|_| ErroFinanceiro::ValorInvalido)?);
    }

    Ok(lancamentos)
}

/// Monta o lançamento `Realizado` de uma baixa (`docs/modulos/financeiro.md` §7,
/// "Recebimento/Pagamento de parcela").
///
/// - `Receber`: D conta destino (valor recebido) + D descontos concedidos (desconto)
///   / C contraparte (principal) + C receita financeira (juros + multa).
/// - `Pagar`: D contraparte (principal) + D despesa financeira (juros + multa)
///   / C conta destino (valor pago) + C conta de desconto (desconto por antecipação).
///
/// # Errors
/// [`ErroFinanceiro::BaixaZerada`] se o plano não move dinheiro; propaga desbalanceamento
/// improvável do construtor como [`ErroFinanceiro::ValorInvalido`].
pub fn baixar_parcela(
    especie: EspecieTitulo,
    contraparte: Contraparte,
    parcela: &Parcela,
    plano: &PlanoBaixa,
    contas: ContasBaixa,
    autoria: Autoria,
) -> Result<LancamentoBalanceado, ErroFinanceiro> {
    if !plano.valor_recebido.e_positivo() {
        return Err(ErroFinanceiro::BaixaZerada);
    }
    let encargos = plano.juros + plano.multa;
    let historico = format!(
        "Baixa da parcela {} — {}",
        parcela.numero,
        plano.data.formatar()
    );

    let mut c = ConstrutorLancamento::novo(parcela.empresa, plano.data, historico)
        .origem(origem_titulo(parcela.titulo))
        .liquidacao(plano.data)
        .criado_por(autoria.usuario, autoria.dispositivo)
        .agora(autoria.agora);

    c = match especie {
        EspecieTitulo::Receber => c
            .debitar(contas.conta_destino, plano.valor_recebido)
            .debitar_se(contas.desconto, plano.desconto)
            .creditar(contas.contraparte, plano.principal)
            .contraparte(contraparte)
            .creditar_se(contas.encargos, encargos),
        EspecieTitulo::Pagar => c
            .debitar(contas.contraparte, plano.principal)
            .contraparte(contraparte)
            .debitar_se(contas.encargos, encargos)
            .creditar(contas.conta_destino, plano.valor_recebido)
            .creditar_se(contas.desconto, plano.desconto),
    };

    c.construir().map_err(|_| ErroFinanceiro::ValorInvalido)
}

#[cfg(test)]
mod testes {
    use cardeal_kernel::{Data, Dinheiro, Fuso, Percentual};
    use cardeal_ledger::Contraparte;

    use super::*;
    use crate::titulo::{ConstrutorTitulo, PoliticaJuros};

    fn hoje() -> Data {
        Data::hoje(Fuso::BRASILIA)
    }

    fn autoria() -> Autoria {
        Autoria {
            usuario: Id::novo(),
            dispositivo: Id::novo(),
            agora: Instante::agora(),
            fuso: Fuso::BRASILIA,
        }
    }

    #[test]
    fn lancamento_de_titulo_a_receber_balanceia_por_parcela() {
        let tcp = ConstrutorTitulo::novo(
            Id::novo(),
            EspecieTitulo::Receber,
            Contraparte::Cliente(Id::novo()),
            Dinheiro::reais(90),
            hoje(),
        )
        .parcelas(3, hoje().mais_dias(30), 30)
        .construir()
        .unwrap();

        let contas = ContasTitulo {
            contraparte: Id::novo(),
            resultado: Id::novo(),
        };
        let lancs = lancar_titulo(&tcp, contas, autoria()).unwrap();
        assert_eq!(lancs.len(), 3);
        for l in &lancs {
            assert!(l.interno().esta_balanceado());
            assert_eq!(
                l.interno().estado,
                cardeal_ledger::EstadoLancamento::Confirmado
            );
        }
    }

    #[test]
    fn baixa_com_juros_e_desconto_sempre_fecha() {
        let mut parcela = ConstrutorTitulo::novo(
            Id::novo(),
            EspecieTitulo::Receber,
            Contraparte::Cliente(Id::novo()),
            Dinheiro::reais(100),
            hoje(),
        )
        .juros(PoliticaJuros::SimplesDiario, Percentual::milesimos(100))
        .multa(Percentual::pontos(2))
        .construir()
        .unwrap()
        .parcelas
        .remove(0);
        parcela.vencimento = hoje().mais_dias(-10);

        let plano = parcela
            .planejar_baixa(hoje(), parcela.situacao_em(hoje()).total_devido())
            .unwrap();
        let contas = ContasBaixa {
            conta_destino: Id::novo(),
            contraparte: Id::novo(),
            encargos: Id::novo(),
            desconto: Id::novo(),
        };
        let l = baixar_parcela(
            EspecieTitulo::Receber,
            Contraparte::Cliente(Id::novo()),
            &parcela,
            &plano,
            contas,
            autoria(),
        )
        .unwrap();
        assert!(l.interno().esta_balanceado());
        assert_eq!(
            l.interno().estado,
            cardeal_ledger::EstadoLancamento::Realizado
        );
    }

    #[test]
    fn baixa_a_pagar_em_dia_balanceia() {
        let parcela = ConstrutorTitulo::novo(
            Id::novo(),
            EspecieTitulo::Pagar,
            Contraparte::Fornecedor(Id::novo()),
            Dinheiro::reais(500),
            hoje(),
        )
        .construir()
        .unwrap()
        .parcelas
        .remove(0);

        let plano = parcela
            .planejar_baixa(hoje(), Dinheiro::reais(500))
            .unwrap();
        let contas = ContasBaixa {
            conta_destino: Id::novo(),
            contraparte: Id::novo(),
            encargos: Id::novo(),
            desconto: Id::novo(),
        };
        let l = baixar_parcela(
            EspecieTitulo::Pagar,
            Contraparte::Fornecedor(Id::novo()),
            &parcela,
            &plano,
            contas,
            autoria(),
        )
        .unwrap();
        assert!(l.interno().esta_balanceado());
    }
}
