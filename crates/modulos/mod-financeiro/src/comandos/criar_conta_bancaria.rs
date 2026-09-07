//! Cria uma conta bancária nova, analítica, filha de "1.1" (Disponível).
//!
//! `docs/modulos/financeiro.md` §5. **Decisão**: a conta padrão `1.1.02 "Bancos"` (papel
//! [`PapelConta::Bancos`]) continua existindo e continua sendo o destino padrão de
//! `BaixarPagamento`/`BaixarRecebimento` sem `conta_destino` explícito — este comando não a
//! toca. Cada conta bancária nova entra como irmã dela, direto sob `1.1` (`1.1.05`, `1.1.06`…
//! `CodigoConta::proximo_filho` calcula o próximo código livre), com `papel: None`: ela só é
//! endereçável pelo `Id`, nunca por papel — não há hoje um jeito de resolver "a conta do Banco
//! X" por papel semântico, só listando/escolhendo pelo `Id` na tela.

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_ledger::{Conta, GrupoFluxo, Natureza, PortaRazao, RepositorioRazao};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

/// Cria uma conta bancária nova.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriarContaBancaria {
    /// O nome exibido (ex.: "Nubank", "Banco do Brasil — cc 12345-6").
    pub nome: String,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct ContaBancariaCriada {
    /// A conta criada.
    pub conta: Id,
    /// O código que ela recebeu no plano (ex.: `"1.1.05"`).
    pub codigo: String,
}

impl Comando for CriarContaBancaria {
    type Saida = ContaBancariaCriada;
    const PERMISSAO: &'static str = "financeiro.banco.criar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar: a conta "Disponível" (1.1), pai de toda conta bancária.
        let mut repo = RepositorioRazao::novo(uow);
        let pai_id = repo
            .conta_por_codigo(ctx.empresa, "1.1")
            .map_err(|e| Erro::de_dominio(&e))?
            .ok_or_else(|| Erro::nao_encontrado("conta \"1.1\" (Disponível) no plano"))?;
        let pai = repo
            .info_conta(pai_id)
            .map_err(|e| Erro::de_dominio(&e))?
            .ok_or_else(|| Erro::nao_encontrado("conta \"1.1\" (Disponível) no plano"))?;
        let ultimo_filho = repo
            .ultimo_codigo_filho(ctx.empresa, pai_id)
            .map_err(|e| Erro::de_dominio(&e))?;

        // 2. Validar (domínio puro): o próximo código livre e a conta nova.
        let pai_codigo = cardeal_ledger::CodigoConta::novo(pai.codigo.clone());
        let codigo = pai_codigo.proximo_filho(ultimo_filho.as_ref());
        let conta = Conta::abrir_filha(
            pai_id,
            &pai,
            codigo,
            self.nome,
            Natureza::Ativo,
            Some(GrupoFluxo::Operacional),
            None,
            "financeiro",
        )
        .map_err(|e| Erro::de_dominio(&e))?;

        // 4. Persistir.
        let codigo_txt = conta.codigo.to_string();
        repo.inserir_conta(&conta)
            .map_err(|e| Erro::de_dominio(&e))?;

        Ok(ContaBancariaCriada {
            conta: conta.id,
            codigo: codigo_txt,
        })
    }
}
