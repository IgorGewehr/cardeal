//! Recorrência — a regra que projeta obrigações futuras **sem gravar uma linha por
//! ocorrência**.
//!
//! `docs/modulos/financeiro.md` §11.7: uma recorrência mensal de 3 anos não grava 36
//! títulos; ela gera as datas de vencimento em memória para a projeção e só vira `Titulo`
//! real quando falta `antecedencia_geracao_dias` para o vencimento. Domínio puro.

use cardeal_kernel::{Competencia, Data, Dinheiro, Id, Periodo, Versao};
use cardeal_ledger::Contraparte;
use serde::{Deserialize, Serialize};

use crate::erros::ErroFinanceiro;
use crate::titulo::{ConstrutorTitulo, EspecieTitulo, TituloComParcelas};

/// Teto de iterações ao enumerar ocorrências — 100 anos de passos mensais. Protege contra
/// um período absurdo pedido por engano.
const MAX_PASSOS: usize = 1_200;

/// Como o valor de cada ocorrência é determinado.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TipoValor {
    /// Sempre o mesmo valor (`valor_fixo`).
    Fixo,
    /// Corrigido por um índice (`indice`); o valor concreto é resolvido fora do módulo.
    Indexado,
    /// Média das últimas `media_ultimos_n` ocorrências; resolvido fora do módulo.
    Variavel,
}

/// A periodicidade de uma recorrência.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Periodicidade {
    /// Todo mês, no dia `dia_referencia` (limitado ao último dia do mês).
    Mensal,
    /// Toda semana, no dia da semana `dia_referencia` (0 = domingo … 6 = sábado).
    Semanal,
    /// Todo ano, no mesmo mês e dia de `inicio`.
    Anual,
    /// Definida por `expressao_cron` — ainda não suportada pelo domínio (exige um agendador).
    Personalizada,
}

/// A regra de uma obrigação que se repete no tempo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recorrencia {
    /// Identidade.
    pub id: Id,
    /// A empresa.
    pub empresa: Id,
    /// Descrição legível: "Aluguel da loja".
    pub descricao: String,
    /// A receber ou a pagar.
    pub especie: EspecieTitulo,
    /// Com quem — cliente ou fornecedor.
    pub contraparte: Contraparte,
    /// Como o valor é determinado.
    pub tipo_valor: TipoValor,
    /// O valor, quando `tipo_valor` é [`TipoValor::Fixo`].
    pub valor_fixo: Option<Dinheiro>,
    /// O índice de correção, quando `tipo_valor` é [`TipoValor::Indexado`].
    pub indice: Option<String>,
    /// Quantas ocorrências passadas entram na média, quando `tipo_valor` é [`TipoValor::Variavel`].
    pub media_ultimos_n: Option<u16>,
    /// A periodicidade.
    pub periodicidade: Periodicidade,
    /// Dia de referência: dia do mês (mensal) ou dia da semana 0–6 (semanal).
    pub dia_referencia: Option<u8>,
    /// Expressão cron, quando `periodicidade` é [`Periodicidade::Personalizada`].
    pub expressao_cron: Option<String>,
    /// Primeira data em que a regra vale.
    pub inicio: Data,
    /// Última data em que a regra vale (aberta se `None`).
    pub fim: Option<Data>,
    /// A conta do Razão a debitar/creditar como contrapartida.
    pub conta_contrapartida: Id,
    /// Centro de custo, quando aplicável.
    pub centro_custo: Option<Id>,
    /// Com quantos dias de antecedência a ocorrência vira `Titulo` real.
    pub antecedencia_geracao_dias: u16,
    /// Se a regra está ativa.
    pub ativa: bool,
    /// Versão para bloqueio otimista.
    pub versao: Versao,
}

impl Recorrencia {
    /// Verifica a consistência interna da regra.
    ///
    /// # Errors
    /// [`ErroFinanceiro::RegraDeRecorrenciaInvalida`] com a causa concreta.
    pub fn validar(&self) -> Result<(), ErroFinanceiro> {
        use ErroFinanceiro::RegraDeRecorrenciaInvalida as Inv;

        if self.descricao.trim().is_empty() {
            return Err(Inv("a descrição não pode ser vazia"));
        }
        if let Some(fim) = self.fim {
            if fim < self.inicio {
                return Err(Inv("`fim` é anterior a `inicio`"));
            }
        }
        match self.tipo_valor {
            TipoValor::Fixo => match self.valor_fixo {
                Some(v) if v.e_positivo() => {}
                _ => return Err(Inv("valor fixo ausente ou não positivo")),
            },
            TipoValor::Indexado => {
                if self.indice.as_deref().unwrap_or("").trim().is_empty() {
                    return Err(Inv("índice de correção não informado"));
                }
            }
            TipoValor::Variavel => {
                if self.media_ultimos_n.unwrap_or(0) == 0 {
                    return Err(Inv("`media_ultimos_n` precisa ser maior que zero"));
                }
            }
        }
        match self.periodicidade {
            Periodicidade::Mensal => match self.dia_referencia {
                Some(d) if (1..=31).contains(&d) => {}
                None => {}
                Some(_) => return Err(Inv("dia do mês fora de 1..=31")),
            },
            Periodicidade::Semanal => match self.dia_referencia {
                Some(d) if d <= 6 => {}
                None => {}
                Some(_) => return Err(Inv("dia da semana fora de 0..=6")),
            },
            Periodicidade::Anual => {}
            Periodicidade::Personalizada => {
                if self
                    .expressao_cron
                    .as_deref()
                    .unwrap_or("")
                    .trim()
                    .is_empty()
                {
                    return Err(Inv("periodicidade personalizada sem `expressao_cron`"));
                }
            }
        }
        Ok(())
    }

    /// Enumera as datas de vencimento da recorrência que caem dentro de `janela` — sem
    /// materializar nada. Uma recorrência inativa devolve lista vazia.
    ///
    /// # Errors
    /// - Propaga [`Self::validar`].
    /// - [`ErroFinanceiro::RegraDeRecorrenciaInvalida`] para [`Periodicidade::Personalizada`],
    ///   que exige um agendador de cron ainda indisponível.
    pub fn ocorrencias(&self, janela: Periodo) -> Result<Vec<Data>, ErroFinanceiro> {
        self.validar()?;
        if !self.ativa {
            return Ok(Vec::new());
        }

        let inicio = self.inicio.max(janela.de);
        let fim = match self.fim {
            Some(f) => f.min(janela.ate),
            None => janela.ate,
        };
        if fim < inicio {
            return Ok(Vec::new());
        }

        let mut datas = Vec::new();
        match self.periodicidade {
            Periodicidade::Mensal => {
                let dia_ref = self
                    .dia_referencia
                    .unwrap_or_else(|| u8::try_from(self.inicio.dia()).unwrap_or(1));
                let mut comp = inicio.competencia();
                for _ in 0..MAX_PASSOS {
                    let data = dia_do_mes(comp, dia_ref);
                    if data > fim {
                        break;
                    }
                    if data >= inicio && data >= self.inicio {
                        datas.push(data);
                    }
                    comp = comp.proxima();
                }
            }
            Periodicidade::Semanal => {
                let alvo = self
                    .dia_referencia
                    .unwrap_or_else(|| self.inicio.dia_da_semana() as u8);
                let mut data = inicio;
                // Avança até o primeiro dia da semana alvo.
                for _ in 0..7 {
                    if data.dia_da_semana() as u8 == alvo {
                        break;
                    }
                    data = data.mais_dias(1);
                }
                while data <= fim {
                    if data >= self.inicio {
                        datas.push(data);
                    }
                    data = data.mais_dias(7);
                }
            }
            Periodicidade::Anual => {
                let (mes, dia) = (self.inicio.mes(), self.inicio.dia());
                let mut ano = inicio.ano();
                for _ in 0..MAX_PASSOS {
                    let comp = Competencia::nova(ano, mes);
                    let data = dia_do_mes(comp, u8::try_from(dia).unwrap_or(1));
                    if data > fim {
                        break;
                    }
                    if data >= inicio && data >= self.inicio {
                        datas.push(data);
                    }
                    ano += 1;
                }
            }
            Periodicidade::Personalizada => {
                return Err(ErroFinanceiro::RegraDeRecorrenciaInvalida(
                    "periodicidade personalizada ainda não é enumerável pelo domínio",
                ));
            }
        }
        Ok(datas)
    }

    /// A próxima ocorrência que já entrou na janela de geração (`antecedencia_geracao_dias`
    /// a contar de `hoje`), se houver — é o gatilho da tarefa `MaterializarRecorrencia`.
    ///
    /// # Errors
    /// Propaga [`Self::ocorrencias`].
    pub fn proxima_a_materializar(&self, hoje: Data) -> Result<Option<Data>, ErroFinanceiro> {
        let ate = hoje.mais_dias(i32::from(self.antecedencia_geracao_dias));
        Ok(self
            .ocorrencias(Periodo::novo(hoje, ate))?
            .into_iter()
            .next())
    }

    /// Materializa uma ocorrência num `Titulo` de parcela única com vencimento em
    /// `vencimento`. `valor` já vem resolvido (para `Fixo`, use [`Self::valor_de`]).
    ///
    /// # Errors
    /// [`ErroFinanceiro::ValorInvalido`] se o valor não for positivo.
    pub fn materializar(
        &self,
        vencimento: Data,
        valor: Dinheiro,
    ) -> Result<TituloComParcelas, ErroFinanceiro> {
        let mut c = ConstrutorTitulo::novo(
            self.empresa,
            self.especie,
            self.contraparte,
            valor,
            vencimento,
        )
        .origem("financeiro_recorrencia", Some(self.id))
        .observacao(self.descricao.clone())
        .parcelas(1, vencimento, 30);
        if let Some(cc) = self.centro_custo {
            c = c.centro_custo(cc);
        }
        c.construir()
    }

    /// O valor de uma ocorrência para [`TipoValor::Fixo`]. Para `Indexado`/`Variavel` o valor
    /// vem de fora (índice publicado, histórico) — este método devolve erro.
    ///
    /// # Errors
    /// [`ErroFinanceiro::RegraDeRecorrenciaInvalida`] se `tipo_valor` não é `Fixo` ou se o
    /// valor fixo está ausente.
    pub fn valor_de(&self) -> Result<Dinheiro, ErroFinanceiro> {
        match (self.tipo_valor, self.valor_fixo) {
            (TipoValor::Fixo, Some(v)) if v.e_positivo() => Ok(v),
            (TipoValor::Fixo, _) => Err(ErroFinanceiro::RegraDeRecorrenciaInvalida(
                "valor fixo ausente",
            )),
            _ => Err(ErroFinanceiro::RegraDeRecorrenciaInvalida(
                "valor depende de fonte externa (índice ou média)",
            )),
        }
    }
}

/// A data do dia `dia_ref` (limitado ao último dia do mês) na competência dada.
fn dia_do_mes(comp: Competencia, dia_ref: u8) -> Data {
    let ultimo = comp.ultimo_dia();
    let dia = u32::from(dia_ref).clamp(1, ultimo.dia());
    Data::de_ymd(comp.ano(), comp.mes(), dia).unwrap_or(ultimo)
}

#[cfg(test)]
mod testes {
    use cardeal_kernel::{DiaDaSemana, Fuso};
    use proptest::prelude::*;

    use super::*;

    fn hoje() -> Data {
        Data::hoje(Fuso::BRASILIA)
    }

    fn base() -> Recorrencia {
        Recorrencia {
            id: Id::novo(),
            empresa: Id::novo(),
            descricao: "Aluguel da loja".to_string(),
            especie: EspecieTitulo::Pagar,
            contraparte: Contraparte::Fornecedor(Id::novo()),
            tipo_valor: TipoValor::Fixo,
            valor_fixo: Some(Dinheiro::reais(2200)),
            indice: None,
            media_ultimos_n: None,
            periodicidade: Periodicidade::Mensal,
            dia_referencia: Some(10),
            expressao_cron: None,
            inicio: Data::de_ymd(2026, 1, 1).unwrap(),
            fim: None,
            conta_contrapartida: Id::novo(),
            centro_custo: None,
            antecedencia_geracao_dias: 5,
            ativa: true,
            versao: Versao::INICIAL,
        }
    }

    #[test]
    fn mensal_gera_uma_data_por_mes_no_dia_de_referencia() {
        let r = base();
        let jan_a_mar = Periodo::novo(
            Data::de_ymd(2026, 1, 1).unwrap(),
            Data::de_ymd(2026, 3, 31).unwrap(),
        );
        let datas = r.ocorrencias(jan_a_mar).unwrap();
        assert_eq!(
            datas,
            vec![
                Data::de_ymd(2026, 1, 10).unwrap(),
                Data::de_ymd(2026, 2, 10).unwrap(),
                Data::de_ymd(2026, 3, 10).unwrap(),
            ]
        );
    }

    #[test]
    fn mensal_dia_31_cai_no_ultimo_dia_de_fevereiro() {
        let mut r = base();
        r.dia_referencia = Some(31);
        let fev = Periodo::novo(
            Data::de_ymd(2026, 2, 1).unwrap(),
            Data::de_ymd(2026, 2, 28).unwrap(),
        );
        assert_eq!(
            r.ocorrencias(fev).unwrap(),
            vec![Data::de_ymd(2026, 2, 28).unwrap()]
        );
    }

    #[test]
    fn respeita_inicio_e_fim() {
        let mut r = base();
        r.inicio = Data::de_ymd(2026, 2, 15).unwrap();
        r.fim = Some(Data::de_ymd(2026, 4, 5).unwrap());
        let ano = Periodo::novo(
            Data::de_ymd(2026, 1, 1).unwrap(),
            Data::de_ymd(2026, 12, 31).unwrap(),
        );
        // Fev/10 é antes de inicio; abr/10 é depois de fim. Sobra só mar/10.
        assert_eq!(
            r.ocorrencias(ano).unwrap(),
            vec![Data::de_ymd(2026, 3, 10).unwrap()]
        );
    }

    #[test]
    fn inativa_nao_gera_nada() {
        let mut r = base();
        r.ativa = false;
        let ano = Periodo::novo(
            Data::de_ymd(2026, 1, 1).unwrap(),
            Data::de_ymd(2026, 12, 31).unwrap(),
        );
        assert!(r.ocorrencias(ano).unwrap().is_empty());
    }

    #[test]
    fn semanal_gera_no_dia_da_semana_alvo() {
        let mut r = base();
        r.periodicidade = Periodicidade::Semanal;
        // 2026-01-05 é uma segunda-feira.
        r.inicio = Data::de_ymd(2026, 1, 5).unwrap();
        r.dia_referencia = Some(DiaDaSemana::Segunda as u8);
        let jan = Periodo::novo(
            Data::de_ymd(2026, 1, 1).unwrap(),
            Data::de_ymd(2026, 1, 31).unwrap(),
        );
        let datas = r.ocorrencias(jan).unwrap();
        assert_eq!(datas.len(), 4);
        for d in datas {
            assert_eq!(d.dia_da_semana(), DiaDaSemana::Segunda);
        }
    }

    #[test]
    fn personalizada_ainda_nao_e_enumeravel() {
        let mut r = base();
        r.periodicidade = Periodicidade::Personalizada;
        r.expressao_cron = Some("0 0 1 * *".to_string());
        let erro = r
            .ocorrencias(Periodo::novo(hoje(), hoje().mais_dias(90)))
            .unwrap_err();
        assert!(matches!(
            erro,
            ErroFinanceiro::RegraDeRecorrenciaInvalida(_)
        ));
    }

    #[test]
    fn validar_pega_regras_inconsistentes() {
        let mut r = base();
        r.tipo_valor = TipoValor::Fixo;
        r.valor_fixo = None;
        assert!(r.validar().is_err());

        let mut r = base();
        r.tipo_valor = TipoValor::Indexado;
        r.valor_fixo = None;
        r.indice = Some("  ".to_string());
        assert!(r.validar().is_err());

        let mut r = base();
        r.fim = Some(r.inicio.mais_dias(-1));
        assert!(r.validar().is_err());
    }

    #[test]
    fn proxima_a_materializar_respeita_a_antecedencia() {
        let mut r = base();
        r.inicio = Data::de_ymd(2026, 1, 1).unwrap();
        r.antecedencia_geracao_dias = 5;
        // Em 2026-03-04, o vencimento 2026-03-10 ainda está a 6 dias — fora da janela.
        assert_eq!(
            r.proxima_a_materializar(Data::de_ymd(2026, 3, 4).unwrap())
                .unwrap(),
            None
        );
        // Em 2026-03-06, entra (faltam 4 dias).
        assert_eq!(
            r.proxima_a_materializar(Data::de_ymd(2026, 3, 6).unwrap())
                .unwrap(),
            Some(Data::de_ymd(2026, 3, 10).unwrap())
        );
    }

    #[test]
    fn materializar_gera_titulo_de_parcela_unica() {
        let r = base();
        let venc = Data::de_ymd(2026, 3, 10).unwrap();
        let tcp = r.materializar(venc, r.valor_de().unwrap()).unwrap();
        assert_eq!(tcp.parcelas.len(), 1);
        assert_eq!(tcp.parcelas[0].vencimento, venc);
        assert_eq!(tcp.titulo.valor_original, Dinheiro::reais(2200));
        assert_eq!(tcp.titulo.origem_modulo, "financeiro_recorrencia");
        assert_eq!(tcp.titulo.origem_id, Some(r.id));
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(300))]

        /// Toda ocorrência enumerada cai dentro da janela pedida e não antes de `inicio`.
        #[test]
        fn ocorrencias_ficam_na_janela(
            dia_ref in 1u8..=28,
            meses_janela in 1i32..=48,
            offset_inicio in -12i32..=12,
        ) {
            let mut r = base();
            r.dia_referencia = Some(dia_ref);
            r.inicio = Data::de_ymd(2026, 1, 1).unwrap().mais_meses(offset_inicio);
            let janela = Periodo::novo(
                Data::de_ymd(2026, 1, 1).unwrap(),
                Data::de_ymd(2026, 1, 1).unwrap().mais_meses(meses_janela),
            );
            for d in r.ocorrencias(janela).unwrap() {
                prop_assert!(d >= janela.de && d <= janela.ate);
                prop_assert!(d >= r.inicio);
                prop_assert_eq!(u32::from(dia_ref), d.dia());
            }
        }
    }
}
