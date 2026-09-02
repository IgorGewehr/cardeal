//! Erros do sistema.
//!
//! Um erro do Cardeal **nunca** é uma string solta. Ele carrega um código estável, uma
//! mensagem em português e, quando útil, [`Detalhes`] com causa e ações sugeridas — que a
//! interface transforma em botões. Ver `docs/09-protocolo-api.md` §3.

use std::fmt;

use serde::{Deserialize, Serialize};

/// O `Result` do projeto. Use sempre este alias.
pub type Resultado<T, E = Erro> = std::result::Result<T, E>;

/// Código estável de erro. As faixas estão documentadas em `docs/09-protocolo-api.md` §3.
///
/// | Faixa | Categoria |
/// |---|---|
/// | 1xxx | Validação de entrada |
/// | 2xxx | Regra de negócio |
/// | 3xxx | Autorização |
/// | 4xxx | Concorrência |
/// | 5xxx | Infraestrutura |
/// | 6xxx | Integração externa |
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CodigoErro(pub u16);

impl CodigoErro {
    // ── 1xxx — validação ─────────────────────────────────────────────────────
    /// Entrada malformada, sem categoria mais específica.
    pub const ENTRADA_INVALIDA: Self = Self(1001);
    /// Campo obrigatório não preenchido.
    pub const CAMPO_OBRIGATORIO: Self = Self(1003);
    /// Valor numérico ou monetário inválido.
    pub const VALOR_INVALIDO: Self = Self(1004);
    /// Data inválida ou fora da faixa aceita.
    pub const DATA_INVALIDA: Self = Self(1005);
    /// CPF, CNPJ ou inscrição estadual inválido.
    pub const DOCUMENTO_INVALIDO: Self = Self(1006);
    /// Texto excede o tamanho permitido.
    pub const TEXTO_LONGO_DEMAIS: Self = Self(1007);
    /// Valor fora do intervalo permitido.
    pub const FORA_DA_FAIXA: Self = Self(1008);

    // ── 2xxx — regra de negócio ──────────────────────────────────────────────
    /// A operação viola uma regra do domínio.
    pub const REGRA_VIOLADA: Self = Self(2001);
    /// O registro solicitado não existe.
    pub const NAO_ENCONTRADO: Self = Self(2002);
    /// Já existe registro equivalente.
    pub const DUPLICADO: Self = Self(2003);
    /// A transição de estado pedida não é permitida.
    pub const ESTADO_INVALIDO: Self = Self(2004);
    /// Lançamento com débitos diferentes dos créditos.
    pub const RAZAO_DESBALANCEADO: Self = Self(2010);
    /// O período contábil está fechado.
    pub const PERIODO_FECHADO: Self = Self(2011);
    /// Não há caixa aberto para a operação.
    pub const CAIXA_FECHADO: Self = Self(2014);
    /// Saldo de estoque insuficiente.
    pub const ESTOQUE_INSUFICIENTE: Self = Self(2031);
    /// Limite de crédito do cliente excedido.
    pub const CREDITO_EXCEDIDO: Self = Self(2032);

    // ── 3xxx — autorização ───────────────────────────────────────────────────
    /// O usuário não tem a permissão exigida.
    pub const SEM_PERMISSAO: Self = Self(3001);
    /// Sessão expirada ou inválida.
    pub const SESSAO_INVALIDA: Self = Self(3002);
    /// Dispositivo não registrado ou revogado.
    pub const DISPOSITIVO_NAO_AUTORIZADO: Self = Self(3003);
    /// Um limite quantitativo do papel foi excedido (desconto, teto de pagamento).
    pub const LIMITE_EXCEDIDO: Self = Self(3004);
    /// Credenciais incorretas.
    pub const CREDENCIAL_INVALIDA: Self = Self(3005);
    /// Conta temporariamente bloqueada por excesso de tentativas.
    pub const CONTA_BLOQUEADA: Self = Self(3006);

    // ── 4xxx — concorrência ──────────────────────────────────────────────────
    /// O registro foi alterado por outra pessoa desde a leitura.
    pub const VERSAO_DESATUALIZADA: Self = Self(4001);
    /// O recurso está travado por outra sessão.
    pub const RECURSO_TRAVADO: Self = Self(4002);

    // ── 5xxx — infraestrutura ────────────────────────────────────────────────
    /// Falha interna não classificada.
    pub const FALHA_INTERNA: Self = Self(5001);
    /// O banco de dados está indisponível.
    pub const BANCO_INDISPONIVEL: Self = Self(5002);
    /// A operação excedeu o tempo limite.
    pub const TEMPO_ESGOTADO: Self = Self(5003);
    /// Falha de leitura ou escrita em disco.
    pub const FALHA_DE_DISCO: Self = Self(5004);

    // ── 6xxx — integração externa ────────────────────────────────────────────
    /// A API fiscal está indisponível.
    pub const FISCAL_INDISPONIVEL: Self = Self(6101);
    /// A SEFAZ rejeitou o documento.
    pub const FISCAL_REJEITADO: Self = Self(6102);
    /// O TEF recusou a transação.
    pub const TEF_RECUSADO: Self = Self(6203);
    /// Periférico não respondeu.
    pub const PERIFERICO_INDISPONIVEL: Self = Self(6301);

    /// A categoria do código, derivada da faixa.
    pub const fn categoria(self) -> Categoria {
        match self.0 / 1000 {
            1 => Categoria::Validacao,
            2 => Categoria::Negocio,
            3 => Categoria::Autorizacao,
            4 => Categoria::Concorrencia,
            6 => Categoria::Integracao,
            _ => Categoria::Infraestrutura,
        }
    }

    /// Verdadeiro se repetir a operação tal como está pode dar certo.
    pub const fn vale_repetir(self) -> bool {
        matches!(
            self.categoria(),
            Categoria::Infraestrutura | Categoria::Integracao | Categoria::Concorrencia
        )
    }
}

impl fmt::Display for CodigoErro {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}", self.0)
    }
}

/// A categoria de um erro, usada para decidir apresentação e política de retentativa.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Categoria {
    /// O usuário digitou algo inválido.
    Validacao,
    /// A operação contraria uma regra do negócio.
    Negocio,
    /// Falta permissão.
    Autorizacao,
    /// Outra pessoa ou processo interferiu.
    Concorrencia,
    /// Algo do ambiente falhou.
    Infraestrutura,
    /// Um serviço externo falhou.
    Integracao,
}

/// Uma ação que a interface oferece ao usuário como botão na caixa de erro.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcaoSugerida {
    /// O texto do botão. Verbo no infinitivo: "Abrir caixa", "Escolher outro caixa".
    pub rotulo: String,
    /// Identificador da ação, interpretado pela tela que a exibe.
    pub acao: String,
    /// Se verdadeiro, é a ação destacada (botão primário).
    pub primaria: bool,
}

impl AcaoSugerida {
    /// Cria uma ação secundária.
    pub fn nova(rotulo: impl Into<String>, acao: impl Into<String>) -> Self {
        Self {
            rotulo: rotulo.into(),
            acao: acao.into(),
            primaria: false,
        }
    }

    /// Cria a ação primária (destacada).
    pub fn primaria(rotulo: impl Into<String>, acao: impl Into<String>) -> Self {
        Self {
            rotulo: rotulo.into(),
            acao: acao.into(),
            primaria: true,
        }
    }
}

/// O que a interface mostra ao usuário quando algo dá errado.
///
/// Um erro sem `Detalhes` vira uma caixa simples com a mensagem. Um erro com `Detalhes`
/// vira uma conversa: título, explicação e botões que resolvem.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Detalhes {
    /// Frase curta e humana. Vira o título da caixa. Sem jargão técnico.
    pub resumo: String,
    /// O que causou, em português. Vira o corpo.
    pub causa: String,
    /// Ações possíveis. Viram botões.
    pub acoes: Vec<AcaoSugerida>,
}

impl Detalhes {
    /// Cria detalhes com resumo e causa.
    pub fn nova(resumo: impl Into<String>, causa: impl Into<String>) -> Self {
        Self {
            resumo: resumo.into(),
            causa: causa.into(),
            acoes: Vec::new(),
        }
    }

    /// Acrescenta uma ação sugerida.
    #[must_use]
    pub fn com(mut self, acao: AcaoSugerida) -> Self {
        self.acoes.push(acao);
        self
    }
}

/// O erro do Cardeal. Serializável, atravessa o protocolo e chega à interface intacto.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Erro {
    /// Código estável.
    pub codigo: CodigoErro,
    /// Mensagem técnica, sempre em português. É o que vai para o log.
    pub mensagem: String,
    /// Campo do formulário responsável, quando houver — a interface o destaca.
    pub campo: Option<String>,
    /// Apresentação ao usuário final.
    pub detalhes: Option<Detalhes>,
}

impl Erro {
    /// Cria um erro com código e mensagem.
    pub fn novo(codigo: CodigoErro, mensagem: impl Into<String>) -> Self {
        Self {
            codigo,
            mensagem: mensagem.into(),
            campo: None,
            detalhes: None,
        }
    }

    /// Associa o erro a um campo do formulário.
    #[must_use]
    pub fn no_campo(mut self, campo: impl Into<String>) -> Self {
        self.campo = Some(campo.into());
        self
    }

    /// Acrescenta a apresentação ao usuário.
    #[must_use]
    pub fn com_detalhes(mut self, detalhes: Detalhes) -> Self {
        self.detalhes = Some(detalhes);
        self
    }

    /// Constrói a partir de qualquer erro de domínio.
    pub fn de_dominio<E: ErroDominio>(erro: &E) -> Self {
        Self {
            codigo: erro.codigo(),
            mensagem: erro.to_string(),
            campo: erro.campo().map(str::to_owned),
            detalhes: erro.detalhes(),
        }
    }

    /// Atalho para "campo obrigatório não preenchido".
    pub fn obrigatorio(campo: &str) -> Self {
        Self::novo(
            CodigoErro::CAMPO_OBRIGATORIO,
            format!("O campo \"{campo}\" é obrigatório"),
        )
        .no_campo(campo)
    }

    /// Atalho para "registro não encontrado".
    pub fn nao_encontrado(o_que: &str) -> Self {
        Self::novo(
            CodigoErro::NAO_ENCONTRADO,
            format!("{o_que} não encontrado(a)"),
        )
    }

    /// Verdadeiro se repetir a operação pode dar certo.
    pub fn vale_repetir(&self) -> bool {
        self.codigo.vale_repetir()
    }
}

impl fmt::Display for Erro {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.codigo, self.mensagem)
    }
}

impl std::error::Error for Erro {}

/// O que todo erro de módulo implementa para atravessar a fronteira do protocolo.
///
/// ```ignore
/// #[derive(Debug, thiserror::Error)]
/// pub enum ErroFinanceiro {
///     #[error("A parcela já foi quitada em {data}")]
///     ParcelaJaQuitada { data: Data },
/// }
///
/// impl ErroDominio for ErroFinanceiro {
///     fn codigo(&self) -> CodigoErro {
///         match self {
///             Self::ParcelaJaQuitada { .. } => CodigoErro::ESTADO_INVALIDO,
///         }
///     }
/// }
/// ```
pub trait ErroDominio: std::error::Error {
    /// O código estável correspondente.
    fn codigo(&self) -> CodigoErro;

    /// O campo do formulário responsável, se houver.
    fn campo(&self) -> Option<&str> {
        None
    }

    /// A apresentação ao usuário. O padrão é nenhuma, e a interface mostra só a mensagem.
    fn detalhes(&self) -> Option<Detalhes> {
        None
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn categorias_por_faixa() {
        assert_eq!(
            CodigoErro::CAMPO_OBRIGATORIO.categoria(),
            Categoria::Validacao
        );
        assert_eq!(CodigoErro::CAIXA_FECHADO.categoria(), Categoria::Negocio);
        assert_eq!(
            CodigoErro::SEM_PERMISSAO.categoria(),
            Categoria::Autorizacao
        );
        assert_eq!(
            CodigoErro::VERSAO_DESATUALIZADA.categoria(),
            Categoria::Concorrencia
        );
        assert_eq!(
            CodigoErro::BANCO_INDISPONIVEL.categoria(),
            Categoria::Infraestrutura
        );
        assert_eq!(
            CodigoErro::FISCAL_INDISPONIVEL.categoria(),
            Categoria::Integracao
        );
    }

    #[test]
    fn politica_de_retentativa() {
        assert!(CodigoErro::FISCAL_INDISPONIVEL.vale_repetir());
        assert!(CodigoErro::RECURSO_TRAVADO.vale_repetir());
        assert!(!CodigoErro::SEM_PERMISSAO.vale_repetir());
        assert!(!CodigoErro::CAMPO_OBRIGATORIO.vale_repetir());
    }

    #[test]
    fn erro_com_acoes() {
        let e = Erro::novo(CodigoErro::CAIXA_FECHADO, "O caixa está fechado").com_detalhes(
            Detalhes::nova(
                "Não foi possível baixar esta parcela",
                "O caixa \"Caixa 1\" foi fechado às 18:03 por João Silva.",
            )
            .com(AcaoSugerida::primaria(
                "Abrir caixa",
                "financeiro.abrir_caixa",
            ))
            .com(AcaoSugerida::nova(
                "Escolher outro caixa",
                "financeiro.escolher_caixa",
            )),
        );
        assert_eq!(e.detalhes.as_ref().unwrap().acoes.len(), 2);
        assert!(e.to_string().starts_with("[2014]"));
    }
}
