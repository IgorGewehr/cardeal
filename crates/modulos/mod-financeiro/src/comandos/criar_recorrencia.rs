//! Cria uma regra de recorrência — aluguel, internet, uma assinatura `SaaS` que a empresa paga
//! ou vende por mês. Só grava a regra; nenhum `Titulo` é gerado agora (`docs/modulos/
//! financeiro.md` §5) — isso é trabalho de [`super::materializar_recorrencias_pendentes`],
//! chamado por uma tarefa agendada (ainda não existe um agendador no motor — ver o próprio
//! `src/lib.rs`).

use cardeal_kernel::{Data, Dinheiro, Erro, Id, Resultado, Versao};
use cardeal_ledger::Contraparte;
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::recorrencia::{Periodicidade, Recorrencia, TipoValor};
use crate::repositorio::RepositorioFinanceiro;
use crate::titulo::EspecieTitulo;

/// Cria uma regra de recorrência nova, inativa até `ativa: true`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriarRecorrencia {
    /// Descrição legível: "Aluguel da loja", "Assinatura `SaaS` — Cliente X".
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
    /// Quantas ocorrências entram na média, quando `tipo_valor` é [`TipoValor::Variavel`].
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
    /// A conta do Razão a debitar (Pagar) ou creditar (Receber) como contrapartida —
    /// resultado, escolhida entre as já existentes no plano.
    pub conta_contrapartida: Id,
    /// Centro de custo, quando aplicável.
    pub centro_custo: Option<Id>,
    /// Categoria de relatório, propagada para cada título materializado.
    pub categoria: Option<Id>,
    /// Com quantos dias de antecedência a ocorrência vira `Titulo` real.
    pub antecedencia_geracao_dias: u16,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct RecorrenciaCriada {
    /// A recorrência criada.
    pub recorrencia: Id,
}

impl Comando for CriarRecorrencia {
    type Saida = RecorrenciaCriada;
    const PERMISSAO: &'static str = "financeiro.recorrencia.criar";
    const RISCO: Risco = Risco::Medio;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Validar (domínio puro).
        let recorrencia = Recorrencia {
            id: Id::novo(),
            empresa: ctx.empresa,
            descricao: self.descricao,
            especie: self.especie,
            contraparte: self.contraparte,
            tipo_valor: self.tipo_valor,
            valor_fixo: self.valor_fixo,
            indice: self.indice,
            media_ultimos_n: self.media_ultimos_n,
            periodicidade: self.periodicidade,
            dia_referencia: self.dia_referencia,
            expressao_cron: self.expressao_cron,
            inicio: self.inicio,
            fim: self.fim,
            conta_contrapartida: self.conta_contrapartida,
            centro_custo: self.centro_custo,
            categoria: self.categoria,
            antecedencia_geracao_dias: self.antecedencia_geracao_dias,
            ativa: true,
            versao: Versao::INICIAL,
        };
        recorrencia.validar().map_err(|e| Erro::de_dominio(&e))?;

        // 2. Persistir — nenhum título é gerado agora (ver o doc do módulo).
        RepositorioFinanceiro::novo(uow).inserir_recorrencia(&recorrencia)?;

        Ok(RecorrenciaCriada {
            recorrencia: recorrencia.id,
        })
    }
}
