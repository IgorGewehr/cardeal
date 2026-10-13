//! Define (ou tira) o custo-hora de um técnico ou o padrão da oficina — ver
//! [`crate::custo_hora`].

#[cfg(feature = "sqlite")]
use cardeal_kernel::{CodigoErro, Erro, Resultado};
use cardeal_kernel::{Dinheiro, Id};
use cardeal_modkit::Comando;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::Risco;
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

/// Define o custo-hora.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DefinirCustoHora {
    /// O técnico (usuário); `None` = o padrão da oficina.
    pub tecnico: Option<Id>,
    /// Quanto custa uma hora; `None` tira (o técnico volta a usar o padrão).
    pub custo_hora: Option<Dinheiro>,
}

impl Comando for DefinirCustoHora {
    type Saida = ();
    const PERMISSAO: &'static str = "os.custo_hora.definir";
    const RISCO: Risco = Risco::Medio;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        use crate::repositorio::{blob, persist};

        let chave = self.tecnico.map_or_else(Vec::new, blob);
        match self.custo_hora {
            Some(c) if !c.e_positivo() => Err(Erro::novo(
                CodigoErro::FORA_DA_FAIXA,
                "o custo-hora precisa ser maior que zero",
            )),
            Some(c) => {
                uow.conexao()
                    .execute(
                        "INSERT INTO os_custo_hora (empresa, tecnico, custo, atualizado_em)
                         VALUES (?1,?2,?3,?4)
                         ON CONFLICT (empresa, tecnico) DO UPDATE SET
                           custo = excluded.custo, atualizado_em = excluded.atualizado_em",
                        rusqlite::params![
                            blob(ctx.empresa),
                            chave,
                            c.em_centavos(),
                            ctx.agora.em_micros()
                        ],
                    )
                    .map_err(persist)?;
                Ok(())
            }
            None => {
                uow.conexao()
                    .execute(
                        "DELETE FROM os_custo_hora WHERE empresa = ?1 AND tecnico = ?2",
                        rusqlite::params![blob(ctx.empresa), chave],
                    )
                    .map_err(persist)?;
                Ok(())
            }
        }
    }
}
