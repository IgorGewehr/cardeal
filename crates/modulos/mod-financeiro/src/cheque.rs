//! A carteira de cheques recebidos (`docs/modulos/financeiro.md` §5 e §7). Domínio puro.
//!
//! Receber um cheque **dá a baixa** na parcela, mas o dinheiro ainda não entrou: a baixa cai em
//! "Cheques a receber" (1.2.03), não no caixa — D Cheques a receber / C Clientes a receber. O
//! cheque fica `EmCarteira` até o banco compensar (D Banco / C Cheques a receber) ou devolver:
//! a devolução estorna a compensação, se houve, e estorna a baixa, e a parcela volta a ficar em
//! aberto. Cheque pré-datado: `bom_para` diz a partir de quando pode ser depositado.

use cardeal_kernel::{Data, Dinheiro, Id};
use serde::{Deserialize, Serialize};

use crate::erros::ErroFinanceiro;

/// Em que pé está um cheque.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EstadoCheque {
    /// Recebido, ainda não compensado.
    EmCarteira,
    /// O banco compensou: o dinheiro entrou.
    Compensado,
    /// Voltou: a parcela foi reaberta.
    Devolvido,
}

impl EstadoCheque {
    /// O rótulo para o usuário.
    #[must_use]
    pub const fn rotulo(self) -> &'static str {
        match self {
            Self::EmCarteira => "Em carteira",
            Self::Compensado => "Compensado",
            Self::Devolvido => "Devolvido",
        }
    }

    /// O texto guardado no banco.
    #[must_use]
    pub const fn txt(self) -> &'static str {
        match self {
            Self::EmCarteira => "EmCarteira",
            Self::Compensado => "Compensado",
            Self::Devolvido => "Devolvido",
        }
    }

    /// Lê o texto guardado no banco.
    #[must_use]
    pub fn de(s: &str) -> Self {
        match s {
            "Compensado" => Self::Compensado,
            "Devolvido" => Self::Devolvido,
            _ => Self::EmCarteira,
        }
    }
}

/// O que está escrito no cheque.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DadosDoCheque {
    /// O código do banco (3 dígitos: 001, 104, 237…).
    pub banco: String,
    /// A agência.
    pub agencia: String,
    /// A conta.
    pub conta: String,
    /// O número do cheque.
    pub numero: String,
    /// Quem emitiu (o titular da conta).
    pub emitente: String,
    /// CPF ou CNPJ do emitente, se informado.
    pub documento_emitente: Option<String>,
}

fn digitos(s: &str) -> String {
    s.chars().filter(char::is_ascii_digit).collect()
}

impl DadosDoCheque {
    /// Só dígitos no banco, agência, conta, número e documento; banco com 3 dígitos; emitente
    /// obrigatório.
    ///
    /// # Errors
    /// [`ErroFinanceiro::DadosDoChequeInvalidos`] dizendo o que falta.
    pub fn normalizado(self) -> Result<Self, ErroFinanceiro> {
        let banco = digitos(&self.banco);
        let banco = match banco.len() {
            1 | 2 => format!("{banco:0>3}"),
            3 => banco,
            _ => {
                return Err(ErroFinanceiro::DadosDoChequeInvalidos(
                    "o banco tem 3 dígitos (001, 104, 237…)",
                ))
            }
        };
        let (agencia, conta, numero) = (
            digitos(&self.agencia),
            digitos(&self.conta),
            digitos(&self.numero),
        );
        if agencia.is_empty() || conta.is_empty() || numero.is_empty() {
            return Err(ErroFinanceiro::DadosDoChequeInvalidos(
                "informe agência, conta e número do cheque",
            ));
        }
        let emitente = self.emitente.trim().to_owned();
        if emitente.is_empty() {
            return Err(ErroFinanceiro::DadosDoChequeInvalidos(
                "informe quem emitiu o cheque",
            ));
        }
        let documento_emitente = self
            .documento_emitente
            .map(|d| digitos(&d))
            .filter(|d| !d.is_empty());
        if documento_emitente
            .as_ref()
            .is_some_and(|d| d.len() != 11 && d.len() != 14)
        {
            return Err(ErroFinanceiro::DadosDoChequeInvalidos(
                "o documento do emitente é um CPF (11 dígitos) ou CNPJ (14)",
            ));
        }
        Ok(Self {
            banco,
            agencia,
            conta,
            numero,
            emitente,
            documento_emitente,
        })
    }

    /// "Banco 001 · Ag. 1234 · C/C 56789 · nº 000123".
    #[must_use]
    pub fn resumo(&self) -> String {
        format!(
            "Banco {} · Ag. {} · C/C {} · nº {}",
            self.banco, self.agencia, self.conta, self.numero
        )
    }
}

/// Um cheque recebido.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cheque {
    /// Identidade.
    pub id: Id,
    /// A empresa.
    pub empresa: Id,
    /// A parcela que ele pagou.
    pub parcela: Id,
    /// A baixa que ele gerou (em Cheques a receber).
    pub baixa: Id,
    /// O que está escrito nele.
    pub dados: DadosDoCheque,
    /// O valor.
    pub valor: Dinheiro,
    /// Quando foi recebido.
    pub recebido_em: Data,
    /// A partir de quando pode ser depositado (pré-datado); o recebimento, se à vista.
    pub bom_para: Data,
    /// O estado.
    pub estado: EstadoCheque,
    /// A conta onde foi depositado, quando compensado.
    pub conta_deposito: Option<Id>,
    /// Quando compensou.
    pub compensado_em: Option<Data>,
    /// O lançamento da compensação.
    pub lancamento_compensacao: Option<Id>,
    /// Quando voltou.
    pub devolvido_em: Option<Data>,
    /// A alínea da devolução (11, 12, 21…), se o banco informou.
    pub alinea: Option<u8>,
    /// O motivo da devolução, em texto.
    pub motivo: Option<String>,
}

impl Cheque {
    /// Já pode ir ao banco: em carteira e com a data "bom para" alcançada.
    #[must_use]
    pub fn pronto_para_depositar(&self, hoje: Data) -> bool {
        self.estado == EstadoCheque::EmCarteira && self.bom_para <= hoje
    }

    /// `EmCarteira → Compensado`, na conta e data do depósito.
    ///
    /// # Errors
    /// [`ErroFinanceiro::ChequeEmEstadoInvalido`] fora da carteira;
    /// [`ErroFinanceiro::DataDoChequeInvalida`] antes do recebimento.
    pub fn compensar(
        &mut self,
        conta: Id,
        data: Data,
        lancamento: Id,
    ) -> Result<(), ErroFinanceiro> {
        if self.estado != EstadoCheque::EmCarteira {
            return Err(ErroFinanceiro::ChequeEmEstadoInvalido {
                atual: self.estado.rotulo(),
                operacao: "compensar",
            });
        }
        if data < self.recebido_em {
            return Err(ErroFinanceiro::DataDoChequeInvalida(
                "a compensação não pode ser antes do recebimento",
            ));
        }
        self.estado = EstadoCheque::Compensado;
        self.conta_deposito = Some(conta);
        self.compensado_em = Some(data);
        self.lancamento_compensacao = Some(lancamento);
        Ok(())
    }

    /// `EmCarteira | Compensado → Devolvido`.
    ///
    /// # Errors
    /// [`ErroFinanceiro::ChequeEmEstadoInvalido`] se já foi devolvido.
    pub fn devolver(
        &mut self,
        data: Data,
        alinea: Option<u8>,
        motivo: String,
    ) -> Result<(), ErroFinanceiro> {
        if self.estado == EstadoCheque::Devolvido {
            return Err(ErroFinanceiro::ChequeEmEstadoInvalido {
                atual: self.estado.rotulo(),
                operacao: "devolver",
            });
        }
        self.estado = EstadoCheque::Devolvido;
        self.devolvido_em = Some(data);
        self.alinea = alinea;
        self.motivo = Some(motivo);
        Ok(())
    }
}

/// As alíneas de devolução mais comuns (as do Banco Central).
pub const ALINEAS: &[(u8, &str)] = &[
    (11, "Sem fundos — 1ª apresentação"),
    (12, "Sem fundos — 2ª apresentação"),
    (13, "Conta encerrada"),
    (14, "Prática espúria"),
    (20, "Folha cancelada pelo correntista"),
    (21, "Contraordem ou sustação"),
    (22, "Divergência ou falta de assinatura"),
    (25, "Talonário cancelado"),
    (28, "Sustação por furto ou roubo"),
    (31, "Erro formal (data, valor, preenchimento)"),
    (35, "Fraude ou adulteração"),
    (44, "Cheque prescrito"),
];

/// A descrição de uma alínea conhecida.
#[must_use]
pub fn descricao_alinea(alinea: u8) -> Option<&'static str> {
    ALINEAS.iter().find(|(a, _)| *a == alinea).map(|(_, d)| *d)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn dados() -> DadosDoCheque {
        DadosDoCheque {
            banco: "1".into(),
            agencia: "1234-5".into(),
            conta: "56.789-0".into(),
            numero: "000123".into(),
            emitente: "  João Silva ".into(),
            documento_emitente: Some("529.982.247-25".into()),
        }
    }

    fn dia(d: u32) -> Data {
        Data::de_ymd(2026, 10, d).unwrap()
    }

    fn cheque() -> Cheque {
        Cheque {
            id: Id::novo(),
            empresa: Id::novo(),
            parcela: Id::novo(),
            baixa: Id::novo(),
            dados: dados().normalizado().unwrap(),
            valor: Dinheiro::reais(300),
            recebido_em: dia(10),
            bom_para: dia(20),
            estado: EstadoCheque::EmCarteira,
            conta_deposito: None,
            compensado_em: None,
            lancamento_compensacao: None,
            devolvido_em: None,
            alinea: None,
            motivo: None,
        }
    }

    #[test]
    fn dados_normalizados_e_recusas() {
        let d = dados().normalizado().unwrap();
        assert_eq!(
            (d.banco.as_str(), d.agencia.as_str(), d.conta.as_str()),
            ("001", "12345", "567890")
        );
        assert_eq!(d.emitente, "João Silva");
        assert_eq!(d.documento_emitente.as_deref(), Some("52998224725"));
        assert_eq!(d.resumo(), "Banco 001 · Ag. 12345 · C/C 567890 · nº 000123");
        let mut sem_numero = dados();
        sem_numero.numero = " ".into();
        assert!(sem_numero.normalizado().is_err());
        let mut banco_longo = dados();
        banco_longo.banco = "0001".into();
        assert!(banco_longo.normalizado().is_err());
        let mut doc_ruim = dados();
        doc_ruim.documento_emitente = Some("123".into());
        assert!(doc_ruim.normalizado().is_err());
    }

    #[test]
    fn pre_datado_so_vai_ao_banco_no_bom_para() {
        let c = cheque();
        assert!(!c.pronto_para_depositar(dia(19)));
        assert!(c.pronto_para_depositar(dia(20)));
    }

    #[test]
    fn compensar_e_devolver_respeitam_o_estado() {
        let mut c = cheque();
        assert!(
            c.compensar(Id::novo(), dia(9), Id::novo()).is_err(),
            "antes de receber"
        );
        c.compensar(Id::novo(), dia(21), Id::novo()).unwrap();
        assert_eq!(c.estado, EstadoCheque::Compensado);
        assert!(
            c.compensar(Id::novo(), dia(22), Id::novo()).is_err(),
            "de novo"
        );
        assert!(!c.pronto_para_depositar(dia(25)));
        // Compensado e devolvido depois (o banco debita de volta).
        c.devolver(dia(23), Some(11), "Sem fundos".into()).unwrap();
        assert_eq!(c.estado, EstadoCheque::Devolvido);
        assert!(c.devolver(dia(24), None, "de novo".into()).is_err());
        assert_eq!(descricao_alinea(11), Some("Sem fundos — 1ª apresentação"));
        assert_eq!(descricao_alinea(99), None);
    }
}
