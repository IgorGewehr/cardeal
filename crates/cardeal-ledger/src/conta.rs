//! Plano de contas.
//!
//! Ver `docs/05-nucleo-financeiro.md` §3.1 para o plano gerencial completo e a justificativa
//! de cada grupo.

use std::fmt;

use cardeal_kernel::Id;
use serde::{Deserialize, Serialize};

/// A natureza contábil de uma conta. Determina se débito aumenta ou diminui o saldo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Natureza {
    /// Bens e direitos.
    Ativo,
    /// Obrigações.
    Passivo,
    /// Capital, lucros acumulados, retiradas.
    PatrimonioLiquido,
    /// Receitas.
    Receita,
    /// Custos e despesas.
    Despesa,
}

impl Natureza {
    /// Verdadeiro se o débito **aumenta** o saldo desta natureza (Ativo e Despesa).
    /// Verdadeiro/falso já basta para toda a aritmética de saldo: um débito soma
    /// `+valor` se `devedora()`, senão soma `-valor`.
    #[must_use]
    pub const fn devedora(self) -> bool {
        matches!(self, Self::Ativo | Self::Despesa)
    }
}

impl fmt::Display for Natureza {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Ativo => "Ativo",
            Self::Passivo => "Passivo",
            Self::PatrimonioLiquido => "Patrimônio Líquido",
            Self::Receita => "Receita",
            Self::Despesa => "Despesa",
        })
    }
}

/// Se a conta agrupa outras (sintética) ou recebe partida diretamente (analítica).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TipoConta {
    /// Agrupa contas filhas. Nunca recebe partida.
    Sintetica,
    /// Folha da árvore. Único tipo que pode receber lançamento.
    Analitica,
}

/// Classificação de uma conta para o fluxo de caixa. `None` significa que a conta não
/// representa disponibilidade — não entra no Rio do Caixa (`docs/12-ui-ux.md` §6.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GrupoFluxo {
    /// Caixa gerado pela operação do negócio.
    Operacional,
    /// Compra e venda de imobilizado, aplicações.
    Investimento,
    /// Empréstimos, aportes, retiradas de sócio.
    Financiamento,
}

/// Papel semântico de uma conta analítica do plano padrão.
///
/// É o que permite um módulo postar um lançamento sem conhecer o código da conta
/// (`"1.1.01"`) nem o plano de contas daquele tenant específico — ele pede
/// `Contas::papel(PapelConta::Cmv)` e recebe o [`Id`] resolvido. Ver
/// `docs/contratos-internos.md` §3 (`Contas`) e §6 regra 4.
///
/// `#[non_exhaustive]`: módulos futuros (hotelaria, combustível, indústria) vão precisar
/// de papéis próprios: acrescentar uma variante não é breaking change para quem já casa
/// por `match` com braço `_`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum PapelConta {
    // ── disponível ───────────────────────────────────────────────────────────
    /// Caixa físico (por terminal/sessão — resolvido via `Contas::caixa`).
    Caixa,
    /// Conta bancária (resolvido via `Contas::banco`).
    Bancos,
    /// Aplicações financeiras.
    Aplicacoes,
    /// Cartão a compensar, Pix a liquidar.
    ValoresEmTransito,
    // ── créditos ─────────────────────────────────────────────────────────────
    /// Clientes a receber.
    ClientesAReceber,
    /// Cartões a receber (adquirente).
    CartoesAReceber,
    /// Cheques a receber.
    ChequesAReceber,
    /// Adiantamentos a fornecedores.
    AdiantamentoFornecedor,
    /// Impostos a recuperar (crédito de ICMS/PIS/COFINS na compra).
    ImpostosARecuperar,
    // ── estoques ─────────────────────────────────────────────────────────────
    /// Mercadorias para revenda.
    EstoqueMercadorias,
    /// Matéria-prima.
    EstoqueMateriaPrima,
    /// Produtos em processo de fabricação.
    EstoqueEmProcesso,
    /// Produtos acabados.
    EstoqueAcabado,
    // ── obrigações ───────────────────────────────────────────────────────────
    /// Fornecedores a pagar.
    Fornecedores,
    /// Impostos a recolher.
    ImpostosARecolher,
    /// Salários e encargos a pagar.
    SalariosAPagar,
    /// Empréstimos.
    Emprestimos,
    /// Adiantamentos recebidos de clientes.
    AdiantamentoCliente,
    /// Taxas de cartão a repassar à adquirente.
    CartoesARepassar,
    // ── patrimônio líquido ───────────────────────────────────────────────────
    /// Capital social.
    Capital,
    /// Lucros acumulados.
    LucrosAcumulados,
    /// Retiradas de sócio / pró-labore.
    Retiradas,
    // ── receitas ─────────────────────────────────────────────────────────────
    /// Receita de venda de mercadorias.
    ReceitaVendas,
    /// Receita de prestação de serviços.
    ReceitaServicos,
    /// Receita de aluguéis.
    ReceitaAlugueis,
    /// Receita de hospedagem.
    ReceitaHospedagem,
    /// Descontos concedidos (redutora de receita).
    DescontosConcedidos,
    /// Devoluções de venda (redutora de receita).
    DevolucoesVenda,
    /// Juros e outras receitas financeiras.
    ReceitaFinanceira,
    /// Receitas não classificadas nos papéis acima.
    OutrasReceitas,
    // ── custos e despesas ────────────────────────────────────────────────────
    /// Custo da mercadoria vendida.
    Cmv,
    /// Custo do serviço prestado.
    CustoServico,
    /// Despesas com pessoal.
    DespesaPessoal,
    /// Despesas administrativas.
    DespesaAdministrativa,
    /// Despesas comerciais.
    DespesaComercial,
    /// Aluguel, energia, água — ocupação.
    Ocupacao,
    /// Taxas de cartão e outros meios de pagamento.
    TaxasCartao,
    /// Impostos incidentes sobre a venda.
    ImpostosSobreVenda,
    /// Juros e despesas financeiras.
    DespesaFinanceira,
    /// Perdas (evaporação, quebra, obsolescência).
    Perdas,
    /// Quebra de caixa no fechamento.
    QuebraCaixa,
}

impl fmt::Display for PapelConta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

/// Código hierárquico de conta: `"1.1.01.001"`.
///
/// A hierarquia é lida direto do texto — não há coluna separada para "pai": o pai de
/// `"1.1.01.001"` é `"1.1.01"`, obtido removendo o último segmento. Isso mantém o código
/// como única fonte de verdade sobre a posição na árvore.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CodigoConta(String);

impl CodigoConta {
    /// Constrói a partir do texto. Não valida contra um plano existente — isso é
    /// responsabilidade de quem monta o plano (`plano_padrao`) ou do repositório.
    #[must_use]
    pub fn novo(codigo: impl Into<String>) -> Self {
        Self(codigo.into())
    }

    /// O texto do código.
    #[must_use]
    pub fn como_str(&self) -> &str {
        &self.0
    }

    /// A profundidade na árvore, contando os segmentos separados por ponto.
    /// `"1"` tem nível 1, `"1.1.01.001"` tem nível 4.
    #[must_use]
    pub fn nivel(&self) -> u8 {
        u8::try_from(self.0.split('.').count()).unwrap_or(u8::MAX)
    }

    /// O código do pai direto, se houver. `"1.1.01.001"` → `Some("1.1.01")`.
    /// Uma conta de nível 1 (`"1"`) não tem pai.
    #[must_use]
    pub fn pai(&self) -> Option<Self> {
        self.0.rfind('.').map(|pos| Self(self.0[..pos].to_string()))
    }

    /// Verdadeiro se `self` é descendente (direto ou não) de `outro`.
    #[must_use]
    pub fn descende_de(&self, outro: &Self) -> bool {
        self.0.starts_with(outro.0.as_str())
            && self.0.len() > outro.0.len()
            && self.0.as_bytes()[outro.0.len()] == b'.'
    }

    /// O próximo código filho de `self` — usado para abrir uma conta nova em runtime sob uma
    /// sintética existente (`Conta::abrir_filha`). `ultimo_filho` é o maior código filho já
    /// existente (`None` se `self` ainda não tem filhos): o próximo segmento é o número dele
    /// mais um, preservando a largura (`"01"`, `"02"`… até `"99"`, depois `"100"`).
    #[must_use]
    pub fn proximo_filho(&self, ultimo_filho: Option<&Self>) -> Self {
        let Some(ultimo) = ultimo_filho else {
            return Self(format!("{}.01", self.0));
        };
        let ultimo_segmento = ultimo.0.rsplit('.').next().unwrap_or("00");
        let largura = ultimo_segmento.len();
        let numero: u64 = ultimo_segmento.parse().unwrap_or(0) + 1;
        Self(format!("{}.{numero:0largura$}", self.0))
    }
}

impl fmt::Display for CodigoConta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for CodigoConta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CodigoConta({})", self.0)
    }
}

/// Uma conta do plano.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conta {
    /// Identidade.
    pub id: Id,
    /// A empresa dona do plano.
    pub empresa: Id,
    /// O código hierárquico.
    pub codigo: CodigoConta,
    /// O nome exibido.
    pub nome: String,
    /// A natureza contábil.
    pub natureza: Natureza,
    /// Sintética (agrupa) ou analítica (recebe partida).
    pub tipo: TipoConta,
    /// O `Id` da conta pai, se houver.
    pub pai: Option<Id>,
    /// A profundidade na árvore — espelha `codigo.nivel()`.
    pub nivel: u8,
    /// Classificação para o fluxo de caixa. `None` se a conta não é disponibilidade.
    pub grupo_fluxo: Option<GrupoFluxo>,
    /// O papel semântico, quando a conta faz parte do plano padrão.
    pub papel: Option<PapelConta>,
    /// O módulo que criou esta conta, quando não veio do plano padrão.
    pub modulo_origem: Option<String>,
    /// Se falso, a conta não aceita mais lançamento nem edição — mas o histórico permanece.
    pub ativa: bool,
    /// Versão para bloqueio otimista (`docs/03-pilar-resiliencia.md` §4.1).
    pub versao: cardeal_kernel::Versao,
}

impl Conta {
    /// Verdadeiro se a conta pode receber partida diretamente.
    #[must_use]
    pub const fn aceita_lancamento(&self) -> bool {
        matches!(self.tipo, TipoConta::Analitica) && self.ativa
    }

    /// Abre uma conta analítica nova, filha de `pai` — o que um módulo usa para criar contas
    /// em runtime (ex.: uma conta bancária por banco, `docs/modulos/financeiro.md` §5).
    /// `pai` é a informação mínima já disponível via
    /// [`PortaRazao::info_conta`](crate::porta::PortaRazao::info_conta) — pedir o registro
    /// inteiro só para abrir uma filha seria desperdício. `natureza`, `grupo_fluxo` e `papel`
    /// ficam a critério de quem chama (uma conta bancária nova é `Natureza::Ativo`/
    /// `Some(GrupoFluxo::Operacional)`/`papel: None`, por exemplo — o papel `Bancos` já está
    /// mapeado na conta padrão `1.1.02`).
    ///
    /// # Errors
    /// [`ErroRazao::ContaPaiNaoESintetica`] se `pai` não agrupa contas;
    /// [`ErroRazao::ContaPaiInativa`] se `pai` está inativa;
    /// [`ErroRazao::CodigoNaoEhFilhoDoPai`] se `codigo` não é filho direto do código de `pai`.
    #[allow(clippy::too_many_arguments)]
    pub fn abrir_filha(
        pai_id: Id,
        pai: &crate::porta::InfoConta,
        codigo: CodigoConta,
        nome: String,
        natureza: Natureza,
        grupo_fluxo: Option<GrupoFluxo>,
        papel: Option<PapelConta>,
        modulo_origem: impl Into<String>,
    ) -> Result<Self, crate::erros::ErroRazao> {
        if pai.tipo != TipoConta::Sintetica {
            return Err(crate::erros::ErroRazao::ContaPaiNaoESintetica(
                pai.codigo.clone(),
            ));
        }
        if !pai.ativa {
            return Err(crate::erros::ErroRazao::ContaPaiInativa(pai.codigo.clone()));
        }
        let pai_codigo = CodigoConta::novo(pai.codigo.clone());
        if codigo.pai().as_ref() != Some(&pai_codigo) {
            return Err(crate::erros::ErroRazao::CodigoNaoEhFilhoDoPai {
                codigo: codigo.to_string(),
                pai: pai.codigo.clone(),
            });
        }
        Ok(Self {
            id: Id::novo(),
            empresa: pai.empresa,
            nivel: codigo.nivel(),
            codigo,
            nome,
            natureza,
            tipo: TipoConta::Analitica,
            pai: Some(pai_id),
            grupo_fluxo,
            papel,
            modulo_origem: Some(modulo_origem.into()),
            ativa: true,
            versao: cardeal_kernel::Versao::INICIAL,
        })
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn natureza_devedora() {
        assert!(Natureza::Ativo.devedora());
        assert!(Natureza::Despesa.devedora());
        assert!(!Natureza::Passivo.devedora());
        assert!(!Natureza::Receita.devedora());
        assert!(!Natureza::PatrimonioLiquido.devedora());
    }

    #[test]
    fn hierarquia_de_codigo() {
        let folha = CodigoConta::novo("1.1.01.001");
        assert_eq!(folha.nivel(), 4);
        assert_eq!(folha.pai(), Some(CodigoConta::novo("1.1.01")));
        assert_eq!(CodigoConta::novo("1").pai(), None);
        assert_eq!(CodigoConta::novo("1").nivel(), 1);

        assert!(folha.descende_de(&CodigoConta::novo("1.1.01")));
        assert!(folha.descende_de(&CodigoConta::novo("1.1")));
        assert!(folha.descende_de(&CodigoConta::novo("1")));
        assert!(!folha.descende_de(&CodigoConta::novo("1.1.02")));
        assert!(!folha.descende_de(&CodigoConta::novo("1.1.01.001"))); // não descende de si mesma
                                                                       // "1.1.010" não deve ser confundida como descendente de "1.1.01" (prefixo de string
                                                                       // sem separador de segmento não conta).
        assert!(!CodigoConta::novo("1.1.010").descende_de(&CodigoConta::novo("1.1.01")));
    }

    #[test]
    fn proximo_filho_incrementa_preservando_a_largura() {
        let pai = CodigoConta::novo("1.1");
        assert_eq!(pai.proximo_filho(None), CodigoConta::novo("1.1.01"));
        assert_eq!(
            pai.proximo_filho(Some(&CodigoConta::novo("1.1.04"))),
            CodigoConta::novo("1.1.05")
        );
        assert_eq!(
            pai.proximo_filho(Some(&CodigoConta::novo("1.1.99"))),
            CodigoConta::novo("1.1.100")
        );
    }

    fn info_de(codigo: &str, tipo: TipoConta, ativa: bool) -> crate::porta::InfoConta {
        crate::porta::InfoConta {
            empresa: Id::novo(),
            codigo: codigo.to_string(),
            tipo,
            ativa,
        }
    }

    #[test]
    fn abrir_filha_usa_a_natureza_pedida_e_marca_o_modulo_origem() {
        let pai_id = Id::novo();
        let pai = info_de("1.1", TipoConta::Sintetica, true);
        let filha = Conta::abrir_filha(
            pai_id,
            &pai,
            CodigoConta::novo("1.1.05"),
            "Nubank".to_string(),
            Natureza::Ativo,
            Some(GrupoFluxo::Operacional),
            None,
            "financeiro",
        )
        .unwrap();
        assert_eq!(filha.natureza, Natureza::Ativo);
        assert_eq!(filha.tipo, TipoConta::Analitica);
        assert_eq!(filha.pai, Some(pai_id));
        assert_eq!(filha.empresa, pai.empresa);
        assert_eq!(filha.modulo_origem.as_deref(), Some("financeiro"));
        assert!(filha.aceita_lancamento());
    }

    #[test]
    fn abrir_filha_recusa_pai_analitica_inativa_ou_codigo_errado() {
        let sintetica = info_de("1.1", TipoConta::Sintetica, true);
        let analitica = info_de("1.1.01", TipoConta::Analitica, true);
        let inativa = info_de("1.1", TipoConta::Sintetica, false);

        assert!(matches!(
            Conta::abrir_filha(
                Id::novo(),
                &analitica,
                CodigoConta::novo("1.1.01.001"),
                "x".into(),
                Natureza::Ativo,
                None,
                None,
                "financeiro"
            ),
            Err(crate::erros::ErroRazao::ContaPaiNaoESintetica(_))
        ));
        assert!(matches!(
            Conta::abrir_filha(
                Id::novo(),
                &inativa,
                CodigoConta::novo("1.1.05"),
                "x".into(),
                Natureza::Ativo,
                None,
                None,
                "financeiro"
            ),
            Err(crate::erros::ErroRazao::ContaPaiInativa(_))
        ));
        assert!(matches!(
            Conta::abrir_filha(
                Id::novo(),
                &sintetica,
                CodigoConta::novo("1.2.05"),
                "x".into(),
                Natureza::Ativo,
                None,
                None,
                "financeiro"
            ),
            Err(crate::erros::ErroRazao::CodigoNaoEhFilhoDoPai { .. })
        ));
    }
}
