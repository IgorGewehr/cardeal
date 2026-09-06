//! Cria um compromisso, verificando sobreposição de recurso antes de gravar
//! (`docs/modulos/agenda.md` §5/§11.1).
//!
//! `os`/`hotelaria` chamam este comando (pelo despacho — `agenda` não expõe uma função `pub`
//! direta como `mod_estoque::registrar_saida_comum`, porque a checagem de conflito precisa
//! rodar dentro do mesmo carregar/validar de sempre, sem atalho) informando `origem_modulo`/
//! `origem_id`; `agenda` nunca interpreta esse contexto (§1).

use cardeal_kernel::{Erro, Id, Instante, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::compromisso::{Compromisso, TipoCompromisso};
use crate::erros::ErroAgenda;
use crate::repositorio::RepositorioAgenda;

/// Cria um compromisso reservando um ou mais recursos.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriarCompromisso {
    /// Título legível.
    pub titulo: String,
    /// Interno ou com cliente.
    pub tipo: TipoCompromisso,
    /// O cliente vinculado (`clientes_pessoa.id`), quando `tipo` é `Cliente`.
    pub cliente: Option<Id>,
    /// Início.
    pub inicio: Instante,
    /// Fim.
    pub fim: Instante,
    /// Os recursos a reservar — pode ser vazio (compromisso sem sala/técnico/equipamento).
    pub recursos: Vec<Id>,
    /// O módulo de origem (`"os"`, `"hotelaria"`), `None` se avulso.
    pub origem_modulo: Option<String>,
    /// O agregado de origem, quando houver.
    pub origem_id: Option<Id>,
    /// Confirmação explícita de que o horário foge da `DisponibilidadeRecurso` cadastrada
    /// (`docs/modulos/agenda.md` §11.2) — sem isto, um horário fora do padrão é recusado.
    pub fora_do_horario_padrao: bool,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct CompromissoAgendado {
    /// O compromisso criado.
    pub compromisso: Id,
}

impl Comando for CriarCompromisso {
    type Saida = CompromissoAgendado;
    const PERMISSAO: &'static str = "agenda.compromisso.criar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Validar (domínio puro): título e período.
        let compromisso = Compromisso::abrir(
            ctx.empresa,
            self.titulo,
            self.tipo,
            self.cliente,
            self.inicio,
            self.fim,
            self.origem_modulo,
            self.origem_id,
            ctx.usuario,
        )
        .map_err(|e| Erro::de_dominio(&e))?;

        // 2. Verificar cada recurso: conflito de horário e (salvo confirmação explícita)
        // disponibilidade cadastrada.
        let repo = RepositorioAgenda::novo(uow);
        for &recurso in &self.recursos {
            let sobrepostos =
                repo.compromissos_do_recurso_sobrepondo(recurso, self.inicio, self.fim)?;
            if let Some(existente) = sobrepostos.first() {
                return Err(Erro::de_dominio(&ErroAgenda::ConflitoDeAgenda {
                    recurso,
                    compromisso_existente: existente.id,
                }));
            }

            if !self.fora_do_horario_padrao {
                let regras = repo.disponibilidade_do_recurso(recurso)?;
                if !regras.is_empty() {
                    let dia = self.inicio.data(ctx.fuso).dia_da_semana() as u8;
                    let min_inicio = minutos_do_dia(self.inicio, ctx.fuso);
                    let min_fim = minutos_do_dia(self.fim, ctx.fuso);
                    let cabe = regras.iter().any(|r| r.cobre(dia, min_inicio, min_fim));
                    if !cabe {
                        return Err(Erro::de_dominio(&ErroAgenda::RecursoIndisponivel {
                            recurso,
                        }));
                    }
                }
            }
        }

        // 3. Persistir.
        RepositorioAgenda::novo(uow).inserir_compromisso(&compromisso, &self.recursos)?;

        // 4. Publicar.
        uow.publicar(crate::eventos::CompromissoCriado {
            compromisso: compromisso.id,
            inicio: compromisso.inicio,
            fim: compromisso.fim,
            recursos: self.recursos,
        })
        .map_err(|e| Erro::de_dominio(&e))?;

        Ok(CompromissoAgendado {
            compromisso: compromisso.id,
        })
    }
}

fn minutos_do_dia(instante: Instante, fuso: cardeal_kernel::Fuso) -> u16 {
    let hora = instante.hora(fuso);
    u16::try_from(hora.hora() * 60 + hora.minuto()).unwrap_or(0)
}
