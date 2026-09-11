//! Corrige manualmente início/fim de um apontamento de tempo — para quando o técnico
//! esquece de parar o cronômetro (ou digitou a hora errada). Sempre exige motivo: uma
//! correção pós-fato nunca é silenciosa (mesma disciplina de
//! `mod_financeiro::EstornarBaixa`/`RenegociarTitulo`).

use cardeal_kernel::{Erro, Id, Instante, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::repositorio::RepositorioOs;

/// Ajusta manualmente um apontamento de tempo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AjustarApontamento {
    /// O apontamento a corrigir.
    pub apontamento: Id,
    /// O novo início.
    pub novo_inicio: Instante,
    /// O novo fim.
    pub novo_fim: Instante,
    /// Por que está sendo corrigido — obrigatório, nunca implícito.
    pub motivo: String,
}

impl Comando for AjustarApontamento {
    type Saida = i64;
    const PERMISSAO: &'static str = "os.apontamento.ajustar";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let mut repo = RepositorioOs::novo(uow);
        let mut apontamento = repo
            .buscar_apontamento(self.apontamento)?
            .ok_or_else(|| Erro::nao_encontrado("apontamento de tempo"))?;

        // 2. Validar (domínio puro): exige motivo, fim >= início.
        apontamento
            .ajustar(self.novo_inicio, self.novo_fim, self.motivo)
            .map_err(|e| Erro::de_dominio(&e))?;

        // 3. Persistir.
        let duracao = apontamento.duracao_segundos(self.novo_fim);
        repo.atualizar_apontamento(&apontamento)?;

        Ok(duracao)
    }
}
